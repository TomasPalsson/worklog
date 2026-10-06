use super::*;
use std::collections::VecDeque;
use std::sync::Arc as Rc;
use std::sync::{Mutex, MutexGuard};

// `Proc` is `Send`, so the fake world is shared through mutexes.
#[derive(Default)]
struct Cell<T>(Mutex<T>);

impl<T: Copy> Cell<T> {
    fn get(&self) -> T {
        *self.0.lock().unwrap()
    }
    fn set(&self, v: T) {
        *self.0.lock().unwrap() = v;
    }
}

#[derive(Default)]
struct RefCell<T>(Mutex<T>);

impl<T> RefCell<T> {
    fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

#[derive(Default)]
struct World {
    now: Cell<u64>,
    launches: Cell<u32>,
    answering: Cell<bool>,
    killed: Cell<u32>,
    /// Next launch outcomes; empty means success.
    outcomes: RefCell<VecDeque<Result<(), LaunchError>>>,
    /// Set by a test to make the live process report an exit.
    exit: RefCell<Option<String>>,
}

struct FakeProc(Rc<World>);

impl Proc for FakeProc {
    fn exited(&mut self) -> Option<String> {
        self.0.exit.borrow_mut().take()
    }
    fn kill(&mut self) {
        self.0.killed.set(self.0.killed.get() + 1);
    }
}

struct FakeHost(Rc<World>);

impl Host for FakeHost {
    fn now(&self) -> u64 {
        self.0.now.get()
    }
    fn launch(&mut self) -> Result<Box<dyn Proc>, LaunchError> {
        match self.0.outcomes.borrow_mut().pop_front().unwrap_or(Ok(())) {
            Ok(()) => {
                self.0.launches.set(self.0.launches.get() + 1);
                Ok(Box::new(FakeProc(self.0.clone())))
            }
            Err(e) => Err(e),
        }
    }
    fn answering(&self) -> bool {
        self.0.answering.get()
    }
}

fn sup(enabled: bool) -> (Supervisor<FakeHost>, Rc<World>) {
    let w = Rc::new(World::default());
    let mut s = Supervisor::new(FakeHost(w.clone()));
    s.set_enabled(enabled);
    (s, w)
}

fn crash(s: &mut Supervisor<FakeHost>, w: &World, at: u64, msg: &str) {
    w.now.set(at);
    *w.exit.borrow_mut() = Some(msg.into());
    s.tick();
}

#[test]
fn off_by_default_and_never_launches() {
    let (s, w) = sup(false);
    assert_eq!(s.state(), VerdictState::Off);
    assert_eq!(w.launches.get(), 0);
}

#[test]
fn start_grace_is_under_90_seconds_then_not_answering() {
    let (s, w) = sup(true);
    assert_eq!(s.state(), VerdictState::Starting);
    // 89 s: still starting; exactly 90 s: not answering (catches > vs >=).
    w.now.set(89);
    assert_eq!(s.state(), VerdictState::Starting);
    w.now.set(90);
    assert_eq!(s.state(), VerdictState::NotAnswering);
}

#[test]
fn answering_means_running_even_inside_the_grace_period() {
    let (s, w) = sup(true);
    w.answering.set(true);
    assert_eq!(s.state(), VerdictState::Running);
    // Catches a Running that is only ever reported after the grace period.
    w.now.set(500);
    assert_eq!(s.state(), VerdictState::Running);
}

#[test]
fn missing_uv_shows_needs_uv_and_starts_nothing() {
    let w = Rc::new(World::default());
    w.outcomes.borrow_mut().push_back(Err(LaunchError::NeedsUv));
    let mut s = Supervisor::new(FakeHost(w.clone()));
    s.set_enabled(true);
    assert_eq!(s.state(), VerdictState::NeedsUv);
    assert_eq!(w.launches.get(), 0);
    // Switching on again once uv exists launches (catches a latched failure).
    s.set_enabled(true);
    assert_eq!(w.launches.get(), 1);
    assert_eq!(s.state(), VerdictState::Starting);
}

#[test]
fn launch_failure_is_stopped_with_its_message() {
    let w = Rc::new(World::default());
    w.outcomes
        .borrow_mut()
        .push_back(Err(LaunchError::Failed("no such file".into())));
    let mut s = Supervisor::new(FakeHost(w));
    s.set_enabled(true);
    assert_eq!(
        s.state(),
        VerdictState::Stopped {
            error: "no such file".into()
        }
    );
}

#[test]
fn three_crashes_restart_and_the_fourth_stops_with_the_last_error() {
    let (mut s, w) = sup(true);
    for (i, t) in [10u64, 20, 30].into_iter().enumerate() {
        crash(&mut s, &w, t, "boom");
        assert_eq!(
            w.launches.get(),
            2 + i as u32,
            "restart after crash {}",
            i + 1
        );
    }
    crash(&mut s, &w, 40, "fourth");
    // No fifth launch (catches a limit of 4 restarts).
    assert_eq!(w.launches.get(), 4);
    assert_eq!(
        s.state(),
        VerdictState::Stopped {
            error: "fourth".into()
        }
    );
    // A stopped supervisor stays stopped on later ticks.
    w.now.set(1000);
    s.tick();
    assert_eq!(w.launches.get(), 4);
}

#[test]
fn a_crash_just_inside_the_window_of_three_restarts_stops() {
    let (mut s, w) = sup(true);
    for t in [0u64, 100, 200] {
        crash(&mut s, &w, t, "x");
    }
    // Restarts were recorded at 0, 100, 200; 599 s after the first is inside 600.
    crash(&mut s, &w, 599, "late");
    assert!(matches!(s.state(), VerdictState::Stopped { .. }));
}

#[test]
fn a_crash_exactly_at_the_window_edge_restarts_again() {
    let (mut s, w) = sup(true);
    for t in [0u64, 100, 200] {
        crash(&mut s, &w, t, "x");
    }
    // 600 s: the restart at 0 is exactly one window old and has aged out
    // (catches `<=` where `<` is meant).
    crash(&mut s, &w, 600, "edge");
    assert_eq!(w.launches.get(), 5);
    assert!(!matches!(s.state(), VerdictState::Stopped { .. }));
}

#[test]
fn a_crash_just_past_the_window_restarts_again() {
    let (mut s, w) = sup(true);
    for t in [0u64, 100, 200] {
        crash(&mut s, &w, t, "x");
    }
    // 601 s: the restart at 0 has aged out, so this is a fresh restart
    // (catches a lifetime counter that never forgets).
    crash(&mut s, &w, 601, "later");
    assert_eq!(w.launches.get(), 5);
    assert!(!matches!(s.state(), VerdictState::Stopped { .. }));
}

#[test]
fn healthy_ticks_do_not_relaunch() {
    let (mut s, w) = sup(true);
    w.now.set(60);
    s.tick();
    s.tick();
    assert_eq!(w.launches.get(), 1);
}

#[test]
fn switching_off_kills_the_process_and_a_late_exit_is_ignored() {
    let (mut s, w) = sup(true);
    s.set_enabled(false);
    assert_eq!(w.killed.get(), 1);
    assert_eq!(s.state(), VerdictState::Off);
    *w.exit.borrow_mut() = Some("late".into());
    s.tick();
    assert_eq!(w.launches.get(), 1, "an off supervisor must not restart");
}

#[test]
fn switching_on_twice_does_not_start_a_second_process() {
    let (mut s, w) = sup(true);
    s.set_enabled(true);
    assert_eq!(w.launches.get(), 1);
}

#[test]
fn retry_after_stop_relaunches_with_a_fresh_budget() {
    let (mut s, w) = sup(true);
    for t in [1u64, 2, 3, 4] {
        crash(&mut s, &w, t, "x");
    }
    assert!(matches!(s.state(), VerdictState::Stopped { .. }));
    s.retry();
    assert_eq!(w.launches.get(), 5);
    assert_eq!(s.state(), VerdictState::Starting);
    // The budget restarted: one more crash restarts instead of stopping.
    crash(&mut s, &w, 5, "y");
    assert_eq!(w.launches.get(), 6);
}

#[test]
fn retry_when_not_stopped_is_a_no_op() {
    let (mut s, w) = sup(true);
    s.retry();
    assert_eq!(w.launches.get(), 1);
    let (mut off, w) = sup(false);
    off.retry();
    assert_eq!(w.launches.get(), 0);
    assert_eq!(off.state(), VerdictState::Off);
}

#[test]
fn enabled_key_parses_exactly_and_defaults_on() {
    assert!(parse_enabled(None));
    assert!(parse_enabled(Some("on")));
    assert!(!parse_enabled(Some("off")));
    // Anything else is the default, not "off" and not a panic.
    assert!(parse_enabled(Some("OFF")));
    assert!(parse_enabled(Some("")));
    assert!(parse_enabled(Some("false")));
}

#[test]
fn uv_is_looked_for_in_candidates_before_path() {
    let home = Path::new("/home/me");
    let path = Some(std::ffi::OsString::from("/pathdir:/other"));
    let all = |p: &Path| {
        [
            "/home/me/.local/bin/uv",
            "/opt/homebrew/bin/uv",
            "/pathdir/uv",
        ]
        .contains(&p.to_str().unwrap())
    };
    assert_eq!(
        find_uv_in(home, path.clone(), all),
        Some(PathBuf::from("/home/me/.local/bin/uv"))
    );
    // Only the second candidate and PATH exist: the candidate still wins.
    let second = |p: &Path| ["/opt/homebrew/bin/uv", "/pathdir/uv"].contains(&p.to_str().unwrap());
    assert_eq!(
        find_uv_in(home, path.clone(), second),
        Some(PathBuf::from("/opt/homebrew/bin/uv"))
    );
    // PATH is the fallback, and its second entry is searched.
    let other = |p: &Path| p == Path::new("/other/uv");
    assert_eq!(
        find_uv_in(home, path.clone(), other),
        Some(PathBuf::from("/other/uv"))
    );
    assert_eq!(find_uv_in(home, path, |_| false), None);
    assert_eq!(find_uv_in(home, None, |_| false), None);
}

#[test]
fn uv_args_pin_the_verdict_revision() {
    let args = uv_args(Path::new("/tmp/verdict_server.py"));
    assert_eq!(
        args,
        vec![
            "run".to_string(),
            "--with".to_string(),
            "git+https://github.com/Heman10x-NGU/Verdict-open-jev@30f15564821626ca5c1ad5b2638c4eb7078787dd"
                .to_string(),
            "--with".to_string(),
            "huggingface_hub".to_string(),
            "python".to_string(),
            "/tmp/verdict_server.py".to_string(),
        ]
    );
}

#[test]
fn last_log_line_skips_trailing_blank_lines() {
    assert_eq!(last_line("a\nlast error\n\n  \n"), "last error");
    assert_eq!(last_line(""), "");
}

#[test]
fn real_proc_kill_takes_down_grandchildren() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new("sh")
        .args(["-c", "sleep 300 & echo $!; wait"])
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut line,
    )
    .unwrap();
    let grandchild = line.trim().to_string();
    let mut p = RealProc {
        child,
        log: "/nonexistent".into(),
    };
    p.kill();
    let alive = Command::new("kill")
        .args(["-0", &grandchild])
        .status()
        .unwrap()
        .success();
    assert!(!alive, "grandchild {grandchild} survived kill");
}
