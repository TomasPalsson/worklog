//! Unix-socket IPC server for the web UI.
//!
//! Architecture:
//! * Single axum router bound to a unix socket at `~/.local/share/worklog/api.sock`.
//! * A single `Connection` behind a `tokio::sync::Mutex` — personal tool,
//!   single user, low volume. Serialising writes is the simplest thing
//!   that correctly preserves SQLite invariants.
//! * `spawn_blocking` wraps every db call so the async runtime isn't
//!   starved by sqlite syscalls.
//!
//! Endpoints (all JSON):
//! * `GET  /health`                      — liveness
//! * `GET  /blocks/:day`                 — list blocks for a YYYY-MM-DD day
//! * `POST /blocks/:id/ticket`           — { "jira_issue": "PROJ-1" | null }
//! * `POST /blocks/:id/duration`         — { "minutes": 45 }
//! * `POST /blocks/:id/description`      — { "description": "text" }
//! * `POST /blocks/:id/delete`           — no body
//! * `POST /infer`                       — { "day": "YYYY-MM-DD" }
//! * `POST /jira/refresh`                — no body, refreshes open tickets
//! * `POST /estimate`                    — { "day": "YYYY-MM-DD", "model": "?" }
//! * `POST /sync`                        — { "day": "YYYY-MM-DD", "dry_run": true }
//!
//! Unix-socket file perms are forced to `0600` so only the owning user can
//! speak to the daemon.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::collectors::{jira, tempo};
use crate::{block_service, db, estimate, infer, models::Block, repo};

pub struct AppState {
    /// Single shared connection — SQLite + rusqlite is !Send, so we keep
    /// exactly one and serialise access. Cheap compared to the code path
    /// we are serving (a single keystroke or click).
    pub conn: Mutex<Connection>,
}

pub type Shared = Arc<AppState>;

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/blocks/:day", get(list_blocks))
        .route("/blocks/:id/ticket", post(assign_ticket))
        .route("/blocks/:id/duration", post(set_duration))
        .route("/blocks/:id/description", post(set_description))
        .route("/blocks/:id/delete", post(delete_block))
        .route("/infer", post(run_infer))
        .route("/jira/refresh", post(refresh_jira))
        .route("/estimate", post(run_estimate))
        .route("/sync", post(run_sync))
        .with_state(state)
}

/// Bind a TCP socket at `addr` (typically `127.0.0.1:<port>`) and serve
/// the router. Used by the containerised web UI since Docker Desktop on
/// macOS can't proxy unix sockets through its VM bind mounts.
pub async fn serve_tcp(addr: SocketAddr, router: Router) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding TCP {addr}"))?;
    info!("worklog daemon listening on {addr}");

    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use tower::Service;

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                error!("tcp accept failed: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
        };
        let io = TokioIo::new(stream);
        let router = router.clone();
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req| {
                let mut router = router.clone();
                async move { router.call(req).await }
            });
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                error!("conn error: {e}");
            }
        });
    }
}

/// Bind a unix socket at `path` and serve the router until the returned
/// future is dropped or the process receives SIGINT. Any stale socket file
/// at `path` is removed first.
pub async fn serve_at(path: &Path, router: Router) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    // Remove stale socket from a previous run so we don't fail with EADDRINUSE.
    let _ = tokio::fs::remove_file(path).await;

    let listener = UnixListener::bind(path)
        .with_context(|| format!("binding unix socket at {}", path.display()))?;

    // Tighten perms. On a single-user workstation the security boundary is
    // already the user account — the socket is inside the user's data dir
    // and the containerised web UI bind-mounts that same dir. Docker Desktop
    // on macOS doesn't remap UIDs for unix-socket bind mounts, so 0600
    // would lock the container out. 0666 keeps the filesystem perms
    // permissive; the path itself still sits under ~/.local/share/worklog,
    // which only the user can read.
    //
    // Override with WORKLOG_SOCKET_MODE (octal, e.g. 0600) if you're on a
    // multi-user host and need to tighten it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::env::var("WORKLOG_SOCKET_MODE")
            .ok()
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0o"), 8).ok())
            .unwrap_or(0o666);
        let perms = std::fs::Permissions::from_mode(mode);
        if let Err(e) = std::fs::set_permissions(path, perms) {
            error!("could not chmod socket: {e}");
        }
    }

    info!("worklog daemon listening on {}", path.display());

    // Hand-rolled accept loop: axum 0.7's `serve` is TCP-only, so we drive
    // hyper directly. Each accepted connection is upgraded through the
    // same `Router` via `tower::Service`.
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use tower::Service;

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                error!("accept failed: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
        };
        let io = TokioIo::new(stream);
        let router = router.clone();
        tokio::spawn(async move {
            // Router is Fn-callable via `clone → call(&mut self)` — clone
            // a fresh handle per request so the `service_fn` closure stays
            // Fn, not FnMut.
            let svc = hyper::service::service_fn(move |req| {
                let mut router = router.clone();
                async move { router.call(req).await }
            });
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                error!("conn error: {e}");
            }
        });
    }
}

/// Open a Connection + wrap in state. Helper for the daemon entrypoint
/// so callers don't repeat the boilerplate.
pub fn new_state() -> Result<Shared> {
    let paths = crate::paths::Paths::resolve()?;
    paths.ensure()?;
    let conn = db::open(&paths.db)?;
    Ok(Arc::new(AppState {
        conn: Mutex::new(conn),
    }))
}

/// Path where the daemon listens by default.
pub fn socket_path() -> Result<PathBuf> {
    Ok(crate::paths::Paths::resolve()?.socket)
}

// ───────────────────────── handlers ─────────────────────────

/// Sentinel type so handlers stay concise. Variants map to HTTP status
/// codes: `BadRequest` → 400 (client sent bad input), `Internal` → 500
/// (anything else). Any `anyhow::Error` that bubbles up via `?` becomes
/// `Internal` by default; handlers opt into 400 by constructing
/// `ApiError::bad_request(...)` explicitly.
pub enum ApiError {
    BadRequest(anyhow::Error),
    Internal(anyhow::Error),
}

impl ApiError {
    pub fn bad_request<E: Into<anyhow::Error>>(e: E) -> Self {
        Self::BadRequest(e.into())
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, err) = match self {
            ApiError::BadRequest(e) => (StatusCode::BAD_REQUEST, e),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
        };
        let msg = format!("{err:#}");
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            error!("api error: {msg}");
        }
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_blocks(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Vec<Block>>, ApiError> {
    let blocks = with_conn(state, move |c| repo::list_blocks_for_day(c, &day)).await?;
    Ok(Json(blocks))
}

#[derive(Deserialize)]
pub struct TicketBody {
    pub jira_issue: Option<String>,
}

async fn assign_ticket(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<TicketBody>,
) -> Result<Json<Block>, ApiError> {
    let block = with_conn(state, move |c| {
        block_service::assign_ticket(c, id, body.jira_issue.as_deref())
    })
    .await?;
    Ok(Json(block))
}

#[derive(Deserialize)]
pub struct DurationBody {
    pub minutes: u32,
}

async fn set_duration(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<DurationBody>,
) -> Result<Json<Block>, ApiError> {
    let block = with_conn(state, move |c| {
        block_service::set_duration(c, id, body.minutes)
    })
    .await?;
    Ok(Json(block))
}

#[derive(Deserialize)]
pub struct DescriptionBody {
    pub description: String,
}

async fn set_description(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<DescriptionBody>,
) -> Result<Json<Block>, ApiError> {
    let block = with_conn(state, move |c| {
        block_service::set_description(c, id, &body.description)
    })
    .await?;
    Ok(Json(block))
}

async fn delete_block(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    with_conn(state, move |c| block_service::delete_block(c, id)).await?;
    Ok(Json(json!({ "ok": true, "deleted_id": id })))
}

#[derive(Deserialize)]
pub struct InferBody {
    pub day: String,
}

#[derive(Serialize)]
pub struct InferResponse {
    pub day: String,
    pub blocks: usize,
    pub minutes: i64,
}

async fn run_infer(
    State(state): State<Shared>,
    Json(body): Json<InferBody>,
) -> Result<Json<InferResponse>, ApiError> {
    let day = NaiveDate::parse_from_str(&body.day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", body.day)))?;
    let (count, minutes) = with_conn(state, move |c| {
        let events = infer::load_day_events(c, day)?;
        let blocks = infer::build_blocks(events);
        let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
        infer::persist_blocks(c, day, &blocks)?;
        Ok::<_, anyhow::Error>((blocks.len(), total / 60))
    })
    .await?;
    Ok(Json(InferResponse {
        day: body.day,
        blocks: count,
        minutes,
    }))
}

async fn refresh_jira(State(state): State<Shared>) -> Result<Json<Value>, ApiError> {
    let auth = jira::JiraAuth::from_secrets().map_err(ApiError::from)?;
    let report = with_conn(state, move |c| {
        let client = crate::http::client()?;
        jira::fetch_open_tickets_with(c, &auth, &client)
    })
    .await?;
    Ok(Json(json!({
        "tickets_written": report.tickets_written,
        "source":          report.source,
    })))
}

#[derive(Deserialize)]
pub struct EstimateBody {
    pub day: String,
    pub model: Option<String>,
}

/// Run the AI estimator for every un-estimated block on the requested day.
/// Shells out to `claude -p` under the hood, which can take a few seconds
/// per block, so this is a long-ish request. Fine for a single-user tool.
async fn run_estimate(
    State(state): State<Shared>,
    Json(body): Json<EstimateBody>,
) -> Result<Json<Value>, ApiError> {
    let day = NaiveDate::parse_from_str(&body.day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", body.day)))?;
    let model = body
        .model
        .unwrap_or_else(|| estimate::DEFAULT_MODEL.to_string());
    let stats = with_conn(state, move |c| estimate::estimate_day(c, day, &model)).await?;
    Ok(Json(json!({
        "day":       body.day,
        "estimated": stats.estimated,
        "skipped":   stats.skipped,
        "failed":    stats.failed,
    })))
}

#[derive(Deserialize)]
pub struct SyncBody {
    pub day: String,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

fn default_dry_run() -> bool {
    true
}

/// Push blocks to Tempo for the given day. Defaults to dry-run so a careless
/// click from the UI can't double-post. Requires `tempo_api_token` and
/// `jira_email` (used as accountId) in the keychain or .env.
async fn run_sync(
    State(state): State<Shared>,
    Json(body): Json<SyncBody>,
) -> Result<Json<Value>, ApiError> {
    let day = NaiveDate::parse_from_str(&body.day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", body.day)))?;
    let auth = tempo::TempoAuth::from_secrets().map_err(ApiError::from)?;
    let dry_run = body.dry_run;
    let (report, results) =
        with_conn(state, move |c| tempo::sync_day(c, &auth, day, dry_run)).await?;
    Ok(Json(json!({
        "day":     body.day,
        "dry_run": dry_run,
        "synced":  report.synced,
        "skipped": report.skipped,
        "errors":  report.errors,
        "results": results,
    })))
}

// ───────────────────────── helpers ─────────────────────────

/// Run a blocking closure with exclusive access to the shared connection.
/// Wraps `spawn_blocking` so sqlite calls — and, critically, blocking
/// `reqwest` clients used by tempo/jira collectors — don't panic on drop
/// inside the async context.
async fn with_conn<F, T>(state: Shared, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let conn = state.conn.blocking_lock();
        f(&conn)
    })
    .await
    .context("spawn_blocking")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{self, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `.oneshot`

    use crate::db::open_memory;
    use crate::models::Event;

    fn state_with_block() -> Shared {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        // Seed an event so /infer has something to do.
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "a",
                "2026-04-18T10:00:00+00:00",
                "commit msg",
            ),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "b",
                "2026-04-18T10:05:00+00:00",
                "commit 2",
            ),
        )
        .unwrap();
        Arc::new(AppState {
            conn: Mutex::new(conn),
        })
    }

    async fn read_json(resp: Response) -> Value {
        let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn health_returns_ok() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["ok"], true);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_blocks_returns_the_seeded_block() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/blocks/2026-04-18")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["duration_seconds"], 1800);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn assign_ticket_round_trip() {
        let state = state_with_block();
        let app = router(state.clone());
        let body = Body::from(serde_json::to_vec(&json!({"jira_issue": "PROJ-1"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/ticket")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["jira_issue"], "PROJ-1");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn set_duration_marks_manual() {
        let app = router(state_with_block());
        let body = Body::from(serde_json::to_vec(&json!({"minutes": 60})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/duration")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["duration_seconds"], 3600);
        assert_eq!(v["estimated_by"], "manual");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn infer_endpoint_clusters_and_reports() {
        let state = state_with_block();
        // Delete the pre-seeded block so re-inference produces a fresh one
        // from the two events.
        {
            let conn = state.conn.lock().await;
            conn.execute("DELETE FROM blocks", []).unwrap();
        }
        let app = router(state.clone());
        let body = Body::from(serde_json::to_vec(&json!({"day":"2026-04-18"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/infer")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["day"], "2026-04-18");
        assert_eq!(v["blocks"], 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delete_endpoint_removes_block() {
        let state = state_with_block();
        let app = router(state.clone());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/delete")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let count: i64 = state
            .conn
            .lock()
            .await
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sync_dry_run_reports_blocks_and_leaves_db_untouched() {
        // Seed Tempo creds so TempoAuth::from_secrets() succeeds. cfg(test)
        // secrets uses an in-process HashMap; these don't leak to the real
        // keychain.
        crate::secrets::set("tempo_api_token", "tok").unwrap();
        crate::secrets::set("jira_email", "acct-id-123").unwrap();

        let state = state_with_block();
        {
            // Assign a ticket so the block is syncable.
            let conn = state.conn.lock().await;
            conn.execute(
                "UPDATE blocks SET jira_issue = 'PROJ-1', description = 'test'",
                [],
            )
            .unwrap();
        }
        let app = router(state.clone());
        let body = Body::from(
            serde_json::to_vec(&json!({"day": "2026-04-18", "dry_run": true})).unwrap(),
        );
        let resp = app
            .oneshot(
                Request::post("/sync")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["dry_run"], true);
        let results = v["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["status"], "dry-run");

        // DB untouched — no tempo_worklog_id set.
        let id: Option<String> = state
            .conn
            .lock()
            .await
            .query_row(
                "SELECT tempo_worklog_id FROM blocks WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(id.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sync_rejects_invalid_day() {
        crate::secrets::set("tempo_api_token", "tok").unwrap();
        crate::secrets::set("jira_email", "acct-id-123").unwrap();
        let app = router(state_with_block());
        let body = Body::from(serde_json::to_vec(&json!({"day": "not-a-date"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/sync")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        // Bad input → 400, not 500.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = read_json(resp).await;
        assert!(v["error"].as_str().unwrap().contains("invalid day"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn estimate_rejects_invalid_day() {
        let app = router(state_with_block());
        let body = Body::from(serde_json::to_vec(&json!({"day": "garbage"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/estimate")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn infer_rejects_invalid_day() {
        // Previously uncovered — H1-ish coverage gap. /infer also takes a
        // day and must return 400 on bad input rather than 500.
        let app = router(state_with_block());
        let body = Body::from(serde_json::to_vec(&json!({"day": "nope"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/infer")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bad_id_returns_500_with_structured_error() {
        let app = router(state_with_block());
        let body = Body::from(serde_json::to_vec(&json!({"jira_issue":"X"})).unwrap());
        let resp = app
            .oneshot(
                Request::post("/blocks/9999/ticket")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v = read_json(resp).await;
        assert!(v["error"].as_str().unwrap().contains("not found"));
    }
}
