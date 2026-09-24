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
//! * `POST /blocks/:id/estimate`         — no body, re-runs Claude on one block
//! * `GET  /blocks/:id/commits`          — commits in the window (work only)
//! * `POST /infer`                       — { "day": "YYYY-MM-DD" }
//! * `POST /days/:day/allocations`       — { started_at, ended_at, shares: {project: fraction} } — re-runs infer
//! * `POST /days/:day/allocations/delete` — { started_at, ended_at } — re-runs infer
//! * `POST /jira/refresh`                — no body, refreshes open tickets
//! * `GET  /tickets/search?q=&limit=`    — live Jira search (no persistence)
//! * `POST /tickets/external`            — cache a manually-picked ticket
//! * `POST /tickets/create`              — create a Jira issue (sets account)
//! * `GET  /projects`                    — list Jira projects (create picker)
//! * `GET  /accounts`                    — list Tempo accounts (create picker)
//! * `POST /estimate`                    — { "day": "YYYY-MM-DD", "model": "?" }
//! * `POST /sync`                        — { "day": "YYYY-MM-DD", "dry_run": true }
//! * `GET  /export/:day`                 — billing rows + rendered text/csv/json for a day
//! * `POST /export/:day/mark`            — mark a day's blocks as billed (idempotent)
//! * `POST /browser/heartbeat`           — { Heartbeat } from the add-on, requires moz-extension:// Origin
//! * `GET  /days/:day/routed?include_hidden=` — browser/Slack events for a day (default excludes dismissed/noise)
//! * `POST /events/:id/label`            — { LabelRequest } manual label, optionally creating a rule
//! * `POST /events/:id/dismiss`           — { DismissRequest } mark noise, optionally creating an `__ignore__` rule
//! * `GET  /routing/rules`                — hard rules list
//! * `POST /routing/rules/:id/delete`    — no body
//! * `GET  /routing/status`               — last heartbeat/Slack timestamps + Verdict reachability
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
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::billing;
use crate::billing_registry;
use crate::browser_ingest;
use crate::collectors::{jira, tempo};
use crate::git::{self, CommitEntry};
use crate::personal;
use crate::routing;
use crate::routing_absorb;
use crate::routing_contract;
use crate::routing_contract::RouteRule;
use crate::routing_dismiss;
use crate::secrets;
use crate::verdict::VerdictClassifier;
use crate::{
    block_service, db, estimate, infer, infer_allocations,
    models::{Block, Event},
    overlaps, repo,
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
        .route("/blocks/:id/estimate", post(estimate_block))
        .route("/blocks/merge", post(merge_blocks))
        .route("/blocks/auto-merge", post(auto_merge))
        .route("/infer", post(run_infer))
        .route("/days/:day/allocations", post(save_allocation_handler))
        .route(
            "/days/:day/allocations/delete",
            post(delete_allocation_handler),
        )
        .route("/jira/refresh", post(refresh_jira))
        .route("/estimate", post(run_estimate))
        .route("/sync", post(run_sync))
        .route("/export/:day", get(export_day))
        .route("/export/:day/mark", post(mark_export))
        .route("/billing/registry", get(billing_registry_get))
        .route("/billing/customers", post(billing_customer_upsert))
        .route(
            "/billing/customers/:id/delete",
            post(billing_customer_delete),
        )
        .route("/billing/folders", post(billing_folder_upsert))
        .route("/billing/folders/:id/delete", post(billing_folder_delete))
        .route("/settings", get(get_settings).post(post_settings))
        .route(
            "/browser/heartbeat",
            post(browser_heartbeat).options(browser_heartbeat_preflight),
        )
        .route("/days/:day/routed", get(routed_events))
        .route("/events/:id/label", post(set_event_label))
        .route("/events/:id/dismiss", post(dismiss_event_handler))
        .route("/routing/rules", get(routing_rules_list))
        .route("/routing/rules/:id/delete", post(routing_rule_delete))
        .route("/routing/status", get(routing_status))
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

/// How often the daemon's billing-cycle-prune due-check runs — once on
/// start, then on this cadence forever after (spec 002 FR-023 / B39).
/// Named + exported so a test can assert the value without waiting for
/// it to elapse.
pub const PRUNE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Spawn the daemon's periodic billing-cycle prune due-check: runs once
/// immediately, then every [`PRUNE_CHECK_INTERVAL`] thereafter, for as
/// long as the returned handle lives. Skipped entirely — no db access,
/// no path resolution — when [`crate::purge::pruning_enabled`] is
/// `false`. The sqlite work runs on the blocking pool (mirroring
/// [`with_conn`]) so rusqlite never runs directly on the async runtime;
/// `state.conn`'s mutex is acquired only for the duration of that
/// blocking call and released promptly afterwards. A prune failure is
/// logged at `warn` and swallowed — it must never bring the daemon down
/// or fail a request (spec 002 §5.3).
/// `snapshot_to` and `db_path` are resolved ONCE by the caller and owned
/// by the loop. They are deliberately not re-derived per tick from
/// [`crate::paths::Paths::resolve`]: that reads the process-global
/// `$WORKLOG_HOME`, and a test which cleared it between another test's
/// `set_var` and this call once pointed a tick at the real
/// `~/.local/share/worklog` and pruned it for real. A daemon's paths
/// never change at runtime, so resolving them once is both safer and
/// more honest about the dependency.
pub fn spawn_prune_loop(
    state: Shared,
    snapshot_to: PathBuf,
    db_path: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            prune_due_check_once(state.clone(), &snapshot_to, &db_path).await;
            tokio::time::sleep(PRUNE_CHECK_INTERVAL).await;
        }
    })
}

/// Build shared state around an already-open connection. Lets a caller —
/// in practice a test — bind to an explicit database instead of
/// resolving one from the process environment.
pub fn state_from_conn(conn: Connection) -> Shared {
    Arc::new(AppState {
        conn: Mutex::new(conn),
    })
}

/// One tick of [`spawn_prune_loop`]: resolve the current cycle cutoff
/// and hand it to [`crate::purge::prune_if_due`], swallowing any
/// failure. A real prune (the due path) logs exactly one `info!` line
/// naming the cutoff and every deletion count, plus an additional
/// `warn!` when any deleted block was never billed — that loss must not
/// be silent (spec 002 §5.6, FR-018, FR-022 / B32). A not-due check logs
/// nothing at `info` level — it fires on a 6h timer forever and would be
/// pure noise (spec 002 §5.6 / B33) — only a `trace!` for anyone
/// watching that closely. A failure logs `warn!` naming the failing
/// stage, carried by the error's `anyhow::Context` chain.
async fn prune_due_check_once(state: Shared, snapshot_to: &Path, db_path: &Path) {
    if !crate::purge::pruning_enabled() {
        return;
    }
    let snapshot_to = snapshot_to.to_path_buf();
    let db_path = db_path.to_path_buf();
    let outcome =
        tokio::task::spawn_blocking(move || -> Result<Option<crate::purge::PurgeReport>> {
            let today = crate::tz::local_date(chrono::Utc::now());
            let cutoff = crate::purge::cutoff_for_cycle(
                today,
                crate::purge::configured_cycle_start_day(),
                crate::purge::configured_close_day(),
            );
            let opts = crate::purge::PruneOptions {
                cutoff,
                dry_run: false,
                snapshot_to: Some(snapshot_to.as_path()),
                db_path: Some(db_path.as_path()),
            };
            let conn = state.conn.blocking_lock();
            crate::purge::prune_if_due(&conn, &opts)
        })
        .await;

    match outcome {
        Ok(Ok(Some(report))) => {
            info!(
                cutoff = %report.cutoff_date,
                blocks_deleted = report.blocks_deleted,
                blocks_deleted_unbilled = report.blocks_deleted_unbilled,
                events_deleted = report.events_deleted,
                sessions_deleted = report.sessions_deleted,
                tickets_deleted = report.tickets_deleted,
                "billing-cycle prune completed"
            );
            if report.blocks_deleted_unbilled > 0 {
                warn!(
                    "billing-cycle prune deleted {} never-billed block(s) \
                     (no Tempo id and no exported_at marker) — that work is gone",
                    report.blocks_deleted_unbilled
                );
            }
        }
        Ok(Ok(None)) => {
            // Not due — a total no-op. No `info!` here: this tick fires
            // every 6h forever, so logging it at `info` would be pure
            // noise (spec 002 §5.6).
            tracing::trace!("billing-cycle prune due-check: not due, nothing to do");
        }
        Ok(Err(e)) => warn!("billing-cycle prune due-check failed: {e:#}"),
        Err(e) => warn!("billing-cycle prune due-check task panicked: {e}"),
    }
}

// ───────────────────────── handlers ─────────────────────────

/// Sentinel type so handlers stay concise. Variants map to HTTP status
/// codes: `BadRequest` → 400 (client sent bad input), `Internal` → 500
/// (anything else). Any `anyhow::Error` that bubbles up via `?` becomes
/// `Internal` by default; handlers opt into 400 by constructing
/// `ApiError::bad_request(...)` explicitly.
pub enum ApiError {
    BadRequest(anyhow::Error),
    NotFound(anyhow::Error),
    Forbidden(anyhow::Error),
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
            ApiError::NotFound(e) => (StatusCode::NOT_FOUND, e),
            ApiError::Forbidden(e) => (StatusCode::FORBIDDEN, e),
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
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => {
                (format!("{err}"), None)
            }
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
    /// Dominant working directory across the block's events. Chosen by a
    /// PROJECT-ROOT vote using the *exact same* folder key as
    /// [`crate::billing::work_folder_for_block`] — project_path folded via
    /// [`crate::billing::work_folder_for_path`], falling back to the
    /// GitHub repo basename when project_path can't place it, skipping
    /// events that resolve neither. The winning folder is therefore always
    /// identical to the billing group the block sits in. This field is
    /// then a full path *inside* that winning folder — the exact path
    /// with the most events among rows that had one — or `None` when the
    /// winning folder was won entirely by repo-derived events with no
    /// `project_path` at all (e.g. a pure-GitHub block). The UI can never
    /// show a path that contradicts the billing group: it shows a path
    /// from the right folder, or nothing.
    pub project_path: Option<String>,
    /// The folded billing folder key that won the same vote as
    /// `project_path` (see above) — e.g. `lyfjastofnun` for a block whose
    /// events live under `.../lyfjastofnun/.claude/worktrees/ci-on-codebuild`.
    /// Always [`crate::billing::work_folder_for_block`]'s answer for this
    /// block, so the UI can show the billing group even when
    /// `project_path` itself is a worktree/branch path a user wouldn't
    /// recognise as the project name.
    pub project: Option<String>,
    /// "high"/"medium"/"low" from [`crate::timeline::block_confidence`],
    /// keyed off how many distinct sources fed the block.
    pub confidence: String,
}

/// A gap of at least 30 minutes between two consecutive blocks on a day,
/// from [`crate::timeline::day_gaps`].
#[derive(Serialize)]
pub struct DayGap {
    pub started_at: String,
    pub ended_at: String,
    pub minutes: i64,
}

#[derive(Serialize)]
pub struct DaySummary {
    pub day: String,
    pub total_seconds: i64,
    pub blocks: Vec<BlockSummary>,
    pub gaps: Vec<DayGap>,
    /// Windows where ≥2 work projects were both active — see
    /// `overlaps::day_overlaps`. The day strip renders these as a band the
    /// owner can click to rebalance the automatic split.
    pub overlaps: Vec<overlaps::Overlap>,
    /// Every work project's full gap-bridged activity for the day — see
    /// `overlaps::day_activity`. The lanes view draws this as a faint fill
    /// behind each lane's owned block segments, since `infer_lanes` picks
    /// one owner per minute and would otherwise hide the rest.
    pub activity: Vec<overlaps::ProjectActivity>,
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
    let day_parsed = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok();
    let overlaps = day_parsed
        .map(|d| overlaps::day_overlaps(conn, d))
        .transpose()?
        .unwrap_or_default();
    let activity = day_parsed
        .map(|d| overlaps::day_activity(conn, d))
        .transpose()?
        .unwrap_or_default();
    if blocks.is_empty() {
        return Ok(DaySummary {
            day: day.to_owned(),
            total_seconds: 0,
            blocks: vec![],
            gaps: vec![],
            overlaps,
            activity,
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

    // Dominant working directory per block in one query. Batched so the
    // day load stays a fixed number of round-trips regardless of block
    // count, mirroring `personal::dominant_project_path_for_block`'s
    // per-block shape.
    //
    // The winner is picked by a two-stage vote using the *exact same*
    // folder key as `billing::work_folder_for_block`, so the winning
    // folder always agrees with the billing group the block sits in.
    // Voting on the raw exact path lets a genai-infra path that's
    // concentrated in one worktree beat a lyfjastofnun path that's split
    // across several worktrees, even though lyfjastofnun has more events
    // overall and is what billing bills the block under. And voting only
    // on `project_path`, ignoring `repo`, silently disagreed with billing
    // whenever GitHub-only events (which always carry `project_path =
    // NULL`, `repo = Some("org/Name")`) outweighed the block's
    // Claude-derived events — billing would fall back to the repo
    // basename and bill under it while this label used a different
    // folder entirely.
    //
    // Stage 1: for every (project_path, repo) row, compute the folder key
    // the same way `billing::work_folder_for_block` does — fold
    // `project_path` to its project root via `billing::work_folder_for_path`,
    // falling back to the GitHub repo basename when that fails, and
    // skipping the row entirely when neither resolves. Sum counts per
    // folder key. The folder with the most events wins; ties break
    // lexicographically by folder name, exactly like
    // `billing::work_folder_for_block`, so the two never disagree.
    //
    // Stage 2: within the winning folder only, consider rows that have an
    // actual `project_path` (repo-only rows have none to offer) and
    // return the exact path with the most events as the representative
    // `project_path` (the web UI needs a full path, not a bare folder
    // name). Ties break by the lexicographically smallest path for
    // determinism. When the winning folder has no row with a
    // `project_path` at all — a pure-GitHub block, won on repo basename
    // alone — there is no full path to show, so `project_path` stays
    // `None` rather than a bare folder name or a misleading path from a
    // losing folder.
    let path_sql = format!(
        "SELECT be.block_id, e.project_path, e.repo, COUNT(*)
           FROM block_events be
           JOIN events e ON e.id = be.event_id
          WHERE be.block_id IN ({placeholders})
            AND (e.project_path IS NOT NULL OR e.repo IS NOT NULL)
          GROUP BY be.block_id, e.project_path, e.repo"
    );
    let mut path_stmt = conn.prepare(&path_sql)?;
    // [(exact_path, count), ..] — only rows that actually carried a
    // project_path; repo-only rows contribute to the folder total but not
    // to this list.
    type PathCounts = Vec<(String, i64)>;
    // folder_key -> (folder_total, exact-path counts)
    type FolderVotes = std::collections::HashMap<String, (i64, PathCounts)>;
    // block_id -> FolderVotes
    let mut folder_votes: std::collections::HashMap<i64, FolderVotes> =
        std::collections::HashMap::new();
    let path_rows = path_stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    for row in path_rows {
        let (bid, project_path, repo_name, n) = row?;
        // Mirrors `billing::work_folder_for_block`'s vote key exactly.
        let folder_key = project_path
            .as_deref()
            .and_then(crate::billing::work_folder_for_path)
            .or_else(|| {
                repo_name
                    .as_deref()
                    .and_then(|r| r.rsplit('/').next())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            });
        let Some(folder_key) = folder_key else {
            continue;
        };
        let entry = folder_votes
            .entry(bid)
            .or_default()
            .entry(folder_key)
            .or_insert((0, Vec::new()));
        entry.0 += n;
        if let Some(path) = project_path {
            entry.1.push((path, n));
        }
    }

    let mut best_path: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut best_folder: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for (bid, folders) in folder_votes {
        let mut folder_list: Vec<(String, i64, PathCounts)> = folders
            .into_iter()
            .map(|(folder, (total, paths))| (folder, total, paths))
            .collect();
        folder_list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let Some((folder_key, _, mut paths)) = folder_list.into_iter().next() else {
            continue;
        };
        best_folder.insert(bid, folder_key);
        paths.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if let Some((path, _)) = paths.into_iter().next() {
            best_path.insert(bid, path);
        }
    }

    let intervals: Vec<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = blocks
        .iter()
        .filter_map(|b| {
            let start = chrono::DateTime::parse_from_rfc3339(&b.started_at).ok()?;
            let end = chrono::DateTime::parse_from_rfc3339(&b.ended_at).ok()?;
            Some((start.to_utc(), end.to_utc()))
        })
        .collect();
    let gaps = crate::timeline::day_gaps(&intervals, chrono::Duration::minutes(30))
        .into_iter()
        .map(|(start, end)| DayGap {
            started_at: start.to_rfc3339(),
            ended_at: end.to_rfc3339(),
            minutes: (end - start).num_minutes(),
        })
        .collect();

    let enriched = blocks
        .into_iter()
        .map(|block| {
            let id = block.id;
            let sources = sources_by_block.remove(&id).unwrap_or_default();
            BlockSummary {
                event_count: counts.get(&id).copied().unwrap_or(0),
                confidence: crate::timeline::block_confidence(sources.len()).to_owned(),
                sources,
                project_path: best_path.remove(&id),
                project: best_folder.remove(&id),
                block,
            }
        })
        .collect();

    Ok(DaySummary {
        day: day.to_owned(),
        total_seconds,
        blocks: enriched,
        gaps,
        overlaps,
        activity,
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

/// Re-run Claude on a single block. Overwrites the block's description /
/// duration / jira_issue / estimated_by — the user has explicitly asked
/// for a re-describe, so the "skip manual" rule that `POST /estimate`
/// enforces does NOT apply here. Includes local git commits in the LLM
/// prompt alongside the linked events. Refuses personal blocks (400) and
/// missing blocks (404).
async fn estimate_block(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    // Pre-flight on the connection: 404 if the row's gone, 400 if it's
    // personal. Also fetch the dominant project path so the async git
    // call has a cwd to work with. Done on a separate `with_conn` so
    // the connection lock is released before we shell out to git.
    let pre = with_conn(state.clone(), move |c| {
        // Mirror the same query estimate_block_with uses so the two
        // paths agree about block-not-found semantics.
        let row: Option<(i64, bool, String, String)> = c
            .query_row(
                "SELECT id, is_personal, started_at, ended_at
                   FROM blocks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((bid, is_personal, started_at, ended_at)) = row else {
            return Ok::<_, anyhow::Error>(None);
        };
        // Personal blocks: no project_path lookup, no LLM call. Return
        // the personal flag so the caller can 400.
        if is_personal {
            return Ok(Some((bid, true, None, started_at, ended_at)));
        }
        let project_path = personal::dominant_project_path_for_block(c, bid)?;
        Ok(Some((bid, false, project_path, started_at, ended_at)))
    })
    .await?;

    let Some((bid, is_personal, project_path, started_at, ended_at)) = pre else {
        return Err(ApiError::NotFound(anyhow::anyhow!("block {id} not found")));
    };
    if is_personal {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "block {bid} is personal — toggle it back to work before \
             re-describing with Claude"
        )));
    }

    // Pull local git history inside the block's window. Soft-fail: an
    // empty vec lets the estimator proceed with events-only context.
    let commits = if let Some(path) = project_path.as_deref() {
        git::git_log_in_window(std::path::Path::new(path), &started_at, &ended_at)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Three phases, and the split is load-bearing: `with_conn` holds
    // `state.conn` — the daemon's ONE sqlite connection — for as long as
    // its closure runs. Running the estimate inside a single `with_conn`
    // held that mutex across a `claude -p` shell-out worth up to 60s, so
    // every other request (including the web UI's `/days/:day`, which
    // gives up after 10s) queued behind it and the daemon looked dead.
    // Read under the lock, release it for the LLM call, take it again to
    // write.
    let prep = with_conn(state.clone(), move |c| {
        estimate::prepare_block_estimate(c, bid, &commits)
    })
    .await?;

    // No lock held here. `prep` is moved into the blocking closure and
    // handed back out so the write phase can still use it — borrowing it
    // across the thread boundary wouldn't compile. The LLM provider's
    // reqwest client is !Send, so it is built and dropped entirely inside
    // this closure and never crosses an await.
    let (prep, reply) = tokio::task::spawn_blocking(move || {
        let reply = match estimate::resolve_provider()? {
            estimate::ProviderChoice::ClaudeSubprocess => estimate::invoke_block_estimate(
                &prep,
                &estimate::ClaudeSubprocess,
                estimate::DEFAULT_MODEL,
            ),
            estimate::ProviderChoice::LiteLLM(inv) => {
                estimate::invoke_block_estimate(&prep, &inv, estimate::DEFAULT_MODEL)
            }
        }?;
        Ok::<_, anyhow::Error>((prep, reply))
    })
    .await
    .context("spawn_blocking")??;

    let outcome = with_conn(state, move |c| {
        estimate::commit_block_estimate(c, &prep, reply)
    })
    .await?;

    info!(
        block_id = outcome.block_id,
        minutes = outcome.minutes,
        "re-estimated single block"
    );
    Ok(Json(json!({
        "block_id":   outcome.block_id,
        "description": outcome.description,
        "minutes":     outcome.minutes,
        "jira_issue":  outcome.jira_issue,
    })))
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

/// Route, absorb, build blocks (honouring any saved overlap allocations)
/// and persist — the full `POST /infer` pipeline. Shared with the
/// allocation save/delete handlers, which must re-run it so blocks
/// reflect the owner's choice (or its removal) immediately.
async fn reinfer_day(state: Shared, day: NaiveDate) -> Result<(usize, i64), ApiError> {
    // Route before building blocks (design decision 4, PR #41): three
    // phases, mirroring `run_estimate`, so the sqlite mutex is never held
    // across the (slow, network) classifier call.
    let (rule_hits, pending) =
        with_conn(state.clone(), move |c| routing::load_pending(c, day)).await?;
    let guesses = tokio::task::spawn_blocking(move || {
        routing::decide(&pending, &VerdictClassifier::new(), configured_route_rule())
    })
    .await
    .context("spawn_blocking")?;

    let (count, minutes) = with_conn(state, move |c| {
        routing::commit_labels(c, &rule_hits, &guesses)?;
        routing_absorb::absorb_and_noise(c, day)?;
        let events = infer::load_day_events(c, day)?;
        let allocations: Vec<infer_allocations::AllocationWindow> =
            overlaps::load_allocations(c, day)?
                .into_iter()
                .map(
                    |(started_at, ended_at, shares)| infer_allocations::AllocationWindow {
                        started_at,
                        ended_at,
                        shares,
                    },
                )
                .collect();
        let blocks = infer::build_blocks_with_allocations(events, &allocations);
        let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
        infer::persist_blocks(c, day, &blocks)?;
        Ok::<_, anyhow::Error>((blocks.len(), total / 60))
    })
    .await?;
    Ok((count, minutes))
}

async fn run_infer(
    State(state): State<Shared>,
    Json(body): Json<InferBody>,
) -> Result<Json<InferResponse>, ApiError> {
    let day = NaiveDate::parse_from_str(&body.day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", body.day)))?;
    let (count, minutes) = reinfer_day(state, day).await?;
    Ok(Json(InferResponse {
        day: body.day,
        blocks: count,
        minutes,
    }))
}

#[derive(Deserialize)]
pub struct AllocationBody {
    pub started_at: String,
    pub ended_at: String,
    pub shares: std::collections::BTreeMap<String, f64>,
}

/// Save the owner's manual split of an overlap window, then re-run infer
/// (the `POST /infer` pipeline) so blocks reflect it immediately. 400s:
/// bad day/timestamps, shares that are empty, non-positive, don't sum to
/// 1 (±0.001), or name a project that isn't part of this overlap.
async fn save_allocation_handler(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
    Json(body): Json<AllocationBody>,
) -> Result<Json<InferResponse>, ApiError> {
    let day_parsed = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{day}`: {e}")))?;
    let started_at = parse_allocation_ts(&body.started_at)?;
    let ended_at = parse_allocation_ts(&body.ended_at)?;
    validate_shares(&body.shares)?;

    let shares = body.shares.clone();
    with_conn(state.clone(), move |c| {
        let day_overlaps = overlaps::day_overlaps(c, day_parsed)?;
        let matching = day_overlaps
            .iter()
            .find(|o| o.started_at == started_at && o.ended_at == ended_at)
            .ok_or_else(|| anyhow::anyhow!("no overlap at {started_at}–{ended_at}"))?;
        let valid: std::collections::BTreeSet<&str> = matching
            .projects
            .iter()
            .map(|p| p.project.as_str())
            .collect();
        for project in shares.keys() {
            if !valid.contains(project.as_str()) {
                anyhow::bail!("project `{project}` is not part of this overlap");
            }
        }
        overlaps::save_allocation(c, day_parsed, started_at, ended_at, &shares)
    })
    .await
    .map_err(ApiError::bad_request)?;

    let (count, minutes) = reinfer_day(state, day_parsed).await?;
    Ok(Json(InferResponse {
        day,
        blocks: count,
        minutes,
    }))
}

#[derive(Deserialize)]
pub struct DeleteAllocationBody {
    pub started_at: String,
    pub ended_at: String,
}

/// Drop a saved allocation and re-run infer so the automatic split comes
/// back for that window.
async fn delete_allocation_handler(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
    Json(body): Json<DeleteAllocationBody>,
) -> Result<Json<InferResponse>, ApiError> {
    let day_parsed = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{day}`: {e}")))?;
    let started_at = parse_allocation_ts(&body.started_at)?;
    let ended_at = parse_allocation_ts(&body.ended_at)?;
    with_conn(state.clone(), move |c| {
        overlaps::delete_allocation(c, day_parsed, started_at, ended_at)
    })
    .await?;
    let (count, minutes) = reinfer_day(state, day_parsed).await?;
    Ok(Json(InferResponse {
        day,
        blocks: count,
        minutes,
    }))
}

fn parse_allocation_ts(s: &str) -> Result<DateTime<Utc>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid timestamp `{s}`: {e}")))
}

fn validate_shares(shares: &std::collections::BTreeMap<String, f64>) -> Result<(), ApiError> {
    if shares.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "shares must not be empty"
        )));
    }
    if shares.values().any(|f| *f <= 0.0) {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "every share must be > 0"
        )));
    }
    let sum: f64 = shares.values().sum();
    if (sum - 1.0).abs() > 0.001 {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "shares must sum to 1.0 (±0.001), got {sum}"
        )));
    }
    Ok(())
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
    /// Whether the billing-cycle pruner's automatic due-check runs at
    /// all (spec 002 FR-011). Mirrors `crate::purge::pruning_enabled`.
    pub prune_enabled: bool,
    /// Day-of-month the billing cycle starts. Mirrors
    /// `crate::purge::configured_cycle_start_day`; default 20.
    pub cycle_start_day: u32,
    /// Last day-of-month the just-closed cycle can still take hours.
    /// Mirrors `crate::purge::configured_close_day`; default 23.
    pub close_day: u32,
    /// Editable work-hours window for browser heartbeat ingest, e.g.
    /// `Mon-Fri 09:00-17:00`. Mirrors `WORK_HOURS_KEY` via envfile;
    /// defaults to `DEFAULT_WORK_HOURS`.
    pub work_hours: String,
    /// How many times higher than the abstain score the winner must be.
    /// Mirrors `ABSTAIN_MARGIN_KEY` via envfile; defaults to
    /// `DEFAULT_ABSTAIN_MARGIN`.
    pub abstain_margin: f64,
    /// How many times higher than the runner-up the winner must be.
    /// Mirrors `RUNNER_UP_RATIO_KEY` via envfile; defaults to
    /// `DEFAULT_RUNNER_UP_RATIO`.
    pub runner_up_ratio: f64,
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
            | "slack_user_token"
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
    let rule = configured_route_rule();
    Ok(SettingsView {
        personal: PersonalPatterns {
            work: file.work,
            personal: file.personal,
        },
        secrets,
        timezone: crate::tz::configured_tz().unwrap_or_default(),
        personal_config_path: cfg_path.map(|p| p.display().to_string()),
        prune_enabled: crate::purge::pruning_enabled(),
        cycle_start_day: crate::purge::configured_cycle_start_day(),
        close_day: crate::purge::configured_close_day(),
        work_hours: configured_work_hours_raw(),
        abstain_margin: rule.abstain_margin,
        runner_up_ratio: rule.runner_up_ratio,
    })
}

/// Raw `WORK_HOURS_KEY` envfile value, or `DEFAULT_WORK_HOURS` when unset.
fn configured_work_hours_raw() -> String {
    crate::envfile::read(routing_contract::WORK_HOURS_KEY)
        .unwrap_or_else(|| routing_contract::DEFAULT_WORK_HOURS.to_owned())
}

/// Parsed work-hours window for heartbeat ingest. An unparseable stored
/// value (should never happen — `post_settings` validates before writing)
/// falls back to the default rather than 500ing every heartbeat.
fn configured_work_hours() -> browser_ingest::WorkHours {
    let raw = configured_work_hours_raw();
    browser_ingest::WorkHours::parse(&raw).unwrap_or_else(|e| {
        warn!(
            "{}={raw:?} invalid ({e}); falling back to default",
            routing_contract::WORK_HOURS_KEY
        );
        browser_ingest::WorkHours::parse(routing_contract::DEFAULT_WORK_HOURS)
            .expect("DEFAULT_WORK_HOURS must parse")
    })
}

/// The abstain-margin/runner-up-ratio rule a routing guess must clear.
/// Reads both envfile keys independently; an unparseable or
/// out-of-`RATIO_RANGE` stored value falls back to that key's default
/// and emits a `warn!` naming the key and the fallback, mirroring
/// `purge::configured_cycle_day`'s handling of a bad pruner setting.
pub fn configured_route_rule() -> RouteRule {
    RouteRule {
        abstain_margin: configured_ratio(
            routing_contract::ABSTAIN_MARGIN_KEY,
            routing_contract::DEFAULT_ABSTAIN_MARGIN,
        ),
        runner_up_ratio: configured_ratio(
            routing_contract::RUNNER_UP_RATIO_KEY,
            routing_contract::DEFAULT_RUNNER_UP_RATIO,
        ),
    }
}

fn configured_ratio(key: &str, default: f64) -> f64 {
    let (lo, hi) = routing_contract::RATIO_RANGE;
    match crate::envfile::read(key) {
        None => default,
        Some(raw) => {
            match raw.trim().parse::<f64>() {
                Ok(v) if (lo..=hi).contains(&v) => v,
                _ => {
                    warn!("{key}={raw:?} is not a valid ratio in {lo}..={hi}. Falling back to {default}.");
                    default
                }
            }
        }
    }
}

async fn get_settings() -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(settings_off_runtime().await?))
}

/// `current_settings` reads the OS keychain, which can block on a permission
/// dialog; run it off the async workers so a pending prompt can't stall the daemon.
async fn settings_off_runtime() -> Result<SettingsView> {
    tokio::task::spawn_blocking(current_settings)
        .await
        .context("spawn_blocking")?
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
    /// Enable/disable the billing-cycle pruner's automatic due-check.
    /// `None` leaves the current setting untouched.
    pub prune_enabled: Option<bool>,
    /// Day-of-month the billing cycle starts. Accepted as a raw JSON
    /// value rather than `Option<u32>` so a non-integer submission
    /// (e.g. a string) reaches this endpoint's own validation as a 400
    /// naming the valid range, instead of axum's generic deserialize-
    /// error rejection.
    pub cycle_start_day: Option<Value>,
    /// Last day-of-month the just-closed cycle can still take hours.
    /// Same non-`u32` typing rationale as `cycle_start_day`.
    pub close_day: Option<Value>,
    /// Replace the browser heartbeat work-hours window, e.g.
    /// `Mon-Fri 09:00-17:00`. `None` leaves it untouched.
    pub work_hours: Option<String>,
    /// Replace how many times higher than the abstain score the winner
    /// must be (`RATIO_RANGE`). `None` leaves it untouched.
    pub abstain_margin: Option<f64>,
    /// Replace how many times higher than the runner-up the winner must
    /// be (`RATIO_RANGE`). `None` leaves it untouched.
    pub runner_up_ratio: Option<f64>,
}

#[derive(Serialize)]
pub struct SettingsSaveResponse {
    #[serde(flatten)]
    pub settings: SettingsView,
    /// Present only when classification patterns changed and existing
    /// blocks were reclassified against the new rules.
    pub reclassified: Option<personal::ReclassifyStats>,
}

/// Parse a submitted pruner-cycle-day field into a validated `u32`
/// `1..=31` (spec 002 FR-017 / AC-018). `v` is a raw JSON value rather
/// than an already-typed integer specifically so a non-integer
/// submission (e.g. a string, a float, a negative number) is rejected
/// here — as a 400 naming the valid range — rather than by axum's
/// generic `Json<T>` deserialize-error rejection before this handler
/// ever runs.
fn parse_cycle_day(field: &str, v: &Value) -> Result<u32, ApiError> {
    let day = v.as_i64().and_then(|n| u32::try_from(n).ok());
    match day {
        Some(d) if crate::purge::is_valid_cycle_day(d) => Ok(d),
        _ => Err(ApiError::bad_request(anyhow::anyhow!(
            "`{field}` must be an integer 1..=31 (got {v})"
        ))),
    }
}

async fn post_settings(
    State(state): State<Shared>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<SettingsSaveResponse>, ApiError> {
    // ── Phase 1: validate every submitted field BEFORE persisting any
    // of them (spec 002 AC-018 / FR-017: "one bad field among several
    // good ones must persist nothing"). ──

    // Timezone: validate as a fixed offset so a typo 400s instead of
    // silently bucketing days in UTC later.
    let tz = body.timezone.as_deref().map(str::trim);
    if let Some(tz) = tz {
        if !crate::tz::is_valid_tz(tz) {
            return Err(ApiError::bad_request(anyhow::anyhow!(
                "`{tz}` is not a fixed offset — use +HH:MM, -HH:MM, or UTC \
                 (named zones like America/New_York are not supported)"
            )));
        }
    }

    // Pruner cycle days: each field's own range first, then the
    // cross-field relationship — a distinct message from the range
    // one, since a close day earlier than the cycle's start day would
    // collapse the grace period and delete the just-closed cycle
    // immediately (spec 002 Appendix A).
    let cycle_start_day = body
        .cycle_start_day
        .as_ref()
        .map(|v| parse_cycle_day("cycle_start_day", v))
        .transpose()?;
    let close_day = body
        .close_day
        .as_ref()
        .map(|v| parse_cycle_day("close_day", v))
        .transpose()?;
    let effective_start = cycle_start_day.unwrap_or_else(crate::purge::configured_cycle_start_day);
    let effective_close = close_day.unwrap_or_else(crate::purge::configured_close_day);
    if effective_close < effective_start {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "`close_day` ({effective_close}) must not be earlier than \
             `cycle_start_day` ({effective_start}) — the grace period would \
             collapse and the pruner would delete the just-closed cycle immediately"
        )));
    }

    // Work hours: validate the same parser the heartbeat handler uses,
    // so a typo 400s here instead of silently falling back later.
    let work_hours = body.work_hours.as_deref().map(str::trim);
    if let Some(wh) = work_hours {
        browser_ingest::WorkHours::parse(wh).map_err(|e| {
            ApiError::bad_request(anyhow::anyhow!(
                "`{wh}` is not a valid work-hours window: {e}"
            ))
        })?;
    }

    // Route ratios: each must lie in RATIO_RANGE.
    let (ratio_lo, ratio_hi) = routing_contract::RATIO_RANGE;
    for (field, v) in [
        ("abstain_margin", body.abstain_margin),
        ("runner_up_ratio", body.runner_up_ratio),
    ] {
        if let Some(v) = v {
            if !(ratio_lo..=ratio_hi).contains(&v) {
                return Err(ApiError::bad_request(anyhow::anyhow!(
                    "`{field}` must be between {ratio_lo} and {ratio_hi} (got {v})"
                )));
            }
        }
    }

    // ── Phase 2: persist. Nothing above returned early, so every
    // submitted field is valid. ──

    if let Some(tz) = tz {
        crate::envfile::upsert("WORKLOG_TZ", tz)?;
    }

    // Secrets → OS keychain. Empty value deletes. Unknown keys are
    // refused so a stale client can't scribble arbitrary entries.
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

    // Personal patterns → personal.toml, then reclassify existing
    // blocks so the change shows up immediately, not just on next infer.
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

    if let Some(enabled) = body.prune_enabled {
        crate::envfile::upsert(
            "WORKLOG_PRUNE_ENABLED",
            if enabled { "true" } else { "false" },
        )?;
    }
    if let Some(day) = cycle_start_day {
        crate::envfile::upsert("WORKLOG_BILLING_CYCLE_START_DAY", &day.to_string())?;
    }
    if let Some(day) = close_day {
        crate::envfile::upsert("WORKLOG_BILLING_CLOSE_DAY", &day.to_string())?;
    }
    if let Some(wh) = work_hours {
        crate::envfile::upsert(routing_contract::WORK_HOURS_KEY, wh)?;
    }
    if let Some(v) = body.abstain_margin {
        crate::envfile::upsert(routing_contract::ABSTAIN_MARGIN_KEY, &v.to_string())?;
    }
    if let Some(v) = body.runner_up_ratio {
        crate::envfile::upsert(routing_contract::RUNNER_UP_RATIO_KEY, &v.to_string())?;
    }

    info!("settings updated");
    Ok(Json(SettingsSaveResponse {
        settings: settings_off_runtime().await?,
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

/// Latest non-empty `blocks.exported_at` across a day's blocks, or
/// `None` when the day has never been (fully or partially) marked
/// exported.
fn latest_exported_at(conn: &Connection, day: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT MAX(exported_at) FROM blocks
          WHERE day = ?1 AND exported_at IS NOT NULL AND exported_at != ''",
        rusqlite::params![day],
        |row| row.get(0),
    )
    .context("latest_exported_at")
}

async fn export_day(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", day)))?;

    let day_for_conn = day.clone();
    let (rows, exported_at) = with_conn(state, move |c| {
        let rows = billing::rows_for_day(c, &day_for_conn)?;
        let exported_at = latest_exported_at(c, &day_for_conn)?;
        Ok((rows, exported_at))
    })
    .await?;

    let text = billing::render(&rows, billing::Format::Text);
    let csv = billing::render(&rows, billing::Format::Csv);
    let json_str = billing::render(&rows, billing::Format::Json);

    Ok(Json(json!({
        "day": day,
        "exported_at": exported_at,
        "rows": rows,
        "rendered": {
            "text": text,
            "csv": csv,
            "json": json_str,
        },
    })))
}

async fn mark_export(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", day)))?;

    let day_for_conn = day.clone();
    let (marked, exported_at) = with_conn(state, move |c| {
        let marked = block_service::mark_exported(c, &day_for_conn)?;
        let exported_at = latest_exported_at(c, &day_for_conn)?;
        Ok((marked, exported_at))
    })
    .await?;

    info!(day = %day, marked, "marked blocks exported");
    Ok(Json(json!({
        "day": day,
        "marked": marked,
        "exported_at": exported_at,
    })))
}

// ───────────────────────── browser + Slack routing ─────────────────────────

/// Only the add-on itself can send an `Origin: moz-extension://...` header
/// — an ordinary web page can't forge it — so this check is the whole
/// authentication story (design.md decision 5). Shared by the preflight
/// and the POST handler so both enforce exactly the same rule.
fn require_extension_origin(headers: &HeaderMap) -> Result<&str, ApiError> {
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !origin.starts_with("moz-extension://") {
        return Err(ApiError::Forbidden(anyhow::anyhow!(
            "origin {origin:?} is not a moz-extension:// origin"
        )));
    }
    Ok(origin)
}

/// Firefox sends `OPTIONS /browser/heartbeat` before every heartbeat POST;
/// without an answer here the browser never issues the POST at all.
async fn browser_heartbeat_preflight(headers: HeaderMap) -> Result<Response, ApiError> {
    let origin = require_extension_origin(&headers)?.to_string();
    Ok((
        StatusCode::OK,
        [
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin),
            (
                axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
                "POST".to_string(),
            ),
            (
                axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
                "content-type".to_string(),
            ),
        ],
    )
        .into_response())
}

/// The Firefox add-on's heartbeat endpoint.
async fn browser_heartbeat(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(hb): Json<routing_contract::Heartbeat>,
) -> Result<Response, ApiError> {
    let origin = require_extension_origin(&headers)?.to_string();

    let hours = configured_work_hours();
    let offset = crate::tz::day_offset();
    let outcome = with_conn(state, move |c| {
        browser_ingest::ingest_heartbeat(c, &hb, &hours, offset)
    })
    .await?;

    let (stored, reason) = match outcome {
        browser_ingest::IngestOutcome::Stored(_) => (true, None),
        browser_ingest::IngestOutcome::Filtered(reason) => (false, Some(reason)),
    };
    Ok((
        [(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin)],
        Json(json!({ "stored": stored, "reason": reason })),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct RoutedQuery {
    /// Include dismissed/noise events too — the review drawer's request.
    #[serde(default)]
    pub include_hidden: bool,
}

async fn routed_events(
    State(state): State<Shared>,
    AxumPath(day): AxumPath<String>,
    axum::extract::Query(q): axum::extract::Query<RoutedQuery>,
) -> Result<Json<Vec<routing_contract::RoutedEvent>>, ApiError> {
    let parsed = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid day `{}`: {e}", day)))?;
    let events = with_conn(state, move |c| {
        routing::routed_for_day(c, parsed, q.include_hidden)
    })
    .await?;
    Ok(Json(events))
}

async fn set_event_label(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<routing_contract::LabelRequest>,
) -> Result<Json<routing_contract::RoutedEvent>, ApiError> {
    let exists: bool = with_conn(state.clone(), move |c| {
        Ok(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?)
    })
    .await?;
    if !exists {
        return Err(ApiError::NotFound(anyhow::anyhow!("event {id} not found")));
    }
    let routed = with_conn(state, move |c| routing::label_event(c, id, &body))
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(routed))
}

async fn dismiss_event_handler(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<routing_contract::DismissRequest>,
) -> Result<Json<routing_contract::RoutedEvent>, ApiError> {
    let exists: bool = with_conn(state.clone(), move |c| {
        Ok(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?)
    })
    .await?;
    if !exists {
        return Err(ApiError::NotFound(anyhow::anyhow!("event {id} not found")));
    }
    let routed = with_conn(state, move |c| {
        routing_dismiss::dismiss_event(c, id, body.rule_kind)
    })
    .await
    .map_err(ApiError::bad_request)?;
    Ok(Json(routed))
}

async fn routing_rules_list(
    State(state): State<Shared>,
) -> Result<Json<Vec<routing_contract::Rule>>, ApiError> {
    let rules = with_conn(state, routing::list_rules).await?;
    Ok(Json(rules))
}

async fn routing_rule_delete(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    let removed = with_conn(state, move |c| routing::delete_rule(c, id)).await?;
    info!(id, removed, "deleted routing rule");
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Serialize)]
struct RoutingStatus {
    last_heartbeat: Option<String>,
    last_slack: Option<String>,
    classifier_reachable: bool,
}

async fn routing_status(State(state): State<Shared>) -> Result<Json<RoutingStatus>, ApiError> {
    let (last_heartbeat, last_slack) = with_conn(state, |c| {
        let last_heartbeat: Option<String> = c.query_row(
            "SELECT MAX(started_at) FROM events WHERE source = ?1",
            [routing_contract::SOURCE_FIREFOX],
            |r| r.get(0),
        )?;
        let last_slack: Option<String> = c.query_row(
            "SELECT MAX(started_at) FROM events WHERE source = ?1",
            [routing_contract::SOURCE_SLACK],
            |r| r.get(0),
        )?;
        Ok((last_heartbeat, last_slack))
    })
    .await?;

    // Off the connection lock — a slow/hung Verdict process must not stall
    // every other request.
    let classifier_reachable = tokio::task::spawn_blocking(|| {
        crate::daemon_service::is_running(
            routing_contract::CLASSIFIER_ADDR,
            std::time::Duration::from_secs(2),
        )
    })
    .await
    .unwrap_or(false);

    Ok(Json(RoutingStatus {
        last_heartbeat,
        last_slack,
        classifier_reachable,
    }))
}

// ───────────────────────── billing registry ─────────────────────────

/// How many days of events the unmapped-folder discovery looks back over.
/// A month is long enough to surface every folder actually in rotation
/// without dredging up one-off experiments from last year.
const UNMAPPED_LOOKBACK_DAYS: i64 = 30;

/// The whole registry plus the work folders seen recently that still have
/// no mapping — everything Settings → Billing needs in one round trip.
async fn billing_registry_get(State(state): State<Shared>) -> Result<Json<Value>, ApiError> {
    let (customers, folders, unmapped) = with_conn(state, move |c| {
        Ok((
            billing_registry::list_customers(c)?,
            billing_registry::list_folders(c)?,
            billing_registry::unmapped_folders(c, UNMAPPED_LOOKBACK_DAYS)?,
        ))
    })
    .await?;

    let unmapped: Vec<Value> = unmapped
        .into_iter()
        .map(|(folder, events)| json!({ "folder": folder, "events": events }))
        .collect();

    Ok(Json(json!({
        "customers": customers,
        "folders": folders,
        "unmapped": unmapped,
    })))
}

async fn billing_customer_upsert(
    State(state): State<Shared>,
    Json(body): Json<billing_registry::Customer>,
) -> Result<Json<Value>, ApiError> {
    let name = body.name.clone();
    let id = with_conn(state, move |c| billing_registry::upsert_customer(c, &body)).await?;
    info!(customer = %name, id, "upserted billing customer");
    Ok(Json(json!({ "id": id })))
}

async fn billing_customer_delete(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    let removed = with_conn(state, move |c| billing_registry::delete_customer(c, id)).await?;
    info!(id, removed, "deleted billing customer");
    Ok(Json(json!({ "removed": removed })))
}

async fn billing_folder_upsert(
    State(state): State<Shared>,
    Json(body): Json<billing_registry::FolderMap>,
) -> Result<Json<Value>, ApiError> {
    let folder = body.folder.clone();
    let id = with_conn(state, move |c| billing_registry::upsert_folder(c, &body)).await?;
    info!(folder = %folder, id, "upserted billing folder mapping");
    Ok(Json(json!({ "id": id })))
}

async fn billing_folder_delete(
    State(state): State<Shared>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Value>, ApiError> {
    let removed = with_conn(state, move |c| billing_registry::delete_folder(c, id)).await?;
    info!(id, removed, "deleted billing folder mapping");
    Ok(Json(json!({ "removed": removed })))
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

    /// Inserts a block plus one event per `(source, source_id, ts)` tuple,
    /// linked via `block_events`. Shared by the confidence/gaps test below.
    fn seed_block(
        conn: &Connection,
        start: &str,
        end: &str,
        dur: i64,
        events: &[(&str, &str, &str)],
    ) {
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', ?1, ?2, ?3)",
            params![start, end, dur],
        )
        .unwrap();
        let block_id = conn.last_insert_rowid();
        for (source, source_id, ts) in events {
            let eid =
                repo::upsert_event(conn, &Event::minimal(*source, *source_id, *ts, "x")).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![block_id, eid],
            )
            .unwrap();
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn day_summary_reports_block_confidence_and_gaps() {
        // L6: each block's confidence label comes from its distinct source
        // count, and gaps >=30min between blocks are surfaced for the day.
        let conn = open_memory().unwrap();
        seed_block(
            &conn,
            "2026-04-18T09:00:00+00:00",
            "2026-04-18T09:30:00+00:00",
            1800,
            &[
                ("github_commit", "a", "2026-04-18T09:05:00+00:00"),
                ("claude", "b", "2026-04-18T09:10:00+00:00"),
                ("jira", "c", "2026-04-18T09:15:00+00:00"),
            ],
        );
        seed_block(
            &conn,
            "2026-04-18T10:15:00+00:00",
            "2026-04-18T10:30:00+00:00",
            900,
            &[("claude", "d", "2026-04-18T10:20:00+00:00")],
        );

        let app = router(Arc::new(AppState {
            conn: Mutex::new(conn),
        }));
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
        let blocks = v["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        let high = blocks.iter().find(|b| b["event_count"] == 3).unwrap();
        assert_eq!(high["confidence"], "high");
        let low = blocks.iter().find(|b| b["event_count"] == 1).unwrap();
        assert_eq!(low["confidence"], "low");

        let gaps = v["gaps"].as_array().unwrap();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0]["minutes"], 45);
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
    fn day_summary_project_path_agrees_with_billing_folder_vote() {
        // Regression for the 2026-07-28 defect: a block whose lyf events
        // are spread over several worktree sub-paths (3 + 2 = 5) must
        // still win the label over a genai path concentrated in a single
        // sub-path (4), because `billing::work_folder_for_block` bills
        // this block under lyf's project root. Voting on the raw exact
        // path (the old behaviour) would pick the 4-event genai path
        // instead, producing a UI card that disagrees with the billing
        // group it sits in.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let paths = [
            "/home/u/Desktop/Work/lyf/.claude/worktrees/a",
            "/home/u/Desktop/Work/lyf/.claude/worktrees/a",
            "/home/u/Desktop/Work/lyf/.claude/worktrees/a",
            "/home/u/Desktop/Work/lyf/.claude/worktrees/b",
            "/home/u/Desktop/Work/lyf/.claude/worktrees/b",
            "/home/u/Desktop/Work/genai/.claude/worktrees/c",
            "/home/u/Desktop/Work/genai/.claude/worktrees/c",
            "/home/u/Desktop/Work/genai/.claude/worktrees/c",
            "/home/u/Desktop/Work/genai/.claude/worktrees/c",
        ];
        for (i, path) in paths.iter().enumerate() {
            let mut ev = Event::minimal(
                "claude",
                format!("e{i}").as_str(),
                "2026-04-18T09:05:00+00:00",
                "prompt",
            );
            ev.project_path = Some(path.to_string());
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
            Some("/home/u/Desktop/Work/lyf/.claude/worktrees/a"),
            "the lyf project root has 5 events vs genai's 4, so the \
             3-event lyf sub-path must win the label, not the 4-event \
             genai path"
        );
    }

    #[test]
    fn day_summary_project_path_agrees_when_repo_folder_wins() {
        // Regression for the confirmed defect: a block with 6 GitHub events
        // (project_path NULL, repo "org/AcmeBackend") and 5 Claude events
        // (project_path under .../acme/backend) must be billed under
        // "AcmeBackend" by `billing::work_folder_for_block` — and the day
        // summary must AGREE that "AcmeBackend" is the winning folder,
        // which means it can offer no full path for it (no row in the
        // winning folder carries a project_path), so project_path is None,
        // not the acme/backend path from the losing folder.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();

        for i in 0..6 {
            let mut ev = Event::minimal(
                "github_commit",
                format!("gh{i}").as_str(),
                "2026-04-18T09:05:00+00:00",
                "commit",
            );
            ev.repo = Some("org/AcmeBackend".to_string());
            let eid = repo::upsert_event(&conn, &ev).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }
        for i in 0..5 {
            let mut ev = Event::minimal(
                "claude",
                format!("cl{i}").as_str(),
                "2026-04-18T09:06:00+00:00",
                "prompt",
            );
            ev.project_path = Some("/home/u/Desktop/Work/acme/backend".to_string());
            let eid = repo::upsert_event(&conn, &ev).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }

        let billing_folder = crate::billing::work_folder_for_block(&conn, bid).unwrap();
        assert_eq!(
            billing_folder.as_deref(),
            Some("AcmeBackend"),
            "billing bills this block under the repo-derived folder, which \
             has 6 events vs the claude folder's 5"
        );

        let summary = stitch_day_summary(&conn, "2026-04-18").unwrap();
        assert_eq!(summary.blocks.len(), 1);
        assert_eq!(
            summary.blocks[0].project_path, None,
            "the winning folder (AcmeBackend) is repo-derived and has no \
             row with a project_path, so the summary must show None \
             rather than a path from a different, losing folder"
        );
    }

    #[test]
    fn day_summary_project_path_agrees_when_claude_folder_wins() {
        // Mirror of the repo-folder-wins case: when the Claude-derived
        // folder has more events than the GitHub repo folder, that folder
        // wins both the billing vote and the day-summary vote, and the
        // summary's project_path is a real path inside it.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();

        for i in 0..5 {
            let mut ev = Event::minimal(
                "github_commit",
                format!("gh{i}").as_str(),
                "2026-04-18T09:05:00+00:00",
                "commit",
            );
            ev.repo = Some("org/AcmeBackend".to_string());
            let eid = repo::upsert_event(&conn, &ev).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }
        for i in 0..6 {
            let mut ev = Event::minimal(
                "claude",
                format!("cl{i}").as_str(),
                "2026-04-18T09:06:00+00:00",
                "prompt",
            );
            ev.project_path = Some("/home/u/Desktop/Work/acme/backend".to_string());
            let eid = repo::upsert_event(&conn, &ev).unwrap();
            conn.execute(
                "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
                params![bid, eid],
            )
            .unwrap();
        }

        let billing_folder = crate::billing::work_folder_for_block(&conn, bid).unwrap();
        assert_eq!(
            billing_folder.as_deref(),
            Some("backend"),
            "billing bills this block under the claude-derived folder, \
             which has 6 events vs the repo folder's 5"
        );

        let summary = stitch_day_summary(&conn, "2026-04-18").unwrap();
        assert_eq!(summary.blocks.len(), 1);
        assert_eq!(
            summary.blocks[0].project_path.as_deref(),
            Some("/home/u/Desktop/Work/acme/backend"),
            "the winning folder (backend) has a real project_path, so the \
             summary must surface it"
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

    #[test]
    fn day_summary_project_reports_the_folded_billing_folder() {
        // The UI shows worktree branch names (e.g. `ci-on-codebuild`) as
        // projects because `project_path` is the raw exact path. `project`
        // must carry the folded billing folder key instead — `lyfjastofnun`
        // for a block whose events live under
        // `.../lyfjastofnun/.claude/worktrees/ci-on-codebuild`.
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let path = "/home/u/Desktop/Work/lyfjastofnun/.claude/worktrees/ci-on-codebuild";
        let mut ev = Event::minimal("claude", "e0", "2026-04-18T09:05:00+00:00", "prompt");
        ev.project_path = Some(path.to_string());
        let eid = repo::upsert_event(&conn, &ev).unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![bid, eid],
        )
        .unwrap();

        let summary = stitch_day_summary(&conn, "2026-04-18").unwrap();
        assert_eq!(summary.blocks.len(), 1);
        assert_eq!(
            summary.blocks[0].project_path.as_deref(),
            Some(path),
            "project_path keeps showing the raw path for the UI's path chip"
        );
        assert_eq!(
            summary.blocks[0].project.as_deref(),
            Some("lyfjastofnun"),
            "project must be the folded billing folder, not the worktree \
             branch name"
        );
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

    // ─────────────── billing-cycle pruner settings (slice 5) ───────────────
    //
    // `WORKLOG_ENV_FILE`, `WORKLOG_PRUNE_ENABLED`,
    // `WORKLOG_BILLING_CYCLE_START_DAY` and `WORKLOG_BILLING_CLOSE_DAY` are
    // all process-global. `WORKLOG_PRUNE_ENABLED` is already guarded by
    // `PRUNE_ENV_LOCK` below (slice 4's b25/b39) — reusing that same lock
    // here, rather than a second one, is what actually serialises against
    // those tests instead of merely racing them under a different mutex.
    // Any test that reaches `post_settings`'s persist phase also points
    // `WORKLOG_ENV_FILE` at a tempdir first, exactly like `envfile.rs`'s
    // own tests, so a real developer's `~/.config/worklog/.env` is never
    // touched by the suite.
    fn clear_pruner_env() {
        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
        std::env::remove_var("WORKLOG_BILLING_CYCLE_START_DAY");
        std::env::remove_var("WORKLOG_BILLING_CLOSE_DAY");
    }

    /// B27: nothing configured anywhere — `current_settings` reports the
    /// documented defaults (spec 002 AC-017).
    #[tokio::test(flavor = "current_thread")]
    async fn b27_current_settings_reports_pruner_defaults_when_nothing_configured() {
        let _g = prune_env_lock().await;
        clear_pruner_env();

        let view = current_settings().unwrap();
        assert!(view.prune_enabled);
        assert_eq!(view.cycle_start_day, crate::purge::DEFAULT_CYCLE_START_DAY);
        assert_eq!(view.close_day, crate::purge::DEFAULT_CLOSE_DAY);
    }

    /// B28: `close_day` of 0, 32, or a non-integer are each rejected as a
    /// bad request naming the valid range, and nothing is persisted.
    #[tokio::test(flavor = "current_thread")]
    async fn b28_close_day_out_of_range_or_non_integer_rejected_without_persisting() {
        let _g = prune_env_lock().await;
        clear_pruner_env();
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        for bad_body in [
            r#"{"close_day":0}"#,
            r#"{"close_day":32}"#,
            r#"{"close_day":"abc"}"#,
        ] {
            let app = router(state_with_block());
            let resp = app
                .oneshot(
                    Request::post("/settings")
                        .header("content-type", "application/json")
                        .body(Body::from(bad_body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "body={bad_body}");
            let v = read_json(resp).await;
            assert!(
                v["error"].as_str().unwrap().contains("1..=31"),
                "error should name the valid range: {v}"
            );
        }

        // Nothing was ever persisted — the file was never even created,
        // since validation rejects before `envfile::upsert` is reached.
        assert!(
            !env_file.exists(),
            "no field should have been written to the env file"
        );

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    /// B28: one bad field (`close_day`) among several otherwise-good ones
    /// (`prune_enabled`, `cycle_start_day`) must persist NONE of them.
    #[tokio::test(flavor = "current_thread")]
    async fn b28_one_bad_field_among_several_good_ones_persists_nothing() {
        let _g = prune_env_lock().await;
        clear_pruner_env();
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"prune_enabled":false,"cycle_start_day":15,"close_day":32}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let view = current_settings().unwrap();
        assert!(
            view.prune_enabled,
            "the otherwise-good prune_enabled must not have persisted"
        );
        assert_eq!(
            view.cycle_start_day,
            crate::purge::DEFAULT_CYCLE_START_DAY,
            "the otherwise-good cycle_start_day must not have persisted"
        );
        assert_eq!(view.close_day, crate::purge::DEFAULT_CLOSE_DAY);

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    /// B29: a valid `cycle_start_day` write succeeds, is actually
    /// persisted, and changes the cutoff `cutoff_for_cycle` computes.
    /// `configured_cycle_start_day` itself skips the `.env` file fallback
    /// inside this crate's own `cfg(test)` build (mirroring
    /// `tz::configured_tz`'s hermetic-test design — see its doc comment),
    /// so the persisted value is verified the same way `envfile.rs`'s own
    /// tests do: reading the file directly.
    #[tokio::test(flavor = "current_thread")]
    async fn b29_valid_cycle_start_day_persists_and_changes_the_cutoff() {
        let _g = prune_env_lock().await;
        clear_pruner_env();
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cycle_start_day":15}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        assert_eq!(
            crate::envfile::read("WORKLOG_BILLING_CYCLE_START_DAY").as_deref(),
            Some("15"),
            "the valid value must actually be written to the env file"
        );

        let today = NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
        let default_cutoff = crate::purge::cutoff_for_cycle(
            today,
            crate::purge::DEFAULT_CYCLE_START_DAY,
            crate::purge::DEFAULT_CLOSE_DAY,
        );
        let configured_cutoff =
            crate::purge::cutoff_for_cycle(today, 15, crate::purge::DEFAULT_CLOSE_DAY);
        assert_ne!(
            default_cutoff, configured_cutoff,
            "the configured start day must change the computed cutoff"
        );

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    /// Plus: `close_day` earlier than `cycle_start_day` is rejected with
    /// its own distinct message — not the range message — since the
    /// grace period would collapse and delete the just-closed cycle
    /// immediately.
    #[tokio::test(flavor = "current_thread")]
    async fn close_day_earlier_than_cycle_start_day_rejected_with_distinct_message() {
        let _g = prune_env_lock().await;
        clear_pruner_env();
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"cycle_start_day":25,"close_day":20}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = read_json(resp).await;
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.contains("close_day") && msg.contains("cycle_start_day"),
            "should name both fields: {msg}"
        );
        assert!(
            !msg.contains("1..=31"),
            "must be a distinct message from the per-field range error: {msg}"
        );
        assert!(!env_file.exists(), "nothing may be persisted on rejection");

        std::env::remove_var("WORKLOG_ENV_FILE");
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
        let slack_token = view
            .secrets
            .iter()
            .find(|f| f.key == "slack_user_token")
            .unwrap();
        assert!(slack_token.sensitive, "slack user token must be sensitive");
        assert!(
            slack_token.value.is_none(),
            "slack user token must not echo its value"
        );
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

    /// Two WORK projects with interleaved activity across the same hour —
    /// `GET /days/:day` should report exactly one overlap spanning both.
    fn state_with_overlap() -> Shared {
        let conn = open_memory().unwrap();
        for i in 0..20i64 {
            let ts = format!("2026-04-18T10:{:02}:00+00:00", i * 3);
            let mut e = Event::minimal("claude_turn", format!("a{i}"), ts, "work");
            e.project_path = Some("/Users/dev/Desktop/Work/alpha".to_string());
            repo::upsert_event(&conn, &e).unwrap();
        }
        for i in 0..20i64 {
            let ts = format!("2026-04-18T10:{:02}:00+00:00", i * 3);
            let mut e = Event::minimal("claude_work", format!("b{i}"), ts, "work");
            e.project_path = Some("/Users/dev/Desktop/Work/beta".to_string());
            repo::upsert_event(&conn, &e).unwrap();
        }
        Arc::new(AppState {
            conn: Mutex::new(conn),
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn day_summary_reports_the_overlap() {
        let state = state_with_overlap();
        let app = router(state);
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
        let overlaps = v["overlaps"].as_array().unwrap();
        assert_eq!(overlaps.len(), 1, "{v}");
        assert!(overlaps[0]["minutes"].as_i64().unwrap() >= 10);
        assert_eq!(overlaps[0]["projects"].as_array().unwrap().len(), 2);
        assert!(overlaps[0]["allocation"].is_null());
    }

    /// The two interleaved projects from `state_with_overlap` should each
    /// get their own gap-bridged activity spans, not just the overlap they
    /// share — the lanes view needs a project's FULL activity, since
    /// `infer_lanes` only ever assigns a minute to one owner.
    #[tokio::test(flavor = "current_thread")]
    async fn day_summary_reports_activity_for_both_projects() {
        let state = state_with_overlap();
        let app = router(state);
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
        let activity = v["activity"].as_array().unwrap();
        assert_eq!(activity.len(), 2, "{v}");
        let projects: Vec<&str> = activity
            .iter()
            .map(|p| p["project"].as_str().unwrap())
            .collect();
        assert!(projects.contains(&"alpha"), "{projects:?}");
        assert!(projects.contains(&"beta"), "{projects:?}");
        for p in activity {
            let spans = p["spans"].as_array().unwrap();
            assert_eq!(spans.len(), 1, "{p}");
            assert!(!spans[0]["started_at"].as_str().unwrap().is_empty());
            assert!(!spans[0]["ended_at"].as_str().unwrap().is_empty());
        }
    }

    /// `(started_at, ended_at, project names)` for the day's one overlap.
    async fn fetch_overlap_window(state: &Shared) -> (String, String, Vec<String>) {
        let resp = router(state.clone())
            .oneshot(
                Request::get("/days/2026-04-18")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = read_json(resp).await;
        let overlap = &v["overlaps"][0];
        let started_at = overlap["started_at"].as_str().unwrap().to_string();
        let ended_at = overlap["ended_at"].as_str().unwrap().to_string();
        let projects: Vec<String> = overlap["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["project"].as_str().unwrap().to_string())
            .collect();
        (started_at, ended_at, projects)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn allocations_endpoint_saves_split_and_reinfers() {
        let state = state_with_overlap();
        let (started_at, ended_at, projects) = fetch_overlap_window(&state).await;
        assert_eq!(projects.len(), 2);

        let body = Body::from(
            serde_json::to_vec(&json!({
                "started_at": started_at,
                "ended_at": ended_at,
                "shares": { projects[0].clone(): 0.7, projects[1].clone(): 0.3 },
            }))
            .unwrap(),
        );
        let resp = router(state.clone())
            .oneshot(
                Request::post("/days/2026-04-18/allocations")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{:?}", read_json(resp).await);

        let (_, _, _) = fetch_overlap_window(&state).await; // sanity: still one overlap
        let resp = router(state)
            .oneshot(
                Request::get("/days/2026-04-18")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = read_json(resp).await;
        let overlap = &v["overlaps"][0];
        assert_eq!(overlap["allocation"]["shares"][&projects[0]], 0.7);
        assert_eq!(overlap["allocation"]["shares"][&projects[1]], 0.3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn allocations_delete_endpoint_restores_the_automatic_split() {
        let state = state_with_overlap();
        let (started_at, ended_at, projects) = fetch_overlap_window(&state).await;

        let body = Body::from(
            serde_json::to_vec(&json!({
                "started_at": started_at,
                "ended_at": ended_at,
                "shares": { projects[0].clone(): 1.0 },
            }))
            .unwrap(),
        );
        let resp = router(state.clone())
            .oneshot(
                Request::post("/days/2026-04-18/allocations")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = Body::from(
            serde_json::to_vec(&json!({ "started_at": started_at, "ended_at": ended_at })).unwrap(),
        );
        let resp = router(state.clone())
            .oneshot(
                Request::post("/days/2026-04-18/allocations/delete")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = router(state)
            .oneshot(
                Request::get("/days/2026-04-18")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = read_json(resp).await;
        assert!(v["overlaps"][0]["allocation"].is_null());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn allocations_endpoint_rejects_shares_that_dont_sum_to_one() {
        let state = state_with_overlap();
        let (started_at, ended_at, projects) = fetch_overlap_window(&state).await;

        let body = Body::from(
            serde_json::to_vec(&json!({
                "started_at": started_at,
                "ended_at": ended_at,
                "shares": { projects[0].clone(): 0.5 },
            }))
            .unwrap(),
        );
        let resp = router(state)
            .oneshot(
                Request::post("/days/2026-04-18/allocations")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn allocations_endpoint_rejects_a_project_not_in_the_overlap() {
        let state = state_with_overlap();
        let (started_at, ended_at, _) = fetch_overlap_window(&state).await;

        let body = Body::from(
            serde_json::to_vec(&json!({
                "started_at": started_at,
                "ended_at": ended_at,
                "shares": { "not-a-real-project": 1.0 },
            }))
            .unwrap(),
        );
        let resp = router(state)
            .oneshot(
                Request::post("/days/2026-04-18/allocations")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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
    async fn infer_routes_rule_hits_before_building_blocks() {
        // T007: /infer must route unsorted browser/Slack events (rule hits
        // only here — the Verdict helper is never running under test) before
        // clustering, so a rule-matched event's project_path lands on the
        // block it joins in the same request.
        let conn = open_memory().unwrap();
        billing_registry::upsert_folder(
            &conn,
            &billing_registry::FolderMap {
                id: None,
                folder: "X".into(),
                customer: None,
                verkefni: None,
                billable: true,
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO routing_rules (kind, pattern, folder) VALUES ('domain', 'aws.tomasari.is', 'X')",
            [],
        )
        .unwrap();
        // 5 events a minute apart so the resulting block clears
        // MIN_BLOCK_MINUTES (a single rule-matched event would land under
        // the 5-minute floor and never form a block at all).
        for i in 0..5 {
            let ts = format!("2026-04-20T09:0{i}:00+00:00");
            let eid = repo::upsert_event(
                &conn,
                &Event::minimal(
                    routing_contract::SOURCE_FIREFOX,
                    format!("e{i}"),
                    &ts,
                    "AWS Console",
                ),
            )
            .unwrap();
            conn.execute(
                "UPDATE events SET details = 'https://aws.tomasari.is/console' WHERE id = ?1",
                params![eid],
            )
            .unwrap();
        }

        let state = state_from_conn(conn);
        let app = router(state.clone());
        let body = Body::from(serde_json::to_vec(&json!({"day": "2026-04-20"})).unwrap());
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

        let guard = state.conn.lock().await;
        let (label_origin, project_path): (Option<String>, String) = guard
            .query_row(
                "SELECT e.label_origin, e.project_path
                   FROM events e
                   JOIN block_events be ON be.event_id = e.id
                   JOIN blocks b ON b.id = be.block_id
                  WHERE b.day = '2026-04-20'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(label_origin.as_deref(), Some("rule"));
        assert_eq!(
            project_path,
            format!("{}/X", crate::billing::work_prefix().unwrap())
        );
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

    // ───────────── POST /blocks/:id/estimate (Phase 1) ─────────────

    /// B7 at the wire: a missing block id surfaces as 404 with a body
    /// the UI can show verbatim.
    #[tokio::test(flavor = "current_thread")]
    async fn estimate_block_endpoint_returns_404_for_missing_block() {
        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/blocks/9999/estimate")
                    .header("content-type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// B4 at the wire: a personal block surfaces as 400 — the UI can
    /// show the message and hint the user to toggle to work first.
    #[tokio::test(flavor = "current_thread")]
    async fn estimate_block_endpoint_returns_400_for_personal_block() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds, is_personal)
             VALUES ('2026-04-18', '2026-04-18T09:00:00+00:00', '2026-04-18T09:30:00+00:00', 1800, 1)",
            [],
        )
        .unwrap();
        let bid = conn.last_insert_rowid();
        let state = Arc::new(AppState {
            conn: Mutex::new(conn),
        });
        let resp = router(state)
            .oneshot(
                Request::post(format!("/blocks/{bid}/estimate"))
                    .header("content-type", "application/json")
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
                .unwrap_or_default()
                .to_lowercase()
                .contains("personal"),
            "error body must mention personal: {v}"
        );
    }

    // ───────────────── billing-cycle prune due-check (slice 4) ─────────────────
    //
    // `$WORKLOG_PRUNE_ENABLED` is process-global, so any test that flips
    // it must hold this mutex. An async-aware `Mutex` (held across the
    // `.await`s below) rather than a `std::sync::Mutex`, per
    // `clippy::await_holding_lock`.
    //
    // These tests deliberately do NOT touch `$WORKLOG_HOME`. An earlier
    // version set it to a tempdir and called `new_state()`, which resolves
    // paths from that variable — but the variable is process-global and
    // this mutex does not lock against `paths::ENV_LOCK`, whose own tests
    // call `remove_var("WORKLOG_HOME")`. One such interleaving pointed the
    // prune loop at the real `~/.local/share/worklog` and deleted 783
    // blocks and ~392k events. The fix is not another mutex — the other
    // lock holders would never take it — but binding every test to an
    // explicit database via `state_from_conn` and passing explicit paths
    // into `spawn_prune_loop`, so nothing here consults the environment
    // for a path at all. `purge::run` carries a `#[cfg(test)]` backstop
    // that refuses a `db_path` under the real data directory.
    //
    // `$WORKLOG_PRUNE_ENABLED` is safe to flip under this mutex: it is
    // read by `purge::pruning_enabled` and cannot resolve to a path.
    async fn prune_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        crate::envfile::ENV_TEST_LOCK.lock().await
    }

    /// B25: pruning disabled via the env var. The due-check must return
    /// before ever touching the connection or resolving a real path —
    /// no deletion, latch unchanged.
    #[tokio::test(flavor = "current_thread")]
    async fn b25_pruning_disabled_skips_the_check_entirely() {
        let _g = prune_env_lock().await;
        std::env::set_var("WORKLOG_PRUNE_ENABLED", "0");

        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
             VALUES ('2000-01-01', '2000-01-01T09:00:00+00:00', '2000-01-01T09:30:00+00:00', 1800)",
            [],
        )
        .unwrap();
        let state = state_from_conn(conn);

        // Explicit tempdir paths: nothing here may resolve against the
        // process environment, and the disabled check must not touch them
        // anyway.
        let tmp = tempfile::tempdir().unwrap();
        let handle = spawn_prune_loop(
            state.clone(),
            tmp.path().join("worklog.db.preprune"),
            tmp.path().join("worklog.db"),
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        handle.abort();

        let guard = state.conn.lock().await;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "pruning disabled — nothing may be deleted");
        assert!(
            crate::purge::meta_get(&guard, crate::purge::LATCH_KEY)
                .unwrap()
                .is_none(),
            "latch must stay unwritten while disabled"
        );
        drop(guard);

        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
    }

    /// B39: the check-interval is exactly 6 hours, and the loop performs
    /// its first check on start — proven by observing a real deletion, a
    /// fresh latch, and a snapshot file within a small, bounded amount
    /// of real time, far less than the 6h interval. That would be
    /// impossible if the loop slept before its first check.
    #[tokio::test(flavor = "current_thread")]
    async fn b39_interval_is_six_hours_and_loop_checks_on_start() {
        assert_eq!(
            PRUNE_CHECK_INTERVAL,
            std::time::Duration::from_secs(6 * 60 * 60)
        );

        let _g = prune_env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("WORKLOG_PRUNE_ENABLED", "1");

        // Bind to an explicit database rather than `new_state()`, which
        // resolves one from the process-global `$WORKLOG_HOME`. Another
        // test clearing that variable at the wrong moment once pointed
        // this test at the real `~/.local/share/worklog` and pruned it.
        let db_path = tmp.path().join("worklog.db");
        let state = state_from_conn(crate::db::open(&db_path).unwrap());
        {
            let conn = state.conn.lock().await;
            conn.execute(
                "INSERT INTO blocks (day, started_at, ended_at, duration_seconds)
                 VALUES ('2000-01-01', '2000-01-01T09:00:00+00:00', '2000-01-01T09:30:00+00:00', 1800)",
                [],
            )
            .unwrap();
        }

        let snapshot_path = tmp.path().join("worklog.db.preprune");
        let handle = spawn_prune_loop(state.clone(), snapshot_path.clone(), db_path.clone());
        let step = std::time::Duration::from_millis(10);
        let budget = std::time::Duration::from_millis(1000);
        let mut waited = std::time::Duration::ZERO;
        while !snapshot_path.exists() && waited < budget {
            tokio::time::sleep(step).await;
            waited += step;
        }
        handle.abort();

        assert!(
            snapshot_path.is_file(),
            "expected the loop's first check to fire on start, well within \
             {budget:?} — nowhere near the {PRUNE_CHECK_INTERVAL:?} interval"
        );
        let guard = state.conn.lock().await;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "the old block must have been pruned on start");
        assert!(
            crate::purge::meta_get(&guard, crate::purge::LATCH_KEY)
                .unwrap()
                .is_some(),
            "latch must be recorded after the on-start check"
        );
        drop(guard);

        std::env::remove_var("WORKLOG_PRUNE_ENABLED");
    }

    /// The guard that exists because this actually happened: a test whose
    /// paths resolved to the owner's real data directory pruned it for
    /// real. `run` must refuse outright in test builds, before it writes a
    /// snapshot or deletes a single row.
    #[test]
    fn run_refuses_to_touch_the_real_data_directory() {
        let conn = open_memory().unwrap();
        let real_db = dirs::home_dir()
            .unwrap()
            .join(".local/share/worklog/worklog.db");
        let opts = crate::purge::PruneOptions {
            cutoff: chrono::NaiveDate::from_ymd_opt(2026, 7, 20).unwrap(),
            dry_run: false,
            snapshot_to: None,
            db_path: Some(real_db.as_path()),
        };

        let err = crate::purge::run(&conn, &opts)
            .expect_err("run must refuse a db_path inside the real data directory");
        assert!(
            err.to_string()
                .contains("refusing to prune the real data directory"),
            "unexpected error: {err}"
        );
    }

    // ─────────────── browser + Slack routing (T003) ───────────────

    const HB_BODY: &str = r#"{"ts":"2026-04-14T10:30:12Z","url":"https://aws.tomasari.is/cert","title":"AWS cert study","container":null,"incognito":false}"#;
    const HB_BODY_INCOGNITO: &str = r#"{"ts":"2026-04-14T10:30:12Z","url":"https://aws.tomasari.is/cert","title":"AWS cert study","container":null,"incognito":true}"#;

    /// B3: no `moz-extension://` Origin — 403, nothing stored. Only a
    /// real Firefox add-on can send that Origin; any other page is
    /// rejected before the body is ever ingested.
    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_rejects_web_origin() {
        let state = state_from_conn(open_memory().unwrap());
        let app = router(state.clone());
        let resp = app
            .oneshot(
                Request::post("/browser/heartbeat")
                    .header("content-type", "application/json")
                    .header("origin", "https://evil.example")
                    .body(Body::from(HB_BODY))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let guard = state.conn.lock().await;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "nothing may be stored on a rejected origin");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_stores_with_moz_extension_origin() {
        let state = state_from_conn(open_memory().unwrap());
        let app = router(state.clone());
        let resp = app
            .oneshot(
                Request::post("/browser/heartbeat")
                    .header("content-type", "application/json")
                    .header("origin", "moz-extension://abc-123")
                    .body(Body::from(HB_BODY))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["stored"], true);
        assert_eq!(v["reason"], Value::Null);

        let guard = state.conn.lock().await;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_reports_filtered_reason_and_stores_nothing() {
        let state = state_from_conn(open_memory().unwrap());
        let app = router(state.clone());
        let resp = app
            .oneshot(
                Request::post("/browser/heartbeat")
                    .header("content-type", "application/json")
                    .header("origin", "moz-extension://abc-123")
                    .body(Body::from(HB_BODY_INCOGNITO))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["stored"], false);
        assert_eq!(v["reason"], "incognito");

        let guard = state.conn.lock().await;
        let count: i64 = guard
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    /// B13: Firefox sends `OPTIONS /browser/heartbeat` before every POST.
    /// A `moz-extension://` origin gets the three CORS headers back; any
    /// other origin gets the same 403 the POST route already gives (B3).
    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_preflight_allows_moz_extension_origin() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::options("/browser/heartbeat")
                    .header("origin", "moz-extension://abc-123")
                    .header("access-control-request-headers", "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers();
        assert_eq!(
            headers.get("access-control-allow-origin").unwrap(),
            "moz-extension://abc-123"
        );
        assert_eq!(headers.get("access-control-allow-methods").unwrap(), "POST");
        assert_eq!(
            headers.get("access-control-allow-headers").unwrap(),
            "content-type"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_preflight_rejects_other_origin() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::options("/browser/heartbeat")
                    .header("origin", "https://evil.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(resp.headers().get("access-control-allow-origin").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn heartbeat_preflight_success_response_carries_allow_origin() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::post("/browser/heartbeat")
                    .header("content-type", "application/json")
                    .header("origin", "moz-extension://abc-123")
                    .body(Body::from(HB_BODY))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("access-control-allow-origin").unwrap(),
            "moz-extension://abc-123"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routed_events_rejects_bad_day() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::get("/days/not-a-day/routed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routed_events_returns_the_days_unsorted_event() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_FIREFOX,
                "e1",
                "2026-04-14T10:30:00+00:00",
                "AWS cert study",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::get("/days/2026-04-14/routed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["source"], routing_contract::SOURCE_FIREFOX);
        assert_eq!(arr[0]["folder"], Value::Null);
        assert_eq!(arr[0]["label_origin"], Value::Null);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn set_event_label_labels_an_existing_event() {
        let conn = open_memory().unwrap();
        billing_registry::upsert_folder(
            &conn,
            &billing_registry::FolderMap {
                id: None,
                folder: "demo-project".into(),
                customer: None,
                verkefni: None,
                billable: true,
            },
        )
        .unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-14T10:30:00+00:00",
                "#eng",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::post(format!("/events/{id}/label"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"folder":"demo-project","always":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["folder"], "demo-project");
        assert_eq!(v["label_origin"], "fix");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn set_event_label_404s_an_unknown_event() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::post("/events/999999/label")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"folder":"demo-project","always":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn set_event_label_400s_an_unknown_folder() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_FIREFOX,
                "e1",
                "2026-04-14T10:30:00+00:00",
                "x",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::post(format!("/events/{id}/label"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"folder":"ghost-project","always":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dismiss_event_clears_label_and_returns_routed_shape() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-14T10:30:00+00:00",
                "#random",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::post(format!("/events/{id}/dismiss"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rule_kind":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["id"], id);
        assert_eq!(v["folder"], Value::Null);
        assert_eq!(v["label_origin"], "dismissed");
        assert_eq!(v["label_confidence"], Value::Null);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dismiss_event_404s_an_unknown_event() {
        let app = router(state_from_conn(open_memory().unwrap()));
        let resp = app
            .oneshot(
                Request::post("/events/999999/dismiss")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rule_kind":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dismiss_event_400s_a_rule_kind_that_does_not_fit_the_source() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-14T10:30:00+00:00",
                "#random",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::post(format!("/events/{id}/dismiss"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rule_kind":"domain"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dismiss_event_with_rule_kind_creates_ignore_rule_listed_by_routing_rules() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-14T10:30:00+00:00",
                "#random",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/events/{id}/dismiss"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rule_kind":"slack_channel"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(Request::get("/routing/rules").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["pattern"], "#random");
        assert_eq!(arr[0]["folder"], "__ignore__");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routed_events_excludes_dismissed() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_FIREFOX,
                "e1",
                "2026-04-14T10:30:00+00:00",
                "news site",
            ),
        )
        .unwrap();
        crate::routing_dismiss::dismiss_event(&conn, id, None).unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::get("/days/2026-04-14/routed")
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
    async fn routed_events_include_hidden_returns_dismissed() {
        let conn = open_memory().unwrap();
        let id = repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_FIREFOX,
                "e1",
                "2026-04-14T10:30:00+00:00",
                "news site",
            ),
        )
        .unwrap();
        crate::routing_dismiss::dismiss_event(&conn, id, None).unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(
                Request::get("/days/2026-04-14/routed?include_hidden=true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routing_rules_list_and_delete_round_trip() {
        let conn = open_memory().unwrap();
        conn.execute(
            "INSERT INTO routing_rules (kind, pattern, folder)
             VALUES ('domain', 'aws.tomasari.is', 'aws-cert')",
            [],
        )
        .unwrap();
        let rule_id = conn.last_insert_rowid();

        let app = router(state_from_conn(conn));

        let resp = app
            .clone()
            .oneshot(Request::get("/routing/rules").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["pattern"], "aws.tomasari.is");

        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/routing/rules/{rule_id}/delete"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["removed"], true);

        let resp = app
            .oneshot(Request::get("/routing/rules").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let v = read_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn routing_status_reports_last_heartbeat_and_last_slack() {
        let conn = open_memory().unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_FIREFOX,
                "e1",
                "2026-04-14T10:30:00+00:00",
                "x",
            ),
        )
        .unwrap();
        repo::upsert_event(
            &conn,
            &Event::minimal(
                routing_contract::SOURCE_SLACK,
                "s1",
                "2026-04-14T11:00:00+00:00",
                "#eng",
            ),
        )
        .unwrap();

        let app = router(state_from_conn(conn));
        let resp = app
            .oneshot(Request::get("/routing/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = read_json(resp).await;
        assert_eq!(v["last_heartbeat"], "2026-04-14T10:30:00+00:00");
        assert_eq!(v["last_slack"], "2026-04-14T11:00:00+00:00");
        assert_eq!(
            v["classifier_reachable"], false,
            "no Verdict helper is running under test"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_reports_work_hours_and_ratio_defaults() {
        let _g = prune_env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("WORKLOG_ENV_FILE", tmp.path().join(".env"));

        let view = current_settings().unwrap();
        assert_eq!(view.work_hours, routing_contract::DEFAULT_WORK_HOURS);
        assert_eq!(
            view.abstain_margin,
            routing_contract::DEFAULT_ABSTAIN_MARGIN
        );
        assert_eq!(
            view.runner_up_ratio,
            routing_contract::DEFAULT_RUNNER_UP_RATIO
        );

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_post_rejects_bad_work_hours_or_ratios_without_persisting() {
        let _g = prune_env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        for bad_body in [
            r#"{"work_hours":"garbage"}"#,
            r#"{"abstain_margin":0.5}"#,
            r#"{"runner_up_ratio":9.0}"#,
        ] {
            let app = router(state_with_block());
            let resp = app
                .oneshot(
                    Request::post("/settings")
                        .header("content-type", "application/json")
                        .body(Body::from(bad_body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "body={bad_body}");
        }
        assert!(!env_file.exists());

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_post_persists_valid_work_hours_and_ratios() {
        let _g = prune_env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("WORKLOG_ENV_FILE", tmp.path().join(".env"));

        let app = router(state_with_block());
        let resp = app
            .oneshot(
                Request::post("/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"work_hours":"Mon-Fri 08:00-16:00","abstain_margin":1.2,"runner_up_ratio":1.3}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            crate::envfile::read(routing_contract::WORK_HOURS_KEY).as_deref(),
            Some("Mon-Fri 08:00-16:00")
        );
        assert_eq!(
            crate::envfile::read(routing_contract::ABSTAIN_MARGIN_KEY).as_deref(),
            Some("1.2")
        );
        assert_eq!(
            crate::envfile::read(routing_contract::RUNNER_UP_RATIO_KEY).as_deref(),
            Some("1.3")
        );
        let rule = configured_route_rule();
        assert_eq!(rule.abstain_margin, 1.2);
        assert_eq!(rule.runner_up_ratio, 1.3);

        std::env::remove_var("WORKLOG_ENV_FILE");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn configured_route_rule_falls_back_to_defaults_on_bad_envfile_values() {
        let _g = prune_env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join(".env");
        std::env::set_var("WORKLOG_ENV_FILE", &env_file);

        crate::envfile::upsert(routing_contract::ABSTAIN_MARGIN_KEY, "not-a-number").unwrap();
        crate::envfile::upsert(routing_contract::RUNNER_UP_RATIO_KEY, "9.0").unwrap();

        let rule = configured_route_rule();
        assert_eq!(
            rule.abstain_margin,
            routing_contract::DEFAULT_ABSTAIN_MARGIN
        );
        assert_eq!(
            rule.runner_up_ratio,
            routing_contract::DEFAULT_RUNNER_UP_RATIO
        );

        std::env::remove_var("WORKLOG_ENV_FILE");
    }
}
