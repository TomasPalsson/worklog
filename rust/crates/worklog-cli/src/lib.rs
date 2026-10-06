//! worklog-cli — shared entrypoint so `main.rs` stays trivial and tests can
//! drive the CLI via `cargo test`.

#![forbid(unsafe_code)]

pub mod cli;
pub mod daemon_client;
mod eval_cmd;
pub mod style;
mod ticket_cmd;
mod ticket_pick;
pub mod wizard;

pub use cli::run;
