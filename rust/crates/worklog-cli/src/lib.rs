//! worklog-cli — shared entrypoint so `main.rs` stays trivial and tests can
//! drive the CLI via `cargo test`.

#![forbid(unsafe_code)]

pub mod cli;
pub mod daemon_client;
mod eval_cmd;
mod helpers_cmd;
pub mod style;
mod ticket_cmd;
pub mod wizard;

pub use cli::run;
