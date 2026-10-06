//! The one place tracked seconds become billable half-hours.

pub use crate::tempo_line_contract::HALF_HOUR_SECONDS;

/// Round up to the next half hour; under 15 minutes is `0` (below the
/// minimum, not logged).
pub fn round_to_half_hour(seconds: i64) -> i64 {
    if seconds < HALF_HOUR_SECONDS / 2 {
        return 0;
    }
    ((seconds + HALF_HOUR_SECONDS - 1) / HALF_HOUR_SECONDS) * HALF_HOUR_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_up_to_half_hour_with_zero_floor_below_15_minutes() {
        assert_eq!(round_to_half_hour(0), 0);
        assert_eq!(round_to_half_hour(-100), 0); // negative must clamp, not wrap
        assert_eq!(round_to_half_hour(899), 0); // `<=` instead of `<` at the floor
        assert_eq!(round_to_half_hour(900), 1800); // `>` instead of `>=` at the floor
        assert_eq!(round_to_half_hour(1799), 1800);
        assert_eq!(round_to_half_hour(1800), 1800); // exact multiple must not bump up
        assert_eq!(round_to_half_hour(1801), 3600); // truncating division
        assert_eq!(round_to_half_hour(5100), 5400);
        assert_eq!(round_to_half_hour(5401), 7200);
    }
}
