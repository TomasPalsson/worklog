//! worklog-core — shared data layer for the Rust rewrite.
//!
//! Stage 1 skeleton: paths, db, models, repository, secrets.

#![forbid(unsafe_code)]

pub mod block_service;
pub mod collectors;
pub mod daemon;
pub mod daemon_service;
pub mod db;
pub mod estimate;
pub mod hook;
pub mod hook_run;
pub mod http;
pub mod infer;
pub mod models;
pub mod paths;
pub mod personal;
pub mod purge;
pub mod repo;
pub mod schedule;
pub mod secrets;
pub mod sessions;
pub mod tz;
pub mod updater;
pub mod web;

pub use crate::paths::Paths;

/// Crate version, pinned to the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
