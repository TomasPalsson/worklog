//! worklog-core — shared data layer for the Rust rewrite.
//!
//! Stage 1 skeleton: paths, db, models, repository, secrets.

#![forbid(unsafe_code)]

pub mod billing;
pub mod billing_deildir;
pub mod billing_registry;
pub mod block_details;
pub mod block_digest;
pub mod block_eval;
pub mod block_service;
pub mod browser;
pub mod browser_ingest;
pub mod change_log;
mod change_log_column;
mod claude_subprocess;
mod clues_collect;
pub mod clues_contract;
pub mod clues_send;
pub mod clues_work_items;
pub mod collectors;
pub mod daemon;
pub mod daemon_service;
pub mod db;
pub mod deild_contract;
pub mod digest_contract;
pub mod elsewhere;
pub mod envfile;
pub mod estimate;
pub mod git;
pub mod hook;
pub mod hook_run;
pub mod http;
pub mod infer;
pub mod infer_allocations;
mod infer_carry;
mod infer_carry_shares;
mod infer_evidence;
mod infer_lane_tags;
pub mod infer_lanes;
pub mod line_text;
mod line_text_jobs;
pub mod local_clone;
pub mod models;
pub mod overlap_store;
pub mod overlaps;
pub mod paths;
pub mod personal;
pub mod prompt_snippets;
pub mod purge;
pub mod raw_json;
pub mod repo;
pub mod routing;
pub mod routing_absorb;
pub mod routing_contract;
pub mod routing_dismiss;
pub mod schedule;
pub mod scrub;
mod scrub_assignment;
pub mod secrets;
mod session_customers;
pub mod session_pins;
pub mod sessions;
pub mod skill;
pub mod tenant_clues;
pub mod tenant_contract;
pub mod tenant_shares;
pub mod tenant_split;
pub mod tenants;
pub mod timeline;
pub mod tz;
pub mod updater;
pub mod upgrade_006;
pub mod verdict;
pub mod web;

pub use crate::paths::Paths;

/// Crate version, pinned to the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
