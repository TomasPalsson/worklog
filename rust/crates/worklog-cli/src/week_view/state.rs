//! Pure navigation state for the `worklog week` TUI.
//!
//! Kept terminal-free so the navigation logic can be unit-tested without
//! spinning up ratatui. The renderer (sibling module) reads `WeekState`
//! and the optional `CalendarState` overlay; key handlers translate
//! events into `apply_*` calls here.
//!
//! Invariants:
//! * Weeks are Mon..=Sun. `week_start()` is always a Monday.
//! * `selected_day` is always a date inside `[week_start, week_start+6]`.
//! * `block_cursor` is `None` for empty days; otherwise in
//!   `0..blocks_for_selected_day().len()`.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use worklog_core::models::Block;

/// Snap any date to the Monday of its ISO week.
pub fn monday_of(d: NaiveDate) -> NaiveDate {
    let from_mon = d.weekday().num_days_from_monday() as i64;
    d - Duration::days(from_mon)
}

#[derive(Debug, Clone)]
pub struct WeekState {
    week_start: NaiveDate,
    selected_day: NaiveDate,
    /// All blocks for `[week_start, week_start+6]`, ordered by
    /// `(day, started_at)` — same order `repo::list_blocks_for_week`
    /// returns.
    blocks: Vec<Block>,
    /// Index into `blocks_for_selected_day()` (NOT into `blocks`).
    block_cursor: Option<usize>,
}

impl WeekState {
    pub fn new(focus_day: NaiveDate, blocks: Vec<Block>) -> Self {
        let week_start = monday_of(focus_day);
        let mut s = Self {
            week_start,
            selected_day: focus_day,
            blocks,
            block_cursor: None,
        };
        s.reset_block_cursor();
        s
    }

    pub fn week_start(&self) -> NaiveDate {
        self.week_start
    }

    pub fn week_end(&self) -> NaiveDate {
        self.week_start + Duration::days(6)
    }

    pub fn selected_day(&self) -> NaiveDate {
        self.selected_day
    }

    pub fn block_cursor(&self) -> Option<usize> {
        self.block_cursor
    }

    /// All blocks the renderer should show for the seven columns.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn blocks_for_day(&self, day: NaiveDate) -> impl Iterator<Item = &Block> {
        let iso = day.format("%Y-%m-%d").to_string();
        self.blocks.iter().filter(move |b| b.day == iso)
    }

    pub fn blocks_for_selected_day(&self) -> Vec<&Block> {
        self.blocks_for_day(self.selected_day).collect()
    }

    pub fn selected_block(&self) -> Option<&Block> {
        let idx = self.block_cursor?;
        self.blocks_for_selected_day().into_iter().nth(idx)
    }

    pub fn shift_day(&mut self, delta: i64) {
        self.jump_to(self.selected_day + Duration::days(delta));
    }

    pub fn shift_week(&mut self, delta: i64) {
        self.jump_to(self.selected_day + Duration::days(delta * 7));
    }

    /// Jump to an arbitrary date. Re-snaps `week_start`, clamps
    /// `block_cursor` to the new day's block count. Caller is
    /// responsible for re-loading `blocks` if the new week falls
    /// outside the current window — `needs_reload(new_blocks_start)`
    /// helps callers decide.
    pub fn jump_to(&mut self, day: NaiveDate) {
        self.selected_day = day;
        self.week_start = monday_of(day);
        self.reset_block_cursor();
    }

    /// Replace the in-memory block window. Use after a `jump_to`/
    /// `shift_week` that crossed week boundaries.
    pub fn replace_blocks(&mut self, blocks: Vec<Block>) {
        self.blocks = blocks;
        self.reset_block_cursor();
    }

    /// True iff the currently-loaded `blocks` window doesn't cover the
    /// current `week_start..=week_end`. Cheap O(1) check on the cached
    /// `week_start` — we don't actually look at `blocks`, the assumption
    /// is the caller passes the same Monday it used to load.
    pub fn needs_reload(&self, loaded_week_start: NaiveDate) -> bool {
        loaded_week_start != self.week_start
    }

    pub fn move_block_cursor(&mut self, delta: i64) {
        let day_blocks = self.blocks_for_selected_day();
        if day_blocks.is_empty() {
            self.block_cursor = None;
            return;
        }
        let len = day_blocks.len() as i64;
        let cur = self.block_cursor.unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, len - 1);
        self.block_cursor = Some(next as usize);
    }

    fn reset_block_cursor(&mut self) {
        let n = self.blocks_for_selected_day().len();
        self.block_cursor = if n == 0 { None } else { Some(0) };
    }

    /// Total seconds of work in the loaded week (excluding personal).
    pub fn week_total_seconds(&self) -> i64 {
        self.blocks
            .iter()
            .filter(|b| !b.is_personal)
            .map(|b| b.duration_seconds)
            .sum()
    }
}

/// Mini-calendar popup state. Shows three months stacked: previous,
/// current, next. Movement is by day/week/month.
#[derive(Debug, Clone)]
pub struct CalendarState {
    cursor: NaiveDate,
}

impl CalendarState {
    pub fn new(focus: NaiveDate) -> Self {
        Self { cursor: focus }
    }

    pub fn cursor(&self) -> NaiveDate {
        self.cursor
    }

    pub fn move_days(&mut self, delta: i64) {
        self.cursor += Duration::days(delta);
    }

    pub fn move_weeks(&mut self, delta: i64) {
        self.cursor += Duration::days(delta * 7);
    }

    /// Move by whole calendar months, clamping the day-of-month when the
    /// target month is shorter (e.g. Jan 31 → Feb 28/29).
    pub fn move_months(&mut self, delta: i32) {
        let mut year = self.cursor.year();
        let mut month0 = self.cursor.month0() as i32 + delta;
        while month0 < 0 {
            year -= 1;
            month0 += 12;
        }
        while month0 >= 12 {
            year += 1;
            month0 -= 12;
        }
        let month = month0 as u32 + 1;
        let day = self.cursor.day();
        let target = NaiveDate::from_ymd_opt(year, month, day)
            .or_else(|| NaiveDate::from_ymd_opt(year, month, last_day_of_month(year, month)))
            .expect("valid clamped date");
        self.cursor = target;
    }

    /// Returns the three months to render (prev, current, next), each
    /// represented by its first-of-month date.
    pub fn visible_months(&self) -> [NaiveDate; 3] {
        let cur = NaiveDate::from_ymd_opt(self.cursor.year(), self.cursor.month(), 1).unwrap();
        let prev = subtract_one_month(cur);
        let next = add_one_month(cur);
        [prev, cur, next]
    }

    /// `Weekday::Mon` based weekday index 0..=6.
    pub fn weekday_index(d: NaiveDate) -> usize {
        d.weekday().num_days_from_monday() as usize
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    (next - Duration::days(1)).day()
}

fn add_one_month(first_of_month: NaiveDate) -> NaiveDate {
    let (y, m) = if first_of_month.month() == 12 {
        (first_of_month.year() + 1, 1)
    } else {
        (first_of_month.year(), first_of_month.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

fn subtract_one_month(first_of_month: NaiveDate) -> NaiveDate {
    let (y, m) = if first_of_month.month() == 1 {
        (first_of_month.year() - 1, 12)
    } else {
        (first_of_month.year(), first_of_month.month() - 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

/// A single user-visible weekday header (date + label). Helper for the
/// renderer; lives here because the day list is a property of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayHeader {
    pub date: NaiveDate,
    pub weekday: Weekday,
}

impl WeekState {
    pub fn day_headers(&self) -> [DayHeader; 7] {
        std::array::from_fn(|i| {
            let date = self.week_start + Duration::days(i as i64);
            DayHeader {
                date,
                weekday: date.weekday(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(day: &str, start: &str, end: &str, dur: i64, personal: bool) -> Block {
        Block {
            id: 0,
            day: day.into(),
            jira_issue: None,
            started_at: start.into(),
            ended_at: end.into(),
            duration_seconds: dur,
            description: None,
            estimated_by: None,
            flagged: false,
            tempo_worklog_id: None,
            is_personal: personal,
            dirty: false,
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn monday_of_snaps_any_weekday_to_monday() {
        // Wed 2026-04-15 → Mon 2026-04-13
        assert_eq!(monday_of(date(2026, 4, 15)), date(2026, 4, 13));
        // Sun 2026-04-19 → Mon 2026-04-13
        assert_eq!(monday_of(date(2026, 4, 19)), date(2026, 4, 13));
        // Mon 2026-04-13 → itself
        assert_eq!(monday_of(date(2026, 4, 13)), date(2026, 4, 13));
    }

    #[test]
    fn week_state_initialises_around_focus_day() {
        let s = WeekState::new(date(2026, 4, 15), vec![]);
        assert_eq!(s.week_start(), date(2026, 4, 13));
        assert_eq!(s.week_end(), date(2026, 4, 19));
        assert_eq!(s.selected_day(), date(2026, 4, 15));
        assert!(s.block_cursor().is_none()); // empty day
    }

    #[test]
    fn shift_day_within_week_keeps_week_start() {
        let mut s = WeekState::new(date(2026, 4, 15), vec![]);
        s.shift_day(1);
        assert_eq!(s.selected_day(), date(2026, 4, 16));
        assert_eq!(s.week_start(), date(2026, 4, 13));
        assert!(!s.needs_reload(date(2026, 4, 13)));
    }

    #[test]
    fn shift_day_across_week_boundary_advances_week_start() {
        let mut s = WeekState::new(date(2026, 4, 19), vec![]); // Sun
        s.shift_day(1); // → Mon next week
        assert_eq!(s.selected_day(), date(2026, 4, 20));
        assert_eq!(s.week_start(), date(2026, 4, 20));
        assert!(s.needs_reload(date(2026, 4, 13)));
    }

    #[test]
    fn shift_week_jumps_seven_days_and_resnaps() {
        let mut s = WeekState::new(date(2026, 4, 15), vec![]); // Wed
        s.shift_week(-1);
        assert_eq!(s.selected_day(), date(2026, 4, 8));
        assert_eq!(s.week_start(), date(2026, 4, 6));
    }

    #[test]
    fn jump_to_arbitrary_date_resets_cursor() {
        let blocks = vec![
            block("2026-04-15", "10:00", "10:30", 1800, false),
            block("2026-04-15", "11:00", "11:30", 1800, false),
        ];
        let mut s = WeekState::new(date(2026, 4, 15), blocks);
        s.move_block_cursor(1); // cursor=1
        assert_eq!(s.block_cursor(), Some(1));
        s.jump_to(date(2026, 6, 1));
        assert_eq!(s.week_start(), date(2026, 6, 1)); // a Monday
                                                      // After jump, the loaded blocks no longer match the selected
                                                      // day, so cursor should be None.
        assert_eq!(s.block_cursor(), None);
    }

    #[test]
    fn block_cursor_clamps_when_day_has_few_blocks() {
        let blocks = vec![block("2026-04-15", "10:00", "10:30", 1800, false)];
        let mut s = WeekState::new(date(2026, 4, 15), blocks);
        s.move_block_cursor(10);
        assert_eq!(s.block_cursor(), Some(0)); // clamped to 0
        s.move_block_cursor(-10);
        assert_eq!(s.block_cursor(), Some(0));
    }

    #[test]
    fn week_total_excludes_personal_blocks() {
        let blocks = vec![
            block("2026-04-15", "10:00", "10:30", 1800, false), // 30m work
            block("2026-04-16", "10:00", "11:00", 3600, true),  // 60m personal
            block("2026-04-17", "10:00", "11:00", 3600, false), // 60m work
        ];
        let s = WeekState::new(date(2026, 4, 15), blocks);
        assert_eq!(s.week_total_seconds(), 1800 + 3600);
    }

    #[test]
    fn day_headers_span_mon_to_sun() {
        let s = WeekState::new(date(2026, 4, 15), vec![]);
        let h = s.day_headers();
        assert_eq!(h[0].date, date(2026, 4, 13));
        assert_eq!(h[0].weekday, Weekday::Mon);
        assert_eq!(h[6].date, date(2026, 4, 19));
        assert_eq!(h[6].weekday, Weekday::Sun);
    }

    #[test]
    fn calendar_move_days_and_weeks() {
        let mut c = CalendarState::new(date(2026, 4, 15));
        c.move_days(2);
        assert_eq!(c.cursor(), date(2026, 4, 17));
        c.move_weeks(-1);
        assert_eq!(c.cursor(), date(2026, 4, 10));
    }

    #[test]
    fn calendar_move_months_clamps_to_short_month() {
        let mut c = CalendarState::new(date(2026, 1, 31));
        c.move_months(1);
        // Feb 2026 has 28 days.
        assert_eq!(c.cursor(), date(2026, 2, 28));
        c.move_months(-1);
        assert_eq!(c.cursor(), date(2026, 1, 28));
    }

    #[test]
    fn calendar_visible_months_returns_prev_cur_next() {
        let c = CalendarState::new(date(2026, 4, 15));
        let m = c.visible_months();
        assert_eq!(m[0], date(2026, 3, 1));
        assert_eq!(m[1], date(2026, 4, 1));
        assert_eq!(m[2], date(2026, 5, 1));
    }

    #[test]
    fn calendar_visible_months_wraps_year_boundary() {
        let c = CalendarState::new(date(2026, 1, 15));
        let m = c.visible_months();
        assert_eq!(m[0], date(2025, 12, 1));
        assert_eq!(m[1], date(2026, 1, 1));
        assert_eq!(m[2], date(2026, 2, 1));
    }
}
