//! Relative and local dates for the start screen (D-0047).
//!
//! `relative_time` is a pure function of the stored RFC 3339 text and an
//! injected `now`, so its boundaries are tested exactly. `full_local_date`
//! converts the same text to the host's current local offset; when the
//! platform cannot report an offset it falls back to UTC rather than failing.

use std::time::{SystemTime, UNIX_EPOCH};

use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

/// A human age such as `just now`, `5 min ago`, `2 h ago`, `yesterday`,
/// `3 days ago`, or a short date for anything older than a week.
#[must_use]
pub fn relative_time(opened: &str, now: SystemTime) -> String {
    let Ok(opened) = OffsetDateTime::parse(opened, &Rfc3339) else {
        return "unknown".to_owned();
    };
    let now = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    #[allow(clippy::cast_possible_wrap)]
    let delta = now as i64 - opened.unix_timestamp();
    if delta < 60 {
        "just now".to_owned()
    } else if delta < 3_600 {
        format!("{} min ago", delta / 60)
    } else if delta < 86_400 {
        format!("{} h ago", delta / 3_600)
    } else if delta < 2 * 86_400 {
        "yesterday".to_owned()
    } else if delta < 7 * 86_400 {
        format!("{} days ago", delta / 86_400)
    } else {
        short_date(opened, now)
    }
}

/// The full date and time in the host's local offset, for a tooltip.
#[must_use]
pub fn full_local_date(opened: &str) -> String {
    let Ok(opened) = OffsetDateTime::parse(opened, &Rfc3339) else {
        return opened.to_owned();
    };
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local = opened.to_offset(offset);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.year(),
        u8::from(local.month()),
        local.day(),
        local.hour(),
        local.minute(),
        local.second()
    )
}

/// `Nov 4` for the current year, `Nov 4, 2025` otherwise.
fn short_date(opened: OffsetDateTime, now_seconds: u64) -> String {
    let month = month_name(opened.month());
    #[allow(clippy::cast_possible_wrap)]
    let now = OffsetDateTime::from_unix_timestamp(now_seconds as i64)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    if opened.year() == now.year() {
        format!("{month} {}", opened.day())
    } else {
        format!("{month} {}, {}", opened.day(), opened.year())
    }
}

fn month_name(month: time::Month) -> &'static str {
    match month {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn relative_ages_switch_at_the_boundaries() {
        let opened = "2026-11-04T00:00:00Z";
        let base = 1_793_750_400; // 2026-11-04T00:00:00Z
        assert_eq!(relative_time(opened, at(base)), "just now");
        assert_eq!(relative_time(opened, at(base + 59)), "just now");
        assert_eq!(relative_time(opened, at(base + 60)), "1 min ago");
        assert_eq!(relative_time(opened, at(base + 3_599)), "59 min ago");
        assert_eq!(relative_time(opened, at(base + 3_600)), "1 h ago");
        assert_eq!(relative_time(opened, at(base + 86_399)), "23 h ago");
        assert_eq!(relative_time(opened, at(base + 86_400)), "yesterday");
        assert_eq!(relative_time(opened, at(base + 2 * 86_400)), "2 days ago");
        assert_eq!(relative_time(opened, at(base + 3 * 86_400)), "3 days ago");
        assert_eq!(relative_time(opened, at(base + 7 * 86_400)), "Nov 4");
        // A future timestamp (a clock that moved back) reads as just now.
        assert_eq!(relative_time(opened, at(base - 10)), "just now");
        assert_eq!(relative_time("not a date", at(base)), "unknown");
    }

    #[test]
    fn an_old_timestamp_gets_a_short_date_with_its_year() {
        // 2025-11-04T00:00:00Z, seen one year later.
        let opened = "2025-11-04T00:00:00Z";
        let now = at(1_793_750_400);
        assert_eq!(relative_time(opened, now), "Nov 4, 2025");
    }

    #[test]
    fn the_local_tooltip_is_a_full_date() {
        let text = full_local_date("2026-11-04T03:15:04Z");
        assert!(text.starts_with("2026-11-04 "), "{text}");
        assert!(text.len() >= 19, "{text}");
    }
}
