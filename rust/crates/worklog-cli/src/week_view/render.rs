//! Ratatui drawing for the week view.
//!
//! Pure function over [`super::state::WeekState`] (+ optional
//! [`super::state::CalendarState`] overlay). No terminal lifecycle here
//! — that lives in `super::run`.

use chrono::{Datelike, NaiveDate, Weekday};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block as TuiBlock, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use worklog_core::models::Block;

use super::state::{CalendarState, DayHeader, WeekState};

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub struct DrawArgs<'a> {
    pub state: &'a WeekState,
    pub calendar: Option<&'a CalendarState>,
    pub today: NaiveDate,
}

pub fn draw(f: &mut Frame, args: &DrawArgs) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(1),    // week grid
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_title_bar(f, chunks[0], args);
    draw_week_grid(f, chunks[1], args);
    draw_status_bar(f, chunks[2], args);

    if let Some(cal) = args.calendar {
        draw_calendar_popup(f, area, cal, args.today);
    }
}

fn draw_title_bar(f: &mut Frame, area: Rect, args: &DrawArgs) {
    let s = args.state;
    let total = format_hms(s.week_total_seconds());
    let title = format!(
        " worklog · week of {}–{} · total {} ",
        s.week_start().format("%Y-%m-%d"),
        s.week_end().format("%Y-%m-%d"),
        total,
    );
    let p = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Left);
    f.render_widget(p, area);
}

fn draw_week_grid(f: &mut Frame, area: Rect, args: &DrawArgs) {
    let constraints: Vec<Constraint> = (0..7).map(|_| Constraint::Percentage(100 / 7)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (i, header) in args.state.day_headers().iter().enumerate() {
        draw_day_column(f, cols[i], args, header);
    }
}

fn draw_day_column(f: &mut Frame, area: Rect, args: &DrawArgs, header: &DayHeader) {
    let is_selected = header.date == args.state.selected_day();
    let is_today = header.date == args.today;
    let title = format!(
        " {} {} ",
        DAY_NAMES[header.weekday.num_days_from_monday() as usize],
        header.date.format("%m-%d"),
    );
    let title_style = if is_today {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if is_selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let border_style = if is_selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = TuiBlock::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, title_style));

    let day_blocks: Vec<&Block> = args.state.blocks_for_day(header.date).collect();
    let cursor = if is_selected {
        args.state.block_cursor()
    } else {
        None
    };
    let lines: Vec<Line> = if day_blocks.is_empty() {
        vec![Line::from(Span::styled(
            "  (no blocks)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        day_blocks
            .iter()
            .enumerate()
            .map(|(i, b)| block_line(b, Some(i) == cursor))
            .collect()
    };
    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn block_line(b: &Block, selected: bool) -> Line<'_> {
    let mut style = if b.is_personal {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    if selected {
        style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
    }
    let badge = if b.flagged {
        "● "
    } else if b.tempo_worklog_id.as_deref().is_some_and(|s| !s.is_empty()) {
        "✓ "
    } else if b.dirty {
        "~ "
    } else {
        "· "
    };
    let badge_style = if b.flagged {
        Style::default().fg(Color::Red)
    } else if b.tempo_worklog_id.as_deref().is_some_and(|s| !s.is_empty()) {
        Style::default().fg(Color::Green)
    } else if b.dirty {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let time = format!("{}–{}", clock(&b.started_at), clock(&b.ended_at));
    let ticket = b.jira_issue.clone().unwrap_or_else(|| "—".into());
    let desc = b.description.clone().unwrap_or_default();
    let dur = format_duration(b.duration_seconds);

    Line::from(vec![
        Span::styled(badge, badge_style),
        Span::styled(format!("{time}  "), style),
        Span::styled(format!("{ticket}  "), style.add_modifier(Modifier::BOLD)),
        Span::styled(format!("({dur})  "), Style::default().fg(Color::Magenta)),
        Span::styled(desc, style),
    ])
}

fn draw_status_bar(f: &mut Frame, area: Rect, args: &DrawArgs) {
    let in_calendar = args.calendar.is_some();
    let hint = if in_calendar {
        "  ←/→ day · ↑/↓ week · h/l month · Enter jump · Esc cancel"
    } else {
        "  ←/→ day · Shift+←/→ week · ↑/↓ block · t today · g calendar · q quit"
    };
    let p = Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::Black).bg(Color::Gray),
    )));
    f.render_widget(p, area);
}

fn draw_calendar_popup(f: &mut Frame, area: Rect, cal: &CalendarState, today: NaiveDate) {
    // 3 stacked months, each ~9 rows × ~22 cols. Center horizontally.
    let popup_w = 26.min(area.width.saturating_sub(2));
    let popup_h = 32.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let rect = Rect::new(x, y, popup_w, popup_h);
    f.render_widget(Clear, rect);
    let frame = TuiBlock::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " jump to date ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = frame.inner(rect);
    f.render_widget(frame, rect);

    let months = cal.visible_months();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ])
        .split(inner);
    for (i, first) in months.iter().enumerate() {
        draw_month(f, chunks[i], *first, cal.cursor(), today);
    }
}

fn draw_month(f: &mut Frame, area: Rect, first: NaiveDate, cursor: NaiveDate, today: NaiveDate) {
    let mut lines: Vec<Line> = Vec::with_capacity(8);
    lines.push(Line::from(Span::styled(
        first.format("%B %Y").to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Mo Tu We Th Fr Sa Su",
        Style::default().fg(Color::DarkGray),
    )));
    let lead = first.weekday().num_days_from_monday() as usize;
    let last_day = last_day_of_month_local(first);

    let mut spans: Vec<Span> = (0..lead).map(|_| Span::raw("   ")).collect();
    for day in 1..=last_day {
        let date = NaiveDate::from_ymd_opt(first.year(), first.month(), day).unwrap();
        let mut style = Style::default().fg(Color::White);
        if date == today {
            style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
        }
        if date == cursor {
            style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(format!("{day:>2} "), style));
        if (lead + day as usize) % 7 == 0 {
            lines.push(Line::from(std::mem::take(&mut spans)));
        }
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    let p = Paragraph::new(lines);
    f.render_widget(p, area);
}

// ───────── helpers ─────────

fn clock(iso: &str) -> String {
    // ISO-8601 like "2026-04-15T10:00:00Z" or "10:00" — keep last 5 chars
    // before any 'Z'/timezone, which gives "HH:MM" for both shapes.
    let trimmed = iso.trim_end_matches('Z');
    if let Some(t) = trimmed.split('T').nth(1) {
        t.chars().take(5).collect()
    } else if trimmed.len() >= 5 {
        trimmed.chars().take(5).collect()
    } else {
        trimmed.to_string()
    }
}

fn format_duration(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

fn format_hms(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h {m:02}m")
}

fn last_day_of_month_local(first: NaiveDate) -> u32 {
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1).unwrap()
    };
    next.pred_opt().unwrap().day()
}

#[allow(dead_code)]
pub(crate) fn weekday_short(w: Weekday) -> &'static str {
    DAY_NAMES[w.num_days_from_monday() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn block(day: &str, start: &str, end: &str, ticket: Option<&str>, desc: &str) -> Block {
        Block {
            id: 0,
            day: day.into(),
            jira_issue: ticket.map(String::from),
            started_at: start.into(),
            ended_at: end.into(),
            duration_seconds: 1800,
            description: Some(desc.into()),
            estimated_by: None,
            flagged: false,
            tempo_worklog_id: None,
            is_personal: false,
            dirty: false,
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn frame_to_string(t: &Terminal<TestBackend>) -> String {
        let buf = t.backend().buffer();
        let w = buf.area.width as usize;
        let mut out = String::new();
        for (i, cell) in buf.content.iter().enumerate() {
            out.push_str(cell.symbol());
            if (i + 1) % w == 0 {
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn renders_week_header_and_block_text() {
        let blocks = vec![block(
            "2026-04-15",
            "2026-04-15T10:00:00Z",
            "2026-04-15T10:30:00Z",
            Some("PROJ-9"),
            "ship the thing",
        )];
        let state = WeekState::new(d(2026, 4, 15), blocks);
        let backend = TestBackend::new(140, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            draw(
                f,
                &DrawArgs {
                    state: &state,
                    calendar: None,
                    today: d(2026, 4, 15),
                },
            );
        })
        .unwrap();
        let s = frame_to_string(&term);
        assert!(s.contains("week of 2026-04-13"), "title bar: {s}");
        assert!(s.contains("Wed 04-15"), "selected day header: {s}");
        assert!(s.contains("PROJ-9"), "block ticket: {s}");
        assert!(s.contains("10:00–10:30"), "block time: {s}");
        assert!(s.contains("g calendar"), "status bar hint: {s}");
    }

    #[test]
    fn renders_no_blocks_placeholder() {
        let state = WeekState::new(d(2026, 4, 15), vec![]);
        let backend = TestBackend::new(140, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            draw(
                f,
                &DrawArgs {
                    state: &state,
                    calendar: None,
                    today: d(2026, 4, 15),
                },
            );
        })
        .unwrap();
        let s = frame_to_string(&term);
        assert!(s.contains("(no blocks)"));
    }

    #[test]
    fn calendar_popup_shows_three_months_and_today() {
        let state = WeekState::new(d(2026, 4, 15), vec![]);
        let cal = CalendarState::new(d(2026, 4, 15));
        let backend = TestBackend::new(140, 40);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            draw(
                f,
                &DrawArgs {
                    state: &state,
                    calendar: Some(&cal),
                    today: d(2026, 4, 15),
                },
            );
        })
        .unwrap();
        let s = frame_to_string(&term);
        assert!(s.contains("jump to date"));
        assert!(s.contains("April 2026"));
        assert!(s.contains("March 2026"));
        assert!(s.contains("May 2026"));
    }

    #[test]
    fn clock_extracts_hh_mm() {
        assert_eq!(clock("2026-04-15T10:30:00Z"), "10:30");
        assert_eq!(clock("2026-04-15T10:30:00"), "10:30");
        assert_eq!(clock("10:30"), "10:30");
    }

    #[test]
    fn format_duration_human() {
        assert_eq!(format_duration(1800), "30m");
        assert_eq!(format_duration(3600), "1h00m");
        assert_eq!(format_duration(3660), "1h01m");
    }
}
