//! Thin, consistent output styling for the CLI.
//!
//! Goal: every user-facing line flows through one of four helpers so
//! the look stays coherent across commands. When stdout isn't a TTY
//! (pipes, CI, `assert_cmd` tests) `console` automatically strips ANSI
//! codes, so tests that match on the marker character keep passing
//! without special-casing.
//!
//! Marker conventions:
//!
//! * `ok`   — `✓` green: operation succeeded
//! * `warn` — `!` yellow: recoverable / skipped / informational warning
//! * `info` — `·` dim: side-channel info, not a result
//! * `fail` — `✗` red: hard failure (usually paired with a non-zero
//!   exit downstream)
//! * `step` — `▶` bold: section header inside multi-step flows
//!   (e.g. "collecting …")

use console::style;
use std::io::Write;

/// Render `✓ <msg>` in green. Caller passes the already-formatted
/// message body. Newline is appended.
pub fn ok<W: Write>(out: &mut W, msg: &str) -> std::io::Result<()> {
    writeln!(out, "{} {msg}", style("✓").green().bold())
}

/// Render `! <msg>` in yellow for warnings / skipped collectors / the
/// rest of a pipeline still making progress.
pub fn warn<W: Write>(out: &mut W, msg: &str) -> std::io::Result<()> {
    writeln!(out, "{} {msg}", style("!").yellow().bold())
}

/// Render `· <msg>` dim. Use for side-channel info like "(no blocks
/// to estimate)" where neither ok nor warn is quite right.
pub fn info<W: Write>(out: &mut W, msg: &str) -> std::io::Result<()> {
    writeln!(out, "{} {msg}", style("·").dim())
}

/// Render `✗ <msg>` in red. Reserved for hard failures surfaced to the
/// user; the caller is expected to propagate a non-zero exit separately.
pub fn fail<W: Write>(out: &mut W, msg: &str) -> std::io::Result<()> {
    writeln!(out, "{} {msg}", style("✗").red().bold())
}

/// Render `▶ <msg>` bold for section headers in multi-step commands
/// (e.g. `worklog day`'s "collecting…", "inferring…", "estimating…").
/// Prefixes a blank line so sections breathe.
pub fn step<W: Write>(out: &mut W, msg: &str) -> std::io::Result<()> {
    writeln!(out)?;
    writeln!(out, "{} {msg}", style("▶").cyan().bold())
}

/// Best-effort spinner for network / long-running work. Returns a
/// `ProgressBar` that the caller finishes with `.finish_and_clear()` or
/// `.finish_with_message(...)`. When stdout isn't a TTY, indicatif
/// silently draws to a null target — tests see nothing, humans see a
/// ticking spinner.
pub fn spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("valid indicatif template"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(msg.to_owned());
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `console` strips ANSI when writing to a non-tty, so tests that
    /// use substring matching on the marker characters (✓ / ! / · / ✗
    /// / ▶) remain stable. Here we confirm the marker survives into
    /// the buffer unadorned.
    #[test]
    fn markers_preserved_when_writing_to_a_buffer() {
        let mut buf = Vec::new();
        ok(&mut buf, "done").unwrap();
        warn(&mut buf, "skipped").unwrap();
        info(&mut buf, "idle").unwrap();
        fail(&mut buf, "broken").unwrap();
        step(&mut buf, "collecting").unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("✓ done"), "got: {s:?}");
        assert!(s.contains("! skipped"), "got: {s:?}");
        assert!(s.contains("· idle"), "got: {s:?}");
        assert!(s.contains("✗ broken"), "got: {s:?}");
        assert!(s.contains("▶ collecting"), "got: {s:?}");
    }

    #[test]
    fn step_inserts_a_blank_line_before_the_marker() {
        // Visual breathing room between sections is part of the
        // contract — don't let future refactors silently remove it.
        let mut buf = Vec::new();
        step(&mut buf, "go").unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with('\n'), "step should prefix a blank line, got: {s:?}");
    }
}
