//! Terminal lifecycle + key dispatch for `worklog week`.
//!
//! Keeps the imperative `crossterm` / `ratatui::Terminal` plumbing
//! isolated from the pure state machine. Tests live in
//! [`super::state`] and [`super::render`]; this file is exercised by
//! the CLI smoke test in `tests/`.

use std::io::{self, Stdout};
use std::panic;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rusqlite::Connection;
use worklog_core::{db, paths::Paths, repo, tz};

use super::render::{draw, DrawArgs};
use super::state::{CalendarState, WeekState};

/// Entrypoint for the `worklog week` subcommand.
///
/// `focus` (default: today in `$WORKLOG_TZ`) chooses the initial day.
pub fn run(focus: Option<NaiveDate>) -> Result<()> {
    let paths = Paths::resolve()?;
    let conn = db::open(&paths.db).context("opening worklog db")?;
    let today = local_today();
    let start_day = focus.unwrap_or(today);

    let mut state = load_state(&conn, start_day)?;
    let mut calendar: Option<CalendarState> = None;

    let mut term = setup_terminal().context("entering alt-screen TUI mode")?;
    install_panic_restore();

    let result = event_loop(&mut term, &conn, &mut state, &mut calendar, today);

    restore_terminal(&mut term).ok();
    result
}

fn event_loop(
    term: &mut Terminal<CrosstermBackend<Stdout>>,
    conn: &Connection,
    state: &mut WeekState,
    calendar: &mut Option<CalendarState>,
    today: NaiveDate,
) -> Result<()> {
    loop {
        term.draw(|f| {
            draw(
                f,
                &DrawArgs {
                    state,
                    calendar: calendar.as_ref(),
                    today,
                },
            );
        })
        .context("drawing TUI frame")?;

        let Event::Key(key) = event::read().context("reading terminal event")? else {
            continue;
        };
        // crossterm fires both Press and Release on some terminals; we
        // only act on Press to avoid double-firing nav.
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        if calendar.is_some() {
            match handle_calendar_key(key, calendar, state, conn, today)? {
                LoopOutcome::Continue => {}
                LoopOutcome::Quit => return Ok(()),
            }
        } else {
            match handle_week_key(key, state, calendar, conn, today)? {
                LoopOutcome::Continue => {}
                LoopOutcome::Quit => return Ok(()),
            }
        }
    }
}

enum LoopOutcome {
    Continue,
    Quit,
}

fn handle_week_key(
    key: KeyEvent,
    state: &mut WeekState,
    calendar: &mut Option<CalendarState>,
    conn: &Connection,
    today: NaiveDate,
) -> Result<LoopOutcome> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(LoopOutcome::Quit),
        KeyCode::Char('t') => {
            state.jump_to(today);
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Char('g') => {
            *calendar = Some(CalendarState::new(state.selected_day()));
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if shift || matches!(key.code, KeyCode::Char('H')) {
                state.shift_week(-1);
            } else {
                state.shift_day(-1);
            }
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if shift || matches!(key.code, KeyCode::Char('L')) {
                state.shift_week(1);
            } else {
                state.shift_day(1);
            }
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Char('H') => {
            state.shift_week(-1);
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Char('L') => {
            state.shift_week(1);
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Up | KeyCode::Char('k') => state.move_block_cursor(-1),
        KeyCode::Down | KeyCode::Char('j') => state.move_block_cursor(1),
        _ => {}
    }
    Ok(LoopOutcome::Continue)
}

fn handle_calendar_key(
    key: KeyEvent,
    calendar: &mut Option<CalendarState>,
    state: &mut WeekState,
    conn: &Connection,
    _today: NaiveDate,
) -> Result<LoopOutcome> {
    let Some(cal) = calendar.as_mut() else {
        return Ok(LoopOutcome::Continue);
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            *calendar = None;
        }
        KeyCode::Enter => {
            let target = cal.cursor();
            *calendar = None;
            state.jump_to(target);
            reload_if_needed(state, conn, state.week_start())?;
        }
        KeyCode::Left => cal.move_days(-1),
        KeyCode::Right => cal.move_days(1),
        KeyCode::Up => cal.move_weeks(-1),
        KeyCode::Down => cal.move_weeks(1),
        KeyCode::Char('h') => cal.move_months(-1),
        KeyCode::Char('l') => cal.move_months(1),
        _ => {}
    }
    Ok(LoopOutcome::Continue)
}

fn load_state(conn: &Connection, day: NaiveDate) -> Result<WeekState> {
    let monday = super::state::monday_of(day);
    let blocks = repo::list_blocks_for_week(conn, monday)?;
    Ok(WeekState::new(day, blocks))
}

fn reload_if_needed(state: &mut WeekState, conn: &Connection, week_start: NaiveDate) -> Result<()> {
    // After every navigation we reload — the cost of one indexed SQL
    // query is well below the 16ms frame budget, and it keeps the
    // in-memory window in lockstep with `state.week_start()` without
    // bookkeeping.
    let blocks = repo::list_blocks_for_week(conn, week_start)?;
    state.replace_blocks(blocks);
    Ok(())
}

fn local_today() -> NaiveDate {
    Utc::now().with_timezone(&tz::day_offset()).date_naive()
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

fn restore_terminal(term: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    term.show_cursor()?;
    Ok(())
}

/// If a panic happens inside the event loop, ratatui leaves the
/// terminal in alt-screen + raw mode and the user can't see anything.
/// Wrap the existing hook with a best-effort restore.
fn install_panic_restore() {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        prev(info);
    }));
}

/// `--day YYYY-MM-DD` parser shared with the clap command.
pub fn parse_day(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("invalid --day '{s}', expected YYYY-MM-DD"))
}
