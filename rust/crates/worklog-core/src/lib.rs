//! worklog-core — shared data layer for the Rust rewrite.
//!
//! Stage 1 skeleton: paths, db, models, repository, secrets.

#![forbid(unsafe_code)]

pub mod billing;
pub mod billing_registry;
pub mod block_service;
pub mod browser;
pub mod browser_ingest;
pub mod collectors;
pub mod daemon;
pub mod daemon_service;
pub mod db;
pub mod envfile;
pub mod estimate;
pub mod git;
pub mod hook;
pub mod hook_run;
pub mod http;
pub mod infer;
pub mod infer_allocations;
mod infer_carry;
pub mod infer_lanes;
pub mod models;
pub mod overlap_store;
pub mod overlaps;
pub mod paths;
pub mod personal;
pub mod purge;
pub mod repo;
pub mod routing;
pub mod routing_absorb;
pub mod routing_contract;
pub mod routing_dismiss;
pub mod schedule;
pub mod secrets;
pub mod sessions;
pub mod skill;
pub mod timeline;
pub mod tz;
pub mod updater;
pub mod verdict;
pub mod web;

pub use crate::paths::Paths;

/// Crate version, pinned to the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
