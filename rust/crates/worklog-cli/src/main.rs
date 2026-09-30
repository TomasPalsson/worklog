//! worklog — CLI entrypoint.
//!
//! Wraps the internal `anyhow` pipeline in a `miette::Result` at the very
//! outer boundary. Every library function still returns `anyhow::Result`;
//! we hand-convert the error to a `miette::Report` at the point of exit
//! so the user sees a boxed, colored diagnostic instead of a flat
//! `Error: …` line. `{e:#}` on `anyhow::Error` flattens the whole chain
//! into one readable string — good enough for a CLI binary.

#![forbid(unsafe_code)]

fn main() -> miette::Result<()> {
    worklog_cli::run().map_err(|e| {
        if e.chain().any(|c| {
            c.to_string()
                .starts_with(worklog_core::digest_contract::DAY_COMPRESSED)
        }) {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
        miette::Report::msg(format!("{e:#}"))
    })
}
