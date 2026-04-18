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
//!
//! Unix-socket file perms are forced to `0600` so only the owning user can
//! speak to the daemon.

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
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::collectors::jira;
use crate::{block_service, db, infer, models::Block, repo};

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
        .with_state(state)
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

    // Tighten perms — only the owner may speak to the daemon.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        if let Err(e) = std::fs::set_permissions(path, perms) {
            error!("could not chmod 600 socket: {e}");
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

/// Sentinel type so handlers stay concise.
pub struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let msg = format!("{:#}", self.0);
        error!("api error: {msg}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": msg })),
        )
            .into_response()
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
        .with_context(|| format!("invalid day {}", body.day))?;
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

// ───────────────────────── helpers ─────────────────────────

/// Run a blocking closure with exclusive access to the shared connection.
/// Wraps `spawn_blocking` so sqlite calls don't stall the reactor.
async fn with_conn<F, T>(state: Shared, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    // The connection is held across the closure so we must keep the mutex
    // guard alive. Since rusqlite::Connection is !Send on tokio threads,
    // run the closure on the current task (which already owns the lock
    // future) rather than spawn_blocking.
    let guard = state.conn.lock().await;
    f(&guard)
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
