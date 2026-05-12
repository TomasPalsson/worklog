//! `worklog week` — interactive console week view + calendar picker.
//!
//! Layered as:
//!   * [`state`] — pure navigation logic (WeekState, CalendarState).
//!     Terminal-free so it's covered by ordinary unit tests.
//!   * `render` (added in phase 3) — ratatui drawing on top of `state`.
//!   * `run` (added in phase 4) — terminal lifecycle + key dispatch.

pub mod render;
pub mod state;
