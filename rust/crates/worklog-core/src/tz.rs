//! Day-bucket timezone.
//!
//! Events are stored in the DB as UTC (ISO-8601). The block's `day`
//! field and the `load_day_events` window are, by default, also UTC.
//! That's fine for users in UTC±0 but means a developer in UTC-5 sees
//! late-night work land on tomorrow's page.
//!
//! Set `$WORKLOG_TZ` to a fixed offset (e.g. `+01:00`, `-05:00`, `UTC`)
//! to bucket days by that offset instead. Named zones like
//! `America/New_York` are NOT supported — that would require
//! `chrono-tz` and a rules database; fixed offsets cover the common
//! case (users who live in one zone year-round) without the dep.
//!
//! DST-observers can update the env var when DST flips; the cost of
//! the simpler approach is one setting change per year.

use chrono::{FixedOffset, NaiveDate, Utc};

/// Read `$WORKLOG_TZ` as a fixed offset. Empty / missing / invalid
/// values fall back to UTC. Accepts:
///   * `UTC`, `utc`, empty → UTC (+00:00)
///   * `+HH:MM` or `-HH:MM` → the given offset
///   * `+HH`, `-HH` → hour-only shorthand
///
/// An unparseable value (e.g. `America/New_York`, `+99:99`) logs a
/// `tracing::warn!` with a hint pointing at the supported format, then
/// falls back to UTC. Silent fallback would leave a user who typo'd
/// their timezone wondering why late-night events keep landing on
/// tomorrow's page.
pub fn day_offset() -> FixedOffset {
    let utc = FixedOffset::east_opt(0).unwrap();
    match std::env::var("WORKLOG_TZ").ok().as_deref().map(str::trim) {
        None | Some("") => utc,
        Some(s) if s.eq_ignore_ascii_case("utc") || s == "Z" => utc,
        Some(s) => parse_offset(s).unwrap_or_else(|| {
            tracing::warn!(
                "WORKLOG_TZ={s:?} is not a recognised fixed offset \
                 (use +HH:MM or -HH:MM — named zones like America/New_York \
                 are not supported). Falling back to UTC."
            );
            utc
        }),
    }
}

fn parse_offset(s: &str) -> Option<FixedOffset> {
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    let (hours, minutes) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None => (rest.parse::<i32>().ok()?, 0),
    };
    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

/// Convert a UTC instant to the local (TZ-offset) date the user would
/// call it. Used by `infer::new_block` so blocks land on the local day.
pub fn local_date(ts: chrono::DateTime<Utc>) -> NaiveDate {
    ts.with_timezone(&day_offset()).date_naive()
}

/// UTC window `[start, end)` covering the given local day at the current
/// `$WORKLOG_TZ`. Used by `infer::load_day_events` so the SQL range
/// matches the user's local day.
pub fn utc_window_for_local_day(day: NaiveDate) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    use chrono::NaiveTime;
    let off = day_offset();
    let start_local = day
        .and_time(NaiveTime::MIN)
        .and_local_timezone(off)
        .single()
        .unwrap_or_else(|| day.and_time(NaiveTime::MIN).and_utc().with_timezone(&off));
    let end_local = (day + chrono::Duration::days(1))
        .and_time(NaiveTime::MIN)
        .and_local_timezone(off)
        .single()
        .unwrap_or_else(|| (day + chrono::Duration::days(1)).and_time(NaiveTime::MIN).and_utc().with_timezone(&off));
    (start_local.with_timezone(&Utc), end_local.with_timezone(&Utc))
}

#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn default_is_utc() {
        let _g = test_env_lock();
        std::env::remove_var("WORKLOG_TZ");
        assert_eq!(day_offset().local_minus_utc(), 0);
    }

    #[test]
    fn env_utc_literal_parses_to_zero() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "UTC");
        assert_eq!(day_offset().local_minus_utc(), 0);
        std::env::set_var("WORKLOG_TZ", "Z");
        assert_eq!(day_offset().local_minus_utc(), 0);
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn positive_offset_parses() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "+02:00");
        assert_eq!(day_offset().local_minus_utc(), 2 * 3600);
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn negative_offset_parses() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:30");
        assert_eq!(
            day_offset().local_minus_utc(),
            -(5 * 3600 + 30 * 60)
        );
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn hour_only_shorthand_parses() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "+05");
        assert_eq!(day_offset().local_minus_utc(), 5 * 3600);
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn garbage_falls_back_to_utc() {
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "America/New_York");
        assert_eq!(day_offset().local_minus_utc(), 0);
        std::env::set_var("WORKLOG_TZ", "+99:99");
        assert_eq!(day_offset().local_minus_utc(), 0);
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn local_date_shifts_across_boundary_in_negative_tz() {
        // 02:00 UTC on Apr 19 is 21:00 local on Apr 18 in UTC-5.
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "-05:00");
        let ts = Utc.with_ymd_and_hms(2026, 4, 19, 2, 0, 0).unwrap();
        assert_eq!(local_date(ts), NaiveDate::from_ymd_opt(2026, 4, 18).unwrap());
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn utc_window_matches_local_midnight_in_positive_tz() {
        // Local Apr 18 in +02:00 = UTC 2026-04-17T22:00 → 2026-04-18T22:00
        let _g = test_env_lock();
        std::env::set_var("WORKLOG_TZ", "+02:00");
        let (start, end) = utc_window_for_local_day(NaiveDate::from_ymd_opt(2026, 4, 18).unwrap());
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 4, 17, 22, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 4, 18, 22, 0, 0).unwrap());
        std::env::remove_var("WORKLOG_TZ");
    }

    #[test]
    fn utc_window_default_is_utc_day() {
        let _g = test_env_lock();
        std::env::remove_var("WORKLOG_TZ");
        let (start, end) = utc_window_for_local_day(NaiveDate::from_ymd_opt(2026, 4, 18).unwrap());
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 4, 18, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 4, 19, 0, 0, 0).unwrap());
    }
}
