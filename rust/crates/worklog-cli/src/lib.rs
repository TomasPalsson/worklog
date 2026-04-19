//! worklog-cli — shared entrypoint so `main.rs` stays trivial and tests can
//! drive the CLI via `cargo test`.

#![forbid(unsafe_code)]

pub mod cli;
pub mod style;
pub mod wizard;

pub use cli::run;
