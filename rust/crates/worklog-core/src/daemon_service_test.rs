use std::cell::Cell;
use std::time::Duration;

#[test]
fn await_healthy_ok_when_probe_answers_within_budget() {
    // catches: failing on the first unanswered probe (no polling)
    let n = Cell::new(0);
    let r = await_healthy(
        "127.0.0.1:1",
        |_| {
            n.set(n.get() + 1);
            n.get() == 3
        },
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    assert!(r.is_ok());
    assert_eq!(n.get(), 3);
}

#[test]
fn await_healthy_errors_when_probe_never_answers() {
    // catches: returning Ok without checking health
    let e = await_healthy(
        "127.0.0.1:1",
        |_| false,
        Duration::from_millis(20),
        Duration::from_millis(1),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("127.0.0.1:1") && e.contains("20ms"), "{e}");
}

use super::*;

#[test]
fn service_env_path_includes_the_user_bin_dir_that_holds_claude() {
    // Regression: the unit hardcoded a system-only PATH, so the daemon
    // could not spawn `claude` (installed to ~/.local/bin). Every
    // estimate started from the web UI failed with "spawning `claude`"
    // while the same command worked in a terminal.
    let path = service_env_path();
    let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
    let local_bin = format!("{home}/.local/bin");
    assert!(
        path.split(':').any(|p| p == local_bin),
        "~/.local/bin must be on the service PATH; got {path}"
    );
    // System dirs are still there, and user dirs come first so a local
    // install wins like it would in a login shell.
    assert!(path.split(':').any(|p| p == "/usr/bin"), "got {path}");
    let first_sys = path.split(':').position(|p| p == "/usr/bin").unwrap();
    let user = path.split(':').position(|p| p == local_bin).unwrap();
    assert!(
        user < first_sys,
        "user bin dirs must precede system: {path}"
    );
}
use crate::schedule::ENV_LOCK;
use tempfile::tempdir;

// Shared between schedule.rs tests and these — both redirect the same
// env var. Avoiding interleaving keeps platform probes stable.

fn redirect(dir: &Path) -> std::sync::MutexGuard<'static, ()> {
    let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(ENV_SCHEDULE_HOME, dir);
    g
}

fn restore() {
    std::env::remove_var(ENV_SCHEDULE_HOME);
}

fn serve_health() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for mut c in l.incoming().flatten() {
            let _ = c.read(&mut [0u8; 1024]);
            let _ =
                c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    addr
}

fn serve_versions() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for (i, mut c) in l.incoming().flatten().enumerate() {
            let _ = c.read(&mut [0u8; 1024]);
            let body = if i == 0 {
                r#"{"pid":1}"#
            } else {
                r#"{"pid":2}"#
            };
            let _ = c.write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{body}")
                    .as_bytes(),
            );
        }
    });
    addr
}

const SHORT: Duration = Duration::from_millis(50);

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_fails_when_the_load_step_fails() {
    // catches: `let _ =` / dropped `?` on launchctl load / systemctl enable
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let step = crate::schedule::fake_tool(tmp.path(), 1);
    let r = install_at(
        "worklog daemon",
        &serve_health(),
        SHORT,
        Duration::from_millis(1),
    );
    restore();
    let e = r.unwrap_err().to_string();
    assert!(e.contains(step) && e.contains("boom"), "{e}");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_fails_when_the_daemon_never_answers_health() {
    // catches: install() not calling await_healthy after a clean load
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let r = install_at(
        "worklog daemon",
        "127.0.0.1:1",
        SHORT,
        Duration::from_millis(1),
    );
    restore();
    let e = r.unwrap_err().to_string();
    assert!(
        e.contains("did not answer") && e.contains("127.0.0.1:1"),
        "{e}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_ok_when_load_succeeds_and_daemon_answers() {
    // catches: failing the wait even when /health answers
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let r = install_at(
        "worklog daemon",
        &serve_versions(),
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    restore();
    assert!(r.unwrap().installed);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_fails_when_the_old_daemon_is_still_answering() {
    // catches: treating any 2xx /health as proof the new daemon is up
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let r = install_at(
        "worklog daemon",
        &serve_health(),
        SHORT,
        Duration::from_millis(1),
    );
    restore();
    assert!(r.unwrap_err().to_string().contains("did not answer"));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn ensure_running_waits_on_the_callers_address_once() {
    // catches: install() waiting on the hardcoded 9323 instead of `tcp`
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let r = ensure_running_at("127.0.0.1:1", SHORT, Duration::from_millis(1));
    restore();
    let e = r.unwrap_err().to_string();
    assert!(
        e.contains("did not answer") && e.contains("127.0.0.1:1"),
        "{e}"
    );
    assert!(!e.contains("9323"), "{e}");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn ensure_running_waits_after_install_when_no_service_tool_exists() {
    // catches: returning Installed straight after install_at, which skips its wait without a tool
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let r = ensure_running_at("127.0.0.1:1", SHORT, Duration::from_millis(1));
    restore();
    let e = r.unwrap_err().to_string();
    assert!(e.contains("did not answer"), "{e}");
}

fn serve_restartable(flag: PathBuf) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        let mut restarted = false;
        for mut c in l.incoming().flatten() {
            let _ = c.read(&mut [0u8; 1024]);
            restarted |= flag.exists();
            let resp: &[u8] = if flag.exists() {
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            } else if restarted {
                b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{\"pid\":2}"
            } else {
                b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{\"pid\":1}"
            };
            let _ = c.write_all(resp);
        }
    });
    addr
}

fn fake_restartable_tool(home: &Path, flag: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let name = if cfg!(target_os = "macos") {
        "launchctl"
    } else {
        "systemctl"
    };
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let f = bin.join(name);
    let flag = flag.display();
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> {flag}.log\ncase \"$*\" in\n*unload*|*stop*) touch {flag};;\n*load*|*enable*) rm -f {flag};;\nesac\n"
    );
    std::fs::write(&f, script).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_over_a_same_version_daemon_that_restarts_instantly_succeeds() {
    // catches: demanding a down probe AFTER the load step (a fast restart is never seen down)
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let flag = tmp.path().join("stopped");
    fake_restartable_tool(tmp.path(), &flag);
    let r = install_at(
        "worklog daemon",
        &serve_restartable(flag),
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    restore();
    assert!(r.unwrap().installed);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_over_an_unmanaged_daemon_fails_naming_the_stop_and_the_fix() {
    // catches: dropping `old_gone?` (Ok: the post-load wait passes on the old daemon),
    // inverting `b != *old` (Ok: the stop wait passes at once), and a bare
    // "did not answer" that never tells the user to stop the hand-started daemon
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let tcp = serve_health();
    let r = install_at("worklog daemon", &tcp, SHORT, Duration::from_millis(1));
    restore();
    let e = r.unwrap_err().to_string();
    assert!(e.contains("did not answer the stop request"), "{e}");
    assert!(e.contains(&tcp), "{e}");
    assert!(e.contains("started outside"), "{e}");
}

fn serve_v1_with_a_slow_second_answer() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for (i, mut c) in l.incoming().flatten().enumerate() {
            let _ = c.read(&mut [0u8; 1024]);
            if i == 1 {
                std::thread::sleep(Duration::from_millis(400));
            }
            let _ = c.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{\"pid\":1}",
            );
        }
    });
    addr
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_does_not_take_a_timed_out_probe_as_the_old_daemon_gone() {
    // catches: treating a probe timeout (old daemon slow under load) as proof the stop worked
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    crate::schedule::fake_tool(tmp.path(), 0);
    let tcp = serve_v1_with_a_slow_second_answer();
    let r = install_at(
        "worklog daemon",
        &tcp,
        Duration::from_secs(1),
        Duration::from_millis(1),
    );
    restore();
    let e = r.unwrap_err().to_string();
    assert!(e.contains("did not answer the stop request"), "{e}");
}

const PID_REPLY: &str = "HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n";

fn serve_fixed_pid_with_a_down_window(flag: PathBuf) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for mut c in l.incoming().flatten() {
            let _ = c.read(&mut [0u8; 1024]);
            let resp = if flag.exists() {
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                format!("{PID_REPLY}{{\"pid\":7}}")
            };
            let _ = c.write_all(resp.as_bytes());
        }
    });
    addr
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_fails_when_the_daemon_after_load_still_has_the_old_pid() {
    // catches: accepting any 2xx /health after load (the old process came back or never left)
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let flag = tmp.path().join("stopped");
    fake_restartable_tool(tmp.path(), &flag);
    let tcp = serve_fixed_pid_with_a_down_window(flag);
    let r = install_at("worklog daemon", &tcp, SHORT, Duration::from_millis(1));
    restore();
    let e = r.unwrap_err().to_string();
    assert!(e.contains("did not answer"), "{e}");
}

fn serve_slow_first_then_pid_two(flag: PathBuf) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for (i, mut c) in l.incoming().flatten().enumerate() {
            let _ = c.read(&mut [0u8; 1024]);
            if i == 0 {
                std::thread::sleep(Duration::from_millis(400));
            }
            let resp = if flag.exists() {
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                format!("{PID_REPLY}{{\"pid\":2}}")
            };
            let _ = c.write_all(resp.as_bytes());
        }
    });
    addr
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_stops_the_service_when_the_pre_probe_times_out() {
    // catches: reading a timed-out pre-probe as "nothing running" and skipping the stop
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let flag = tmp.path().join("stopped");
    fake_restartable_tool(tmp.path(), &flag);
    let tcp = serve_slow_first_then_pid_two(flag.clone());
    let r = install_at(
        "worklog daemon",
        &tcp,
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    restore();
    let log = std::fs::read_to_string(format!("{}.log", flag.display())).unwrap_or_default();
    // launchd's install() unloads on its own, so the pre-install stop is the second one
    let stops = log
        .lines()
        .filter(|l| l.contains("unload") || l.contains("stop"))
        .count();
    assert_eq!(
        stops,
        if cfg!(target_os = "macos") { 2 } else { 1 },
        "{log}"
    );
    assert!(r.unwrap().installed);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn install_with_nothing_running_passes_on_the_first_pid_it_sees() {
    // catches: demanding a pid different from a pre-install pid that never existed
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let flag = tmp.path().join("stopped");
    fake_restartable_tool(tmp.path(), &flag);
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp = probe.local_addr().unwrap().to_string();
    drop(probe);
    let log = PathBuf::from(format!("{}.log", flag.display()));
    let addr = tcp.clone();
    std::thread::spawn(move || {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while !log.exists() && std::time::Instant::now() < until {
            std::thread::yield_now();
        }
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind(&addr).unwrap();
        for mut c in l.incoming().flatten() {
            let _ = c.read(&mut [0u8; 1024]);
            let _ = c.write_all(format!("{PID_REPLY}{{\"pid\":5}}").as_bytes());
        }
    });
    let r = install_at(
        "worklog daemon",
        &tcp,
        Duration::from_secs(5),
        Duration::from_millis(1),
    );
    restore();
    assert!(r.unwrap().installed);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_install_writes_plist_with_keepalive() {
    // B11: macOS install produces a plist with KeepAlive=true,
    // RunAtLoad=true, and ProgramArguments pointing at the worklog
    // binary + `daemon`. These are the flags launchd needs to treat
    // this as a long-running supervised process (unlike the schedule
    // plist which is a periodic one-shot).
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let status = install("/usr/local/bin/worklog daemon").unwrap();
    assert!(status.installed);
    let plist = status.unit_path.clone().unwrap();
    assert!(plist.is_file(), "plist should exist at {}", plist.display());
    let body = std::fs::read_to_string(&plist).unwrap();
    assert!(body.contains("<key>KeepAlive</key>"), "missing KeepAlive");
    assert!(body.contains("<true/>"));
    assert!(body.contains("<key>RunAtLoad</key>"), "missing RunAtLoad");
    assert!(body.contains("/usr/local/bin/worklog daemon"));
    restore();
}

#[cfg(target_os = "linux")]
#[test]
fn linux_install_writes_service_unit_with_restart() {
    // B12: linux install produces a systemd user unit with
    // Restart=on-failure and WantedBy=default.target. Note: NO
    // timer — the daemon is long-lived, not periodic.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let status = install("/usr/local/bin/worklog daemon").unwrap();
    assert!(status.installed);
    let unit = status.unit_path.clone().unwrap();
    assert!(unit.is_file());
    let body = std::fs::read_to_string(&unit).unwrap();
    assert!(body.contains("Restart=on-failure"), "missing Restart line");
    assert!(body.contains("WantedBy=default.target"), "wrong target");
    assert!(body.contains("ExecStart=/usr/local/bin/worklog daemon"));
    restore();
}

#[test]
fn install_is_idempotent() {
    // B13: calling install() twice is safe — the second call
    // atomic-overwrites and returns installed=true, no orphan unit.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let first = install("worklog daemon").unwrap();
    let second = install("worklog daemon").unwrap();
    assert!(first.installed && second.installed);
    assert_eq!(first.unit_path, second.unit_path);
    restore();
}

#[test]
fn uninstall_removes_unit_file() {
    // B14: after uninstall, the unit file is gone and status reports
    // installed=false.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let _ = install("worklog daemon").unwrap();
    let status = uninstall().unwrap();
    assert!(!status.installed);
    if let Some(p) = status.unit_path {
        assert!(!p.exists(), "unit file should be gone after uninstall");
    }
    restore();
}

#[test]
fn status_reports_installed_when_unit_exists() {
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let _ = install("worklog daemon").unwrap();
    let st = status().unwrap();
    assert!(st.installed);
    restore();
}

#[test]
fn status_reports_not_installed_on_fresh_home() {
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let st = status().unwrap();
    assert!(!st.installed);
    restore();
}

#[test]
fn default_command_is_stable_string() {
    // Regardless of whether `worklog` is on PATH, the command must
    // end in `" daemon"` so the service unit gets the right target.
    let cmd = default_command();
    assert!(cmd.ends_with(" daemon"), "got: {cmd}");
}

// ─────────────────── post-upgrade restart (v0.6) ───────────────────

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn restart_skips_when_service_not_installed() {
    // B18: fresh home, no service unit → NotInstalled. Nothing to
    // cycle, no error.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    let kicked = std::sync::atomic::AtomicBool::new(false);
    let outcome = restart_if_running_with(
        "127.0.0.1:9999",
        |_| true, // probe would say "running" — but status() fails first
        || {
            kicked.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, RestartOutcome::NotInstalled);
    assert!(
        !kicked.load(std::sync::atomic::Ordering::SeqCst),
        "must not kick when service not installed"
    );
    restore();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn restart_skips_when_service_not_running() {
    // B17: installed but not running (user stopped it manually). We
    // shouldn't surprise them by starting it — just report the state.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    install("worklog daemon").unwrap();
    let kicked = std::sync::atomic::AtomicBool::new(false);
    let outcome = restart_if_running_with(
        "127.0.0.1:9999",
        |_| false, // probe says not running
        || {
            kicked.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, RestartOutcome::NotRunning);
    assert!(
        !kicked.load(std::sync::atomic::Ordering::SeqCst),
        "must not kick when probe says not running"
    );
    restore();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn restart_kicks_when_installed_and_running() {
    // B16: installed AND responding to /health → Restarted, and the
    // kick function is actually invoked.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    install("worklog daemon").unwrap();
    let kicked = std::sync::atomic::AtomicBool::new(false);
    let outcome = restart_if_running_with(
        "127.0.0.1:9999",
        |_| true,
        || {
            kicked.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, RestartOutcome::Restarted);
    assert!(
        kicked.load(std::sync::atomic::Ordering::SeqCst),
        "kick function must be called when service is up"
    );
    restore();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn restart_propagates_kick_errors() {
    // Kick failures surface to the caller — `worklog upgrade` will
    // swallow them (binary swap was still successful), but they
    // need to reach the caller first so the log line can be honest.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    install("worklog daemon").unwrap();
    let err = restart_if_running_with(
        "127.0.0.1:9999",
        |_| true,
        || anyhow::bail!("kickstart exited 5: simulated"),
    );
    assert!(err.is_err(), "kick error must propagate");
    restore();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn kick_platform_is_noop_under_env_override() {
    // B19: ENV_SCHEDULE_HOME set → never call real launchctl /
    // systemctl. The function returns Ok(()) so CI tests are
    // hermetic. We can't directly assert "no subprocess was
    // spawned" — but we assert we return instantly with Ok.
    let tmp = tempdir().unwrap();
    let _g = redirect(tmp.path());
    kick_platform().unwrap();
    restore();
}
