//! IPC server (unix socket + optional TCP) for the web UI.
//!
//! Architecture:
//! * Single axum router bound to a unix socket at
//!   `~/.local/share/worklog/api.sock` and, by default, also to
//!   `127.0.0.1:9323` (the dockerised web UI reaches the latter because
//!   Docker Desktop on macOS can't proxy live unix sockets through its
//!   VM bind mounts).
//! * A single `Connection` behind a `tokio::sync::Mutex` — personal tool,
//!   single user, low volume. Serialising writes is the simplest thing
//!   that correctly preserves SQLite invariants.
//! * `spawn_blocking` wraps every db call so the async runtime isn't
//!   starved by sqlite syscalls and blocking reqwest clients drop cleanly.
//!
//! Endpoints (all JSON):
//! * `GET  /health`                      — liveness
//! * `GET  /blocks/:day`                 — list blocks for a YYYY-MM-DD day
//! * `POST /blocks/:id/ticket`           — { "jira_issue": "PROJ-1" | null }
//! * `POST /blocks/:id/duration`         — { "minutes": 45 }
//! * `POST /blocks/:id/description`      — { "description": "text" }
//! * `POST /blocks/:id/delete`           — no body
//! * `POST /blocks/:id/personal`         — { "is_personal": true }
//! * `POST /blocks/:id/split`            — { "first_minutes": 20 }
//! * `POST /blocks/merge`                — { "primary": 1, "absorb": [2,3] }
//! * `POST /blocks/auto-merge`           — { "day": "YYYY-MM-DD" }
//! * `GET  /blocks/:id/commits`          — commits in the window (work only)
//! * `POST /infer`                       — { "day": "YYYY-MM-DD" }
//! * `POST /jira/refresh`                — no body, refreshes open tickets
//! * `GET  /tickets/search?q=&limit=`    — live Jira search (no persistence)
//! * `POST /tickets/external`            — cache a manually-picked ticket
//! * `POST /tickets/create`              — create a Jira issue (sets account)
//! * `GET  /projects`                    — list Jira projects (create picker)
//! * `GET  /accounts`                    — list Tempo accounts (create picker)
//! * `POST /estimate`                    — { "day": "YYYY-MM-DD", "model": "?" }
//! * `POST /sync`                        — { "day": "YYYY-MM-DD", "dry_run": true }
//!
//! Unix-socket file perms default to `0666` so the containerised UI can
//! connect across Docker Desktop's VM (same user, same host — the data
//! dir is the security boundary). Override with `$WORKLOG_SOCKET_MODE`
//! (octal, e.g. `0600`) on multi-user hosts.
//!
//! Errors are split into `ApiError::BadRequest` (→ 400) and `::Internal`
//! (→ 500). Invalid input (e.g. malformed `day`) routes through 400.

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
use tracing::{error, info, warn};

use crate::collectors::{jira, tempo};
use crate::git::{self, CommitEntry};
use crate::personal;
use crate::secrets;
use crate::{
    block_service, db, estimate, infer,
    models::{Block, Event},
    repo,
};

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
        .route("/days/:day", get(day_summary))
        .route("/tickets", get(list_tickets))
        .route("/tickets/search", get(search_tickets))
        .route("/tickets/external", post(record_external_ticket))
        .route("/tickets/create", post(create_ticket))
        .route("/projects", get(list_projects))
        .route("/accounts", get(list_accounts))
        .route("/blocks/:id/events", get(block_events))
        .route("/blocks/:id/commits", get(block_commits))
        .route("/blocks/:id/ticket", post(assign_ticket))
        .route("/blocks/:id/duration", post(set_duration))
        .route("/blocks/:id/description", post(set_description))
        .route("/blocks/:id/delete", post(delete_block))
        .route("/blocks/:id/personal", post(set_personal))
        .route("/blocks/:id/split", post(split_block))
        .route("/blocks/merge", post(merge_blocks))
        .route("/blocks/auto-merge", post(auto_merge))
        .route("/infer", post(run_infer))
        .route("/jira/refresh", post(refresh_jira))
        .route("/estimate", post(run_estimate))
        .route("/sync", post(run_sync))
        .route("/settings", get(get_settings).post(post_settings))
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
        // For 400, emit only the top-level message (no `{:#}` chain
        // walk) so a future handler that wraps e.g. a serde decode
        // error with `ApiError::bad_request` doesn't leak struct-field
        // names or internal type paths to the response body.
        // For 500 we keep the full chain — it goes to the server log
        // via `error!()` where the developer needs it, and the client
        // needs enough context to file a useful bug report.
        let (msg, log_msg) = match status {
            StatusCode::BAD_REQUEST => (format!("{err}"), None),
            _ => (format!("{err:#}"), Some(format!("{err:#}"))),
        };
        if let Some(m) = log_msg {
            error!("api error: {m}");
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

// ───────────────────────── v0.6 read endpoints ─────────────────────────
//
// The web container reads the DB directly via `bun:sqlite` today. That path
// was fast but subtly broken on Docker Desktop — SQLite's WAL shared-memory
// index doesn't sync across the host ↔ VM bind mount, so the container's
// read-only connection could miss writes the daemon just committed. These
// endpoints move reads into the daemon so everyone's on the same connection
// view.

/// Per-block event count + sources, stitched into the day-summary
/// response. Kept here (rather than as a free `serde::Serialize` struct)
/// so the shape stays close to its only caller.
#[derive(Serialize)]
pub struct SourceCount {
    pub source: String,
    pub n: i64,
}

#[derive(Serialize)]
pub struct BlockSummary {
    #[serde(flatten)]
    pub block: Block,
    pub event_count: i64,
    pub sources: Vec<SourceCount>,
    /// Dominant working directory across the block's events — the path
    /// the bulk of its commands ran in. `None` when no event carried a
    /// `project_path` (e.g. a pure calendar block). Surfaced in the
    /// review UI so the user can tell at a glance which repo a block
    /// belongs to.
    pub project_path: Option<String>,
}

#[derive(Serialize)]
pub struct DaySummary {
    pub day: String,
    pub total_seconds: i64,
    pub blocks: Vec<BlockSummary>,
}

#[derive(Serialize)]
pub struct TicketsResponse {
    pub tickets: Vec<crate::models::JiraTicket>,
    pub meta: TicketCacheMeta,
}

#[derive(Serialize)]
pub struct TicketCacheMeta {
    pub count: i64,
    pub last_fetched: Option<String>,
}

async fn day_summary(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<DaySummary>, ApiError> {
    let summary = with_conn(state, move |c| stitch_day_summary(c, &day)).await?;
    Ok(Json(summary))
}

/// Load blocks for a day and enrich each with its event count + per-source
/// breakdown. Kept as a free fn so the daemon handler + tests + any
/// future sync caller share the same aggregation.
fn stitch_day_summary(conn: &Connection, day: &str) -> Result<DaySummary> {
    let blocks = repo::list_blocks_for_day(conn, day)?;
    if blocks.is_empty() {
        return Ok(DaySummary {
            day: day.to_owned(),
            total_seconds: 0,
            blocks: vec![],
        });
    }

    let total_seconds: i64 = blocks.iter().map(|b| b.duration_seconds).sum();

    let ids: Vec<String> = blocks.iter().map(|b| b.id.to_string()).collect();
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Counts + source breakdown in one pair of queries — faster than N
    // round-trips per block.
    let count_sql = format!(
        "SELECT block_id, COUNT(*) FROM block_events
          WHERE block_id IN ({placeholders})
          GROUP BY block_id"
    );
    let mut count_stmt = conn.prepare(&count_sql)?;
    let counts: std::collections::HashMap<i64, i64> = count_stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<_, _>>()?;

    let src_sql = format!(
        "SELECT be.block_id, e.source, COUNT(*)
           FROM block_events be
           JOIN events e ON e.id = be.event_id
          WHERE be.block_id IN ({placeholders})
          GROUP BY be.block_id, e.source
          ORDER BY COUNT(*) DESC"
    );
    let mut src_stmt = conn.prepare(&src_sql)?;
    let mut sources_by_block: std::collections::HashMap<i64, Vec<SourceCount>> =
        std::collections::HashMap::new();
    let rows = src_stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (bid, source, n) = row?;
        sources_by_block
            .entry(bid)
            .or_default()
            .push(SourceCount { source, n });
    }

    // Dominant working directory per block in one query — mirrors
    // `personal::dominant_project_path_for_block` but batched so the day
    // load stays a fixed number of round-trips regardless of block count.
    // For each block, the path that tags the most events wins.
    let path_sql = format!(
        "SELECT be.block_id, e.project_path, COUNT(*)
           FROM block_events be
           JOIN events e ON e.id = be.event_id
          WHERE be.block_id IN ({placeholders})
            AND e.project_path IS NOT NULL
          GROUP BY be.block_id, e.project_path"
    );
    let mut path_stmt = conn.prepare(&path_sql)?;
    let mut best_path: std::collections::HashMap<i64, (String, i64)> =
        std::collections::HashMap::new();
    let path_rows = path_stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in path_rows {
        let (bid, path, n) = row?;
        best_path
            .entry(bid)
            .and_modify(|cur| {
                if n > cur.1 {
                    *cur = (path.clone(), n);
                }
            })
            .or_insert((path, n));
    }

    let enriched = blocks
        .into_iter()
        .map(|block| {
            let id = block.id;
            BlockSummary {
                event_count: counts.get(&id).copied().unwrap_or(0),
                sources: sources_by_block.remove(&id).unwrap_or_default(),
                project_path: best_path.remove(&id).map(|(p, _)| p),
                block,
            }
        })
        .collect();

    Ok(DaySummary {
        day: day.to_owned(),
        total_seconds,
        blocks: enriched,
    })
}

async fn list_tickets(State(state): State<Shared>) -> Result<Json<TicketsResponse>, ApiError> {
    let payload = with_conn(state, |c| {
        let tickets = list_jira_tickets(c)?;
        let meta = jira_cache_meta(c)?;
        Ok(TicketsResponse { tickets, meta })
    })
    .await?;
    Ok(Json(payload))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Live Jira search for tickets the user is not assigned to. Returns the
/// matches directly — nothing is written to the local cache, so the
/// estimator (which reads `external = 0`) keeps seeing only the
/// assignee=currentUser() set.
async fn search_tickets(
    axum::extract::Query(q): axum::extract::Query<SearchQuery>,
) -> Result<Json<Vec<crate::models::JiraTicket>>, ApiError> {
    let query = q.q.trim().to_owned();
    if query.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "query parameter `q` is required"
        )));
    }
    let limit = q.limit.unwrap_or(jira::SEARCH_DEFAULT_LIMIT);
    let auth = jira::JiraAuth::from_secrets().map_err(ApiError::from)?;
    // Same pattern as `refresh_jira`: the blocking reqwest call runs on
    // the blocking pool. No db access, so no `with_conn` needed.
    let results = tokio::task::spawn_blocking(move || -> Result<Vec<crate::models::JiraTicket>> {
        let client = crate::http::client()?;
        jira::search_tickets_with(&auth, &query, limit, &client)
    })
    .await
    .context("spawn_blocking")??;
    Ok(Json(results))
}

/// Record a ticket the user just picked from the in-UI Jira search so
/// the picker can show its summary on subsequent visits. The ticket is
/// stored with `external = 1`, which the estimator filters out — Claude
/// only ever sees the user's actual assignee=currentUser() set.
async fn record_external_ticket(
    State(state): State<Shared>,
    Json(body): Json<crate::models::JiraTicket>,
) -> Result<Json<Value>, ApiError> {
    if body.key.trim().is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "ticket `key` is required"
        )));
    }
    with_conn(state, move |c| repo::upsert_external_ticket(c, &body)).await?;
    Ok(Json(json!({ "ok": true })))
}

/// List the Jira projects the user can see — fills the create-ticket
/// project dropdown. Read-only Jira call; no db access.
async fn list_projects() -> Result<Json<Vec<crate::models::JiraProject>>, ApiError> {
    let auth = jira::JiraAuth::from_secrets().map_err(ApiError::from)?;
    let projects =
        tokio::task::spawn_blocking(move || -> Result<Vec<crate::models::JiraProject>> {
            let client = crate::http::client()?;
            jira::list_projects_with(&auth, &client)
        })
        .await
        .context("spawn_blocking")??;
    Ok(Json(projects))
}

/// List the Tempo accounts — fills the create-ticket account dropdown.
/// The account is the customer mapping, so it's the field that matters
/// most when opening a ticket. Read-only Tempo call; no db access.
async fn list_accounts() -> Result<Json<Vec<crate::models::TempoAccount>>, ApiError> {
    let auth = tempo::TempoAuth::from_secrets().map_err(ApiError::from)?;
    let accounts =
        tokio::task::spawn_blocking(move || -> Result<Vec<crate::models::TempoAccount>> {
            let client = crate::http::client()?;
            tempo::list_accounts_with(&auth, &client)
        })
        .await
        .context("spawn_blocking")??;
    Ok(Json(accounts))
}

#[derive(Deserialize)]
pub struct CreateTicketBody {
    pub project_key: String,
    pub summary: String,
    /// Tempo account id (as a string) the issue's account field is set
    /// to. Optional in the wire shape, but required when an account
    /// field is configured — see the guard below.
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Defaults to `Task` when omitted.
    #[serde(default)]
    pub issue_type: Option<String>,
}

/// Create a Jira issue, set its Tempo account custom field so the
/// ticket's worklogs map to a customer, and cache the result so the
/// picker can render it immediately. The new ticket is stored
/// `external = 1` (same as a manual search pick): visible to the picker,
/// hidden from the estimator's assignee=currentUser() view.
async fn create_ticket(
    State(state): State<Shared>,
    Json(body): Json<CreateTicketBody>,
) -> Result<Json<crate::models::JiraTicket>, ApiError> {
    let project_key = body.project_key.trim().to_owned();
    let summary = body.summary.trim().to_owned();
    if project_key.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "`project_key` is required"
        )));
    }
    if summary.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "`summary` is required"
        )));
    }

    let account_field_id = secrets::get("jira_account_field_id")
        .ok()
        .flatten()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let account_value = body
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    // The account is the whole point — refuse to silently create an
    // unbilled ticket. If the user picked an account but no account
    // field is configured, the value would be dropped on the floor, so
    // 400 with a fix-it pointer rather than create a ticket missing its
    // customer mapping.
    if account_value.is_some() && account_field_id.is_none() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "an account was selected but no Jira account field is configured — \
             set `jira_account_field_id` (e.g. customfield_10100) in \
             Settings → Jira / Tempo so the account maps to a customer"
        )));
    }

    let auth = jira::JiraAuth::from_secrets().map_err(ApiError::from)?;
    let new_issue = jira::NewIssue {
        project_key,
        summary,
        issue_type: body
            .issue_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Task")
            .to_owned(),
        description: body.description,
        account_field_id,
        account_value,
    };
    let ticket = tokio::task::spawn_blocking(move || -> Result<crate::models::JiraTicket> {
        let client = crate::http::client()?;
        jira::create_issue_with(&auth, &new_issue, &client)
    })
    .await
    .context("spawn_blocking")??;

    // Cache so the picker can render the summary on subsequent visits,
    // exactly like a manual external pick.
    let cached = ticket.clone();
    with_conn(state, move |c| repo::upsert_external_ticket(c, &cached)).await?;
    info!(key = %ticket.key, "created jira ticket");
    Ok(Json(ticket))
}

/// Cached Jira tickets, ordered like the existing UI picker: most recently
/// updated first, then alphabetical by key. Mirrors the previous direct
/// SQL in `web/lib/db.ts::listTickets`.
fn list_jira_tickets(conn: &Connection) -> Result<Vec<crate::models::JiraTicket>> {
    let mut stmt = conn.prepare(
        "SELECT key, summary, status, project_key, updated, issue_id
           FROM jira_tickets
          ORDER BY COALESCE(updated, '') DESC, key ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::models::JiraTicket {
            key: r.get(0)?,
            summary: r.get(1)?,
            status: r.get(2)?,
            project_key: r.get(3)?,
            issue_id: r.get(5)?,
            updated: r.get(4)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn jira_cache_meta(conn: &Connection) -> Result<TicketCacheMeta> {
    let (count, last_fetched): (i64, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(fetched_at) FROM jira_tickets",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .context("querying jira_tickets meta")?;
    Ok(TicketCacheMeta {
        count,
        last_fetched,
    })
}

async fn block_events(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Vec<Event>>, ApiError> {
    let events = with_conn(state, move |c| repo::list_events_for_block(c, id)).await?;
    Ok(Json(events))
}

/// Per-block commit sidecar — returns the commits that landed inside
/// the block's `[started_at, ended_at]` window under the block's
/// dominant `project_path`.
///
/// Returns `[]` when the block is personal, has no dominant project
/// path (gcal-only / jira-only blocks), or when shelling out to git
/// fails for any reason. The route is purely additive evidence; soft
/// failure is preferable to surfacing a 500 in the UI.
async fn block_commits(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Vec<CommitEntry>>, ApiError> {
    // Read the block + dominant cwd off the connection on the blocking
    // pool, then release the lock before shelling out to git so a slow
    // repo doesn't stall other API calls.
    let resolved = with_conn(state, move |c| {
        let block =
            repo::get_block(c, id)?.ok_or_else(|| anyhow::anyhow!("block {id} not found"))?;
        if block.is_personal {
            return Ok::<_, anyhow::Error>(None);
        }
        let project_path = personal::dominant_project_path_for_block(c, id)?;
        Ok(project_path.map(|p| (p, block.started_at, block.ended_at)))
    })
    .await?;

    let Some((path, since, until)) = resolved else {
        return Ok(Json(vec![]));
    };

    let commits = git::git_log_in_window(std::path::Path::new(&path), &since, &until)
        .await
        .unwrap_or_default();
    Ok(Json(commits))
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
    let key = body.jira_issue.clone();
    let block = with_conn(state, move |c| {
        block_service::assign_ticket(c, id, body.jira_issue.as_deref())
    })
    .await?;
    info!(
        block_id = id,
        ticket = key.as_deref().unwrap_or("(unassigned)"),
        "assigned ticket"
    );
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
    let minutes = body.minutes;
    let block = with_conn(state, move |c| {
        block_service::set_duration(c, id, body.minutes)
    })
    .await?;
    info!(block_id = id, minutes, "set duration");
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
    let desc_len = body.description.len();
    let block = with_conn(state, move |c| {
        block_service::set_description(c, id, &body.description)
    })
    .await?;
    info!(block_id = id, desc_len, "set description");
    Ok(Json(block))
}

#[derive(Deserialize)]
pub struct PersonalBody {
    pub is_personal: bool,
}

/// Manually flag a block as personal (or pull it back into work). The
/// review UI's per-block toggle. See [`block_service::set_personal`].
async fn set_personal(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<PersonalBody>,
) -> Result<Json<Block>, ApiError> {
    let is_personal = body.is_personal;
    let block = with_conn(state, move |c| {
        block_service::set_personal(c, id, is_personal)
    })
    .await?;
    info!(block_id = id, is_personal, "set personal");
    Ok(Json(block))
}

async fn delete_block(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    // Fetch first so we can clean up Tempo if the block was synced.
    // Doing this before the local delete keeps the two stores in step
    // even when Tempo is down — we'd rather leave the local block in
    // place than have a phantom Tempo entry the user can't see.
    let block = with_conn(state.clone(), move |c| {
        crate::repo::get_block(c, id)?.ok_or_else(|| anyhow::anyhow!("block {id} not found"))
    })
    .await?;

    let mut deleted_tempo_id: Option<String> = None;
    if let Some(tempo_id) = block
        .tempo_worklog_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // Pull TempoAuth from secrets. If the user hasn't configured
        // Tempo credentials, fall back to the local-only delete and
        // warn — they may be cleaning up offline.
        match tempo::TempoAuth::from_secrets() {
            Ok(auth) => {
                let tempo_id = tempo_id.to_owned();
                let auth_clone = auth.clone();
                let tid = tempo_id.clone();
                let res =
                    tokio::task::spawn_blocking(move || tempo::delete_worklog(&auth_clone, &tid))
                        .await
                        .map_err(|e| anyhow::anyhow!("delete task join: {e}"))?;
                match res {
                    Ok(()) => {
                        deleted_tempo_id = Some(tempo_id);
                    }
                    Err(e) => {
                        return Err(ApiError::from(anyhow::anyhow!(
                            "couldn't remove worklog {tempo_id} from Tempo — \
                             local block kept. {e}"
                        )));
                    }
                }
            }
            Err(e) => {
                warn!(
                    block_id = id,
                    tempo_id, error = %e,
                    "no tempo auth — deleting locally but Tempo entry will remain"
                );
            }
        }
    }

    with_conn(state, move |c| block_service::delete_block(c, id)).await?;
    warn!(block_id = id, ?deleted_tempo_id, "deleted block");
    Ok(Json(json!({
        "ok": true,
        "deleted_id": id,
        "deleted_tempo_id": deleted_tempo_id,
    })))
}

#[derive(Deserialize)]
pub struct SplitBody {
    /// Duration in minutes the original block keeps; the rest becomes a
    /// new tail block.
    pub first_minutes: u32,
}

/// Split a block in two at `first_minutes` from its start. See
/// [`block_service::split_block`] for the duration/event/sync semantics.
/// An out-of-range split point is a 400 so the caller can show it verbatim.
async fn split_block(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<SplitBody>,
) -> Result<Json<block_service::SplitOutcome>, ApiError> {
    let minutes = body.first_minutes;
    let outcome = with_conn(state, move |c| block_service::split_block(c, id, minutes))
        .await
        .map_err(ApiError::bad_request)?;
    info!(block_id = id, first_minutes = minutes, "split block");
    Ok(Json(outcome))
}

#[derive(Deserialize)]
pub struct MergeBody {
    /// The surviving block — keeps its ticket, description and tempo id.
    pub primary: i64,
    /// Blocks folded into the primary and then deleted.
    pub absorb: Vec<i64>,
}

/// Merge `absorb` blocks into `primary`. See [`block_service::merge_blocks`]
/// for the duration/event/sync semantics. A bad request (cross-day merge,
/// self-merge, an absorbed block already synced) surfaces as a 400 so the
/// caller can show the message verbatim rather than a generic 500.
async fn merge_blocks(
    State(state): State<Shared>,
    Json(body): Json<MergeBody>,
) -> Result<Json<block_service::MergeOutcome>, ApiError> {
    let primary = body.primary;
    let absorb = body.absorb.clone();
    let outcome = with_conn(state, move |c| {
        block_service::merge_blocks(c, primary, &absorb)
    })
    .await
    .map_err(ApiError::bad_request)?;
    info!(primary = body.primary, absorbed = ?outcome.absorbed, "merged blocks");
    Ok(Json(outcome))
}

#[derive(Deserialize)]
pub struct AutoMergeBody {
    pub day: String,
}

/// Merge every run of adjacent same-ticket blocks on `day`. Delegates to
/// the estimator's merge pass, which safe-skips synced and manually-edited
/// blocks. Returns the number of blocks removed by merging.
async fn auto_merge(
    State(state): State<Shared>,
    Json(body): Json<AutoMergeBody>,
) -> Result<Json<Value>, ApiError> {
    NaiveDate::parse_from_str(&body.day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", body.day)))?;
    let day = body.day.clone();
    let removed = with_conn(state, move |c| {
        estimate::merge_same_ticket_adjacent(c, &day)
    })
    .await?;
    info!(day = %body.day, removed, "auto-merged adjacent same-ticket blocks");
    Ok(Json(json!({ "day": body.day, "removed": removed })))
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
    // Same invoker dance as the CLI: construct an LLM provider for
    // multi-block ticket-day description summaries. We do this inside
    // `with_conn` so the (non-Send) reqwest client lives on the
    // spawn_blocking thread alongside the sqlite Connection.
    let (report, results) = with_conn(state, move |c| {
        let provider = if dry_run {
            None
        } else {
            estimate::resolve_provider().ok()
        };
        let http_client = crate::http::client()?;
        match provider.as_ref() {
            Some(estimate::ProviderChoice::ClaudeSubprocess) => tempo::sync_day_with_invoker(
                c,
                &auth,
                day,
                dry_run,
                &http_client,
                Some(&estimate::ClaudeSubprocess),
                estimate::DEFAULT_MODEL,
            ),
            Some(estimate::ProviderChoice::LiteLLM(inv)) => tempo::sync_day_with_invoker(
                c,
                &auth,
                day,
                dry_run,
                &http_client,
                Some(inv),
                estimate::DEFAULT_MODEL,
            ),
            None => tempo::sync_day_with(c, &auth, day, dry_run, &http_client),
        }
    })
    .await?;
    Ok(Json(json!({
        "day":     body.day,
        "dry_run": dry_run,
        "synced":  report.synced,
        "skipped": report.skipped,
        "errors":  report.errors,
        "results": results,
    })))
}

// ───────────────────────── settings ─────────────────────────
//
// One read endpoint (`GET /settings`) and one write endpoint
// (`POST /settings`) back the review UI's settings panel. The write is a
// partial update: any of the three groups (personal patterns, secrets,
// timezone) may be omitted and is then left untouched.

/// A credential/config key as the settings panel sees it. Token-like
/// keys never echo their stored value — only whether one is present.
/// Non-sensitive keys (emails, URLs, model names, provider choice) come
/// back in full so the form can prefill the current value.
#[derive(Serialize)]
pub struct SettingField {
    pub key: &'static str,
    pub present: bool,
    pub sensitive: bool,
    pub value: Option<String>,
}

#[derive(Serialize)]
pub struct PersonalPatterns {
    pub work: Vec<String>,
    pub personal: Vec<String>,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub personal: PersonalPatterns,
    pub secrets: Vec<SettingField>,
    pub timezone: String,
    pub personal_config_path: Option<String>,
}

/// Token-like keys whose value must never be serialised to the browser.
/// Everything else (emails, base URLs, account ids, usernames, the
/// estimator provider choice, model names) is safe to echo so the form
/// can show what's configured.
fn is_sensitive_secret(key: &str) -> bool {
    matches!(
        key,
        "jira_api_token"
            | "tempo_api_token"
            | "github_token"
            | "google_client_secret"
            | "google_refresh_token"
            | "anthropic_api_key"
            | "litellm_api_key"
    )
}

fn current_settings() -> Result<SettingsView> {
    let cfg_path = personal::config_path();
    let file = cfg_path
        .as_deref()
        .map(personal::read_file)
        .unwrap_or_default();
    let secrets = secrets::KNOWN_KEYS
        .iter()
        .map(|&k| {
            let stored = secrets::get(k).ok().flatten().filter(|s| !s.is_empty());
            let sensitive = is_sensitive_secret(k);
            SettingField {
                key: k,
                present: stored.is_some(),
                sensitive,
                value: if sensitive { None } else { stored },
            }
        })
        .collect();
    Ok(SettingsView {
        personal: PersonalPatterns {
            work: file.work,
            personal: file.personal,
        },
        secrets,
        timezone: crate::tz::configured_tz().unwrap_or_default(),
        personal_config_path: cfg_path.map(|p| p.display().to_string()),
    })
}

async fn get_settings() -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(current_settings()?))
}

#[derive(Deserialize)]
pub struct PersonalPatternsIn {
    #[serde(default)]
    pub work: Vec<String>,
    #[serde(default)]
    pub personal: Vec<String>,
}

#[derive(Deserialize)]
pub struct SettingsUpdate {
    /// Replace the whole personal.toml work/personal lists. `None` leaves
    /// classification untouched; `Some` rewrites the file wholesale.
    pub personal: Option<PersonalPatternsIn>,
    /// Map of secret key → value. Only keys present here are touched; an
    /// empty string deletes the key. Unknown keys are ignored.
    #[serde(default)]
    pub secrets: std::collections::HashMap<String, String>,
    /// Fixed-offset timezone (e.g. `+01:00`, `UTC`). `None` leaves as-is.
    pub timezone: Option<String>,
}

#[derive(Serialize)]
pub struct SettingsSaveResponse {
    #[serde(flatten)]
    pub settings: SettingsView,
    /// Present only when classification patterns changed and existing
    /// blocks were reclassified against the new rules.
    pub reclassified: Option<personal::ReclassifyStats>,
}

async fn post_settings(
    State(state): State<Shared>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<SettingsSaveResponse>, ApiError> {
    // 1. Timezone → .env. Validate as a fixed offset first so a typo
    //    returns 400 instead of silently bucketing days in UTC later.
    if let Some(tz) = body.timezone.as_deref().map(str::trim) {
        if !crate::tz::is_valid_tz(tz) {
            return Err(ApiError::bad_request(anyhow::anyhow!(
                "`{tz}` is not a fixed offset — use +HH:MM, -HH:MM, or UTC \
                 (named zones like America/New_York are not supported)"
            )));
        }
        crate::envfile::upsert("WORKLOG_TZ", tz)?;
    }

    // 2. Secrets → OS keychain. Empty value deletes. Unknown keys are
    //    refused so a stale client can't scribble arbitrary entries.
    for (k, v) in &body.secrets {
        if !secrets::KNOWN_KEYS.contains(&k.as_str()) {
            warn!(key = %k, "ignoring unknown secret key in settings update");
            continue;
        }
        if v.is_empty() {
            secrets::delete(k)?;
        } else {
            secrets::set(k, v)?;
        }
    }

    // 3. Personal patterns → personal.toml, then reclassify existing
    //    blocks so the change shows up immediately, not just on next infer.
    let mut reclassified = None;
    if let Some(p) = body.personal {
        let path = personal::config_path()
            .ok_or_else(|| anyhow::anyhow!("no config dir — can't write personal.toml"))?;
        let file = personal::ConfigFile {
            work: clean_globs(p.work),
            personal: clean_globs(p.personal),
        };
        personal::write_file(&path, &file)?;
        let stats = with_conn(state, move |c| personal::reclassify_blocks(c, None)).await?;
        reclassified = Some(stats);
    }

    info!("settings updated");
    Ok(Json(SettingsSaveResponse {
        settings: current_settings()?,
        reclassified,
    }))
}

/// Trim each glob and drop blank entries — the textarea UI sends one
/// pattern per line and trailing empty lines are common.
fn clean_globs(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
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
    use rusqlite::params;
    use tower::ServiceExt; // for `.oneshot`

    use crate::db::open_memory;
    use crate::models::{Event, JiraTicket};

    fn state_with_block() -> Shared {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        // Seed two events and link them to the block so tests of the
        // new /days/:day and /blocks/:id/events endpoints have real
        // rows to assert on.
        let e1 = repo::upsert_event(
            &conn,
            &Event::minimal(
                "github_commit",
                "a",
                "2026-04-18T09:05:00+00:00",
                "commit msg",
            ),
        )
        .unwrap();
        let e2 = repo::upsert_event(
            &conn,
            &Event::minimal(
                "claude",
                "b",
                "2026-04-18T09:10:00+00:00",
                "UserPromptSubmit — fix oauth",
            ),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2), (?1, ?3)",
            params![bid, e1, e2],
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
    async fn create_ticket_rejects_blank_project() {
        // Validation runs before any Jira call, so this needs no network
        // or secrets — a blank project_key is a 400, not a 500.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/tickets/create")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"project_key":"  ","summary":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ─────────────────── v0.6 read endpoints ───────────────────

    #[tokio::test(flavor = "current_thread")]
    async fn day_summary_returns_blocks_with_counts_sources_and_total() {
        // B1: the new `/days/:day` endpoint is the single read path for the
        // web container. One round-trip returns everything needed to render
        // a day — blocks enriched with event_count + sources, plus the
        // total seconds for the day header.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/days/2026-04-18")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["day"], "2026-04-18");
        assert_eq!(v["total_seconds"], 1800);
        let blocks = v["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["event_count"], 2);
        // Both sources should appear. Order isn't guaranteed, so check
        // membership and aggregate rather than positional equality.
        let sources = blocks[0]["sources"].as_array().unwrap();
        let src_set: std::collections::HashSet<&str> = sources
            .iter()
            .map(|s| s["source"].as_str().unwrap())
            .collect();
        assert!(src_set.contains("github_commit"));
        assert!(src_set.contains("claude"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn day_summary_zero_total_when_no_blocks_on_day() {
        // B20: a day with no blocks returns an empty blocks array and
        // total_seconds=0 — not an error. The web empty-state renders
        // this shape.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/days/2099-01-01")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["total_seconds"], 0);
        assert_eq!(v["blocks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn day_summary_reports_dominant_project_path_per_block() {
        // The review UI shows the directory a block's commands mostly ran
        // in. stitch_day_summary must return, per block, the project_path
        // that tags the most events — and None when no event carried one.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        // Two events under ~/work/api, one under ~/work/web — api wins.
        for (i, path) in [
            Some("/home/u/work/api"),
            Some("/home/u/work/api"),
            Some("/home/u/work/web"),
        ]
        .iter()
        .enumerate()
        {
            let mut ev = Event::minimal(
                "claude",
                format!("e{i}").as_str(),
                "2026-04-18T09:05:00+00:00",
                "prompt",
            );
            ev.project_path = path.map(|s| s.to_string());
            let eid = repo::upsert_event(&conn, &ev).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }

        let summary = stitch_day_summary(&conn, "2026-04-18").unwrap();
        assert_eq!(summary.blocks.len(), 1);
        assert_eq!(
            summary.blocks[0].project_path.as_deref(),
            Some("/home/u/work/api"),
            "dominant path should be the one tagging the most events"
        );
    }

    #[test]
    fn day_summary_project_path_is_none_without_cwd() {
        // Blocks whose events carry no project_path (gcal / pure github)
        // come back with project_path = None, not an empty string.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        // Event::minimal leaves project_path = None.
        let eid = repo::upsert_event(
            &conn,
            &Event::minimal("gcal", "m", "2026-04-18T09:05:00+00:00", "standup"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![bid, eid],
        )
        .unwrap();

        let summary = stitch_day_summary(&conn, "2026-04-18").unwrap();
        assert_eq!(summary.blocks.len(), 1);
        assert_eq!(summary.blocks[0].project_path, None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_post_rejects_invalid_timezone() {
        // A named zone isn't a fixed offset — the handler must 400 before
        // persisting it, so the user never silently falls back to UTC.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"timezone":"America/New_York"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = read_json(resp).await;
        assert!(
            v["error"].as_str().unwrap().contains("fixed offset"),
            "error should explain the constraint: {v}"
        );
    }

    #[test]
    fn settings_view_masks_sensitive_secrets_and_lists_all_keys() {
        // GET /settings must surface every known key and never echo a
        // token value to the browser — only whether one is stored.
        let view = current_settings().unwrap();
        assert_eq!(view.secrets.len(), secrets::KNOWN_KEYS.len());
        for f in &view.secrets {
            if f.sensitive {
                assert!(
                    f.value.is_none(),
                    "sensitive key {} must not echo its value",
                    f.key
                );
            }
        }
        let token = view
            .secrets
            .iter()
            .find(|f| f.key == "jira_api_token")
            .unwrap();
        assert!(token.sensitive, "api token must be sensitive");
        let email = view.secrets.iter().find(|f| f.key == "jira_email").unwrap();
        assert!(!email.sensitive, "email is not a secret value");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn block_events_returns_ordered_events_for_block() {
        // B2: /blocks/:id/events returns events in started_at order so
        // the UI drill-down reads as a timeline.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/blocks/1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["source"], "github_commit");
        assert_eq!(arr[1]["source"], "claude");
        assert!(arr[0]["started_at"].as_str().unwrap() < arr[1]["started_at"].as_str().unwrap());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn block_events_returns_empty_for_block_with_no_linked_events() {
        // An orphan block — exists but has no rows in block_events. The
        // drill-down should render a "no events" empty state rather than
        // error out.
        let state = state_with_block();
        {
            let conn = state.conn.lock().await;
            conn.execute(
                "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
                 VALUES ('2026-04-18', '2026-04-18T11:00:00+00:00', '2026-04-18T11:15:00+00:00', 900)",
                [],
            )
            .unwrap();
        }
        let app = router(state);
        let resp = app
            .oneshot(
                Request::get("/blocks/2/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_tickets_returns_cached_tickets_with_meta() {
        // B3: /tickets returns the cached Jira tickets + cache meta in
        // one response so the web combobox can render the empty state
        // (no cache yet) or the hydrated list.
        let state = state_with_block();
        {
            let conn = state.conn.lock().await;
            repo::upsert_ticket(
                &conn,
                &JiraTicket {
                    key: "PROJ-1".into(),
                    summary: "fix login".into(),
                    status: Some("In Progress".into()),
                    project_key: Some("PROJ".into()),
                    updated: Some("2026-04-18T10:00:00Z".into()),
                    issue_id: None,
                },
            )
            .unwrap();
            repo::upsert_ticket(
                &conn,
                &JiraTicket {
                    key: "PROJ-2".into(),
                    summary: "add signup".into(),
                    status: None,
                    project_key: Some("PROJ".into()),
                    updated: None,
                    issue_id: None,
                },
            )
            .unwrap();
        }
        let app = router(state);
        let resp = app
            .oneshot(Request::get("/tickets").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["tickets"].as_array().unwrap().len(), 2);
        assert_eq!(v["meta"]["count"], 2);
        // At least one of the two should carry a non-null last_fetched
        // (schema defaults fetched_at on insert).
        assert!(v["meta"]["last_fetched"].is_string());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_tickets_returns_empty_when_cache_is_cold() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(Request::get("/tickets").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["tickets"].as_array().unwrap().len(), 0);
        assert_eq!(v["meta"]["count"], 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_tickets_rejects_empty_query() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/tickets/search?q=%20%20")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = read_json(resp).await;
        assert!(
            v["error"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("q"),
            "expected error to name the missing param, got {v}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn record_external_ticket_persists_with_external_flag() {
        let state = state_with_block();
        let app = router(state.clone());
        let body = Body::from(
            serde_json::to_vec(&json!({
                "key": "EXT-42",
                "summary": "external pick",
                "status": "To Do",
                "project_key": "EXT",
                "updated": "2026-04-18T11:00:00Z",
                "issue_id": null
            }))
            .unwrap(),
        );
        let resp = app
            .oneshot(
                Request::post("/tickets/external")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Verify the row landed with external=1 and is visible to
        // list_tickets but invisible to the estimator's load_open_tickets.
        let conn = state.conn.lock().await;
        let external: i64 = conn
            .query_row(
                "SELECT external FROM jira_tickets WHERE key = 'EXT-42'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(external, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn record_external_ticket_rejects_empty_key() {
        let app = router(state_with_block());
        let body = Body::from(
            serde_json::to_vec(&json!({
                "key": "   ",
                "summary": "x"
            }))
            .unwrap(),
        );
        let resp = app
            .oneshot(
                Request::post("/tickets/external")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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
            // Seed numeric issue_id so resolve_issue_id doesn't have to
            // call out to a real Jira instance.
            repo::set_ticket_issue_id(&conn, "PROJ-1", "10000").unwrap();
        }
        let app = router(state.clone());
        let body =
            Body::from(serde_json::to_vec(&json!({"day": "2026-04-18", "dry_run": true})).unwrap());
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

    // ─────────────────── /blocks/:id/commits ───────────────────

    #[tokio::test(flavor = "current_thread")]
    async fn block_commits_returns_empty_when_block_has_no_project_path() {
        // state_with_block seeds events without a project_path, so the
        // dominant lookup returns None — the handler must short-circuit
        // to [] without invoking git.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/blocks/1/commits")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn block_commits_returns_empty_for_personal_blocks() {
        // is_personal blocks must not invoke git regardless of cwd —
        // personal work is opaque on purpose.
        let state = state_with_block();
        {
            let conn = state.conn.lock().await;
            conn.execute("UPDATE blocks SET is_personal = 1 WHERE id = 1", [])
                .unwrap();
        }
        let app = router(state);
        let resp = app
            .oneshot(
                Request::get("/blocks/1/commits")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn block_commits_lists_real_commits_inside_window() {
        // Skip when no git binary is present (CI minimal images).
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        seed_git_repo(tmp.path());

        let state = state_with_block();
        {
            let conn = state.conn.lock().await;
            // Widen the block window so it spans the seeded commits.
            conn.execute(
                "UPDATE blocks
                    SET started_at = '2026-05-01T00:00:00+00:00',
                        ended_at   = '2026-05-31T23:59:59+00:00'
                  WHERE id = 1",
                [],
            )
            .unwrap();
            // Attach the cwd to the seeded events so dominant_project_path
            // resolves to our temp repo.
            let path = tmp.path().to_string_lossy().into_owned();
            conn.execute("UPDATE events SET project_path = ?1", params![path])
                .unwrap();
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::get("/blocks/1/commits")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["subject"], "second");
        assert_eq!(arr[1]["subject"], "first");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn block_commits_returns_500_when_block_missing() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::get("/blocks/9999/commits")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// Build a state with two blocks; `days` picks the day for each.
    fn state_with_two_blocks(day_a: &str, day_b: &str) -> (Shared, i64, i64) {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES (?1, ?1 || 'T09:00:00+00:00', ?1 || 'T09:30:00+00:00', 1800)",
            params![day_a],
        )
        .unwrap();
        let a = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES (?1, ?1 || 'T10:00:00+00:00', ?1 || 'T10:30:00+00:00', 1800)",
            params![day_b],
        )
        .unwrap();
        let b = conn.last_insert_rowid();
        (
            Arc::new(AppState {
                conn: Mutex::new(conn),
            }),
            a,
            b,
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn merge_endpoint_folds_blocks_and_returns_outcome() {
        let (state, a, b) = state_with_two_blocks("2026-04-18", "2026-04-18");
        let app = router(state);
        let resp = app
            .oneshot(
                Request::post("/blocks/merge")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"primary":{a},"absorb":[{b}]}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["merged"]["id"], a);
        assert_eq!(v["merged"]["duration_seconds"], 3600);
        assert_eq!(v["absorbed"].as_array().unwrap(), &vec![Value::from(b)]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn merge_endpoint_rejects_cross_day_with_400() {
        let (state, a, b) = state_with_two_blocks("2026-04-18", "2026-04-19");
        let app = router(state);
        let resp = app
            .oneshot(
                Request::post("/blocks/merge")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"primary":{a},"absorb":[{b}]}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = read_json(resp).await;
        assert!(
            v["error"].as_str().unwrap().contains("same day"),
            "got: {v}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn personal_endpoint_toggles_the_flag() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/personal")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"is_personal":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["id"], 1);
        assert_eq!(v["is_personal"], true);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn split_endpoint_divides_a_block() {
        // state_with_block seeds one 1800s (30m) block as id 1.
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/split")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"first_minutes":10}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["first"]["id"], 1);
        assert_eq!(v["first"]["duration_seconds"], 600);
        assert_eq!(v["second"]["duration_seconds"], 1200);
        assert!(v["second"]["id"].as_i64().unwrap() > 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn split_endpoint_rejects_out_of_range_with_400() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/blocks/1/split")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"first_minutes":99}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_merge_endpoint_collapses_same_ticket_blocks() {
        let conn = open_memory().unwrap();
        for start in ["09:00:00", "09:30:00"] {
            conn.execute(
                "INSERT INTO blocks (day, jira_issue, started_at, ended_at, duration_seconds)
                 VALUES ('2026-04-18', 'PROJ-1',
                         '2026-04-18T' || ?1 || '+00:00',
                         '2026-04-18T10:00:00+00:00', 1800)",
                params![start],
            )
            .unwrap();
        }
        let state = Arc::new(AppState {
            conn: Mutex::new(conn),
        });
        let resp = router(state)
            .oneshot(
                Request::post("/blocks/auto-merge")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"day":"2026-04-18"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["removed"], 1, "two same-ticket blocks → one removed");
    }

    fn seed_git_repo(path: &std::path::Path) {
        run_git(path, &["init", "-q", "-b", "main"]);
        run_git(path, &["config", "user.email", "test@example.com"]);
        run_git(path, &["config", "user.name", "Tester"]);
        run_git(path, &["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join("a.txt"), "alpha\n").unwrap();
        run_git(path, &["add", "a.txt"]);
        run_git_with_date(
            path,
            &["commit", "-q", "-m", "first"],
            "2026-05-10T10:00:00Z",
        );
        std::fs::write(path.join("a.txt"), "alpha\nbeta\n").unwrap();
        run_git(path, &["add", "a.txt"]);
        run_git_with_date(
            path,
            &["commit", "-q", "-m", "second"],
            "2026-05-11T10:00:00Z",
        );
    }

    fn run_git(cwd: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn run_git_with_date(cwd: &std::path::Path, args: &[&str], date: &str) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
