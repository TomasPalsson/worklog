//! worklog — CLI entrypoint.
//!
//! Stage 1 skeleton: version/db/doctor/secret subcommands.

#![forbid(unsafe_code)]

fn main() -> anyhow::Result<()> {
    worklog_cli::run()
}
