//! Event collectors. Each module talks to one external service, turns its
//! response into [`crate::models::Event`] rows (or, for Jira, ticket cache
//! rows), and upserts via the repo layer.
//!
//! Collectors are **blocking** in Stage 2 — they are one-shot CLI
//! invocations, not long-running services. Stage 3's axum IPC server will
//! invoke them from async code via `tokio::task::spawn_blocking`.

pub mod gcal;
pub mod github;
pub mod jira;
pub mod tempo;

/// Shared snapshot returned by every collector's public entrypoint.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct CollectReport {
    pub source: &'static str,
    pub tickets_written: usize,
    pub events_written: usize,
    pub synced: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}
