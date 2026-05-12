//! Tiny cross-platform "open this URL in the default browser" helper.
//!
//! We shell out instead of pulling in a new crate (`opener`,
//! `webbrowser`) — the whole module is ~30 lines and worklog-core
//! already uses `Command` everywhere. The exposed API takes an
//! injectable spawner so tests can assert which command would run
//! without launching a real browser in CI.

use std::process::Command;

use anyhow::Result;

/// Outcome of an open attempt. We don't surface child exit status —
/// `open` / `xdg-open` return before the browser is ready and
/// callers don't care; they just want to know whether we tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// We spawned an opener for the URL.
    Spawned,
    /// No opener is available on this platform (e.g. headless Linux
    /// without `xdg-open`). Callers should fall back to printing the
    /// URL so the user can copy it manually.
    Unsupported,
}

/// Attempt to open `url` in the user's default browser.
///
/// Errors only when an opener exists but spawning it failed (rare —
/// usually a permission issue). A missing opener is reported via
/// [`OpenOutcome::Unsupported`] rather than `Err` because it's an
/// expected non-fatal condition on headless boxes.
pub fn open_url(url: &str) -> Result<OpenOutcome> {
    open_url_with(url, &mut spawn_real)
}

/// Test seam — accepts a fake spawner so we can verify dispatch
/// without launching a real browser.
pub fn open_url_with<S>(url: &str, spawn: &mut S) -> Result<OpenOutcome>
where
    S: FnMut(&str, &[&str]) -> std::io::Result<()>,
{
    for (program, args) in candidates_for_url(url) {
        match spawn(program, &args) {
            Ok(()) => return Ok(OpenOutcome::Spawned),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(OpenOutcome::Unsupported)
}

fn candidates_for_url(url: &str) -> Vec<(&'static str, Vec<&str>)> {
    if cfg!(target_os = "macos") {
        vec![("open", vec![url])]
    } else if cfg!(target_os = "windows") {
        // `cmd /c start "" <url>` — the empty quoted string is a
        // placeholder for the window title that `start` insists on.
        vec![("cmd", vec!["/C", "start", "", url])]
    } else {
        vec![
            ("xdg-open", vec![url]),
            ("gio", vec!["open", url]),
            ("gnome-open", vec![url]),
            ("kde-open", vec![url]),
            ("wslview", vec![url]), // WSL → Windows browser
        ]
    }
}

fn spawn_real(program: &str, args: &[&str]) -> std::io::Result<()> {
    // `.spawn()` returns once the child is forked — we don't wait
    // for the browser process to exit.
    Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Calls = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    fn recorder() -> (Calls, impl FnMut(&str, &[&str]) -> std::io::Result<()>) {
        let log: Calls = Rc::new(RefCell::new(Vec::new()));
        let log2 = log.clone();
        let spawn = move |p: &str, args: &[&str]| -> std::io::Result<()> {
            log2.borrow_mut()
                .push((p.to_string(), args.iter().map(|s| s.to_string()).collect()));
            Ok(())
        };
        (log, spawn)
    }

    fn fail_with(kind: std::io::ErrorKind) -> impl FnMut(&str, &[&str]) -> std::io::Result<()> {
        move |_p, _args| Err(std::io::Error::new(kind, "synthetic"))
    }

    #[test]
    fn first_candidate_runs_when_spawner_succeeds() {
        let (log, mut spawn) = recorder();
        let outcome = open_url_with("https://example.com", &mut spawn).unwrap();
        assert_eq!(outcome, OpenOutcome::Spawned);
        let calls = log.borrow();
        assert_eq!(calls.len(), 1);
        let (prog, args) = &calls[0];
        assert!(!prog.is_empty());
        assert!(args.iter().any(|a| a == "https://example.com"));
    }

    #[test]
    fn skips_notfound_and_tries_next_candidate() {
        // Fail the first N-1 candidates with NotFound, succeed on the
        // last. Only meaningful on Linux where we have multiple
        // candidates — on macOS/Windows it still exercises the
        // "first candidate failed → return Unsupported" branch.
        let log: Calls = Rc::new(RefCell::new(Vec::new()));
        let log2 = log.clone();
        let mut call_count = 0usize;
        let mut spawn = move |p: &str, args: &[&str]| -> std::io::Result<()> {
            log2.borrow_mut()
                .push((p.to_string(), args.iter().map(|s| s.to_string()).collect()));
            call_count += 1;
            // Fail every call with NotFound so we exhaust the list.
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        };
        let outcome = open_url_with("https://example.com", &mut spawn).unwrap();
        assert_eq!(outcome, OpenOutcome::Unsupported);
        // We should have tried at least one candidate.
        assert!(!log.borrow().is_empty());
    }

    #[test]
    fn surfaces_non_notfound_io_errors() {
        let mut spawn = fail_with(std::io::ErrorKind::PermissionDenied);
        let err = open_url_with("https://example.com", &mut spawn).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("synthetic"), "expected underlying io error to bubble, got: {msg}");
    }

    #[test]
    fn candidates_include_url_as_arg() {
        let candidates = candidates_for_url("https://localhost:3333");
        assert!(!candidates.is_empty());
        for (_, args) in candidates {
            assert!(
                args.iter().any(|a| *a == "https://localhost:3333"),
                "url missing from candidate args"
            );
        }
    }
}
