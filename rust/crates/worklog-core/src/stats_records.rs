//! Day-level records and streaks for the stats page (see `stats.rs`).

use chrono::{Duration, NaiveDate};

use crate::stats_contract::*;

/// Day-level records from the zero-filled daily rows (strict `>` keeps the
/// earliest day on ties).
pub(crate) fn fill_records(
    rec: &mut StatsRecords,
    daily: &[DailyStat],
    streak_end: NaiveDate,
    from: NaiveDate,
) {
    let mut run = 0;
    for d in daily {
        if d.work_seconds > 0 {
            run += 1;
            rec.longest_streak = rec.longest_streak.max(run);
            if rec
                .busiest_day
                .as_ref()
                .is_none_or(|b| d.work_seconds > b.seconds)
            {
                rec.busiest_day = Some(DayRecord {
                    day: d.day.clone(),
                    seconds: d.work_seconds,
                });
            }
            if let Some(f) = &d.first_at {
                if rec.earliest_start.as_ref().is_none_or(|e| *f < e.time) {
                    rec.earliest_start = Some(TimeRecord {
                        day: d.day.clone(),
                        time: f.clone(),
                    });
                }
            }
            if let Some(l) = &d.last_at {
                if rec.latest_finish.as_ref().is_none_or(|e| *l > e.time) {
                    rec.latest_finish = Some(TimeRecord {
                        day: d.day.clone(),
                        time: l.clone(),
                    });
                }
            }
        } else {
            run = 0;
        }
        if d.prompts > 0 && rec.most_prompts.as_ref().is_none_or(|m| d.prompts > m.n) {
            rec.most_prompts = Some(CountRecord {
                day: d.day.clone(),
                n: d.prompts,
            });
        }
        if d.tool_calls > 0 && rec.most_tools.as_ref().is_none_or(|m| d.tool_calls > m.n) {
            rec.most_tools = Some(CountRecord {
                day: d.day.clone(),
                n: d.tool_calls,
            });
        }
    }
    let worked = |day: NaiveDate| {
        let i = (day - from).num_days();
        i >= 0 && daily.get(i as usize).is_some_and(|d| d.work_seconds > 0)
    };
    let mut day = streak_end;
    if !worked(day) {
        day -= Duration::days(1);
    }
    while worked(day) {
        rec.current_streak += 1;
        day -= Duration::days(1);
    }
}
