//! worklog-core — shared data layer for the Rust rewrite.
//!
//! Stage 1 skeleton: paths, db, models, repository, secrets.

#![forbid(unsafe_code)]

pub mod db;
pub mod hook;
pub mod models;
pub mod paths;
pub mod repo;
pub mod schedule;
pub mod secrets;

pub use crate::paths::Paths;

/// Crate version, pinned to the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
