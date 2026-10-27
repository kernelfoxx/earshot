//! Calendar periods in the user's local time zone. Weeks start on Monday.

use chrono::{DateTime, Datelike, Days, Months, NaiveDate, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodKind {
    Today,
    Week,
    Month,
    Year,
    All,
}

impl PeriodKind {
    pub const ALL: [PeriodKind; 5] = [
        PeriodKind::Today,
        PeriodKind::Week,
        PeriodKind::Month,
        PeriodKind::Year,
        PeriodKind::All,
    ];
}

/// Half-open interval `[start_ms, end_ms)` in UTC milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl Range {
    pub const EVERYTHING: Range = Range {
        start_ms: 0,
        end_ms: i64::MAX,
    };
}

/// The period of `kind` that contains `now`.
pub fn current<Tz: TimeZone>(kind: PeriodKind, now: &DateTime<Tz>) -> Range {
    let tz = now.timezone();
    let today = now.date_naive();
    let (first, next) = match kind {
        PeriodKind::All => return Range::EVERYTHING,
        PeriodKind::Today => (today, today + Days::new(1)),
        PeriodKind::Week => {
            let monday = today - Days::new(today.weekday().num_days_from_monday().into());
            (monday, monday + Days::new(7))
        }
        PeriodKind::Month => {
            let first = today.with_day(1).expect("day 1 exists");
            (first, first + Months::new(1))
        }
        PeriodKind::Year => {
            let first = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 exists");
            (first, first + Months::new(12))
        }
    };
    Range {
        start_ms: local_midnight_ms(&tz, first),
        end_ms: local_midnight_ms(&tz, next),
    }
}

/// Start of `date` in `tz`, in UTC ms. Handles DST gaps at midnight by
/// taking the first valid instant of the day.
fn local_midnight_ms<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> i64 {
    let mut t = date.and_time(NaiveTime::MIN);
    for _ in 0..4 {
        if let Some(dt) = tz.from_local_datetime(&t).earliest() {
            return dt.timestamp_millis();
        }
        t += chrono::Duration::minutes(30);
    }
    // Unreachable for real time zones; fall back to UTC interpretation.
    t.and_utc().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};
    use chrono_tz::Europe::Berlin;

    fn utc_ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    #[test]
    fn today_week_month_year_in_utc() {
        // Wednesday.
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 15, 0, 0).unwrap();
        let r = current(PeriodKind::Today, &now);
        assert_eq!(r.start_ms, utc_ms("2026-10-28T00:00:00Z"));
        assert_eq!(r.end_ms, utc_ms("2026-10-29T00:00:00Z"));

        let r = current(PeriodKind::Week, &now);
        assert_eq!(r.start_ms, utc_ms("2026-10-26T00:00:00Z"), "Monday");
        assert_eq!(r.end_ms, utc_ms("2026-11-02T00:00:00Z"));

        let r = current(PeriodKind::Month, &now);
        assert_eq!(r.start_ms, utc_ms("2026-10-01T00:00:00Z"));
        assert_eq!(r.end_ms, utc_ms("2026-11-01T00:00:00Z"));

        let r = current(PeriodKind::Year, &now);
        assert_eq!(r.start_ms, utc_ms("2026-01-01T00:00:00Z"));
        assert_eq!(r.end_ms, utc_ms("2027-01-01T00:00:00Z"));

        assert_eq!(current(PeriodKind::All, &now), Range::EVERYTHING);
    }

    #[test]
    fn sunday_belongs_to_week_starting_monday() {
        let now = Utc.with_ymd_and_hms(2026, 11, 1, 23, 59, 0).unwrap();
        let r = current(PeriodKind::Week, &now);
        assert_eq!(r.start_ms, utc_ms("2026-10-26T00:00:00Z"));
    }

    #[test]
    fn uses_local_midnight() {
        let tz = FixedOffset::east_opt(3 * 3600).unwrap();
        // 01:00 local on Oct 28 is still Oct 27 in UTC.
        let now = tz.with_ymd_and_hms(2026, 10, 28, 1, 0, 0).unwrap();
        let r = current(PeriodKind::Today, &now);
        assert_eq!(r.start_ms, utc_ms("2026-10-27T21:00:00Z"));
        assert_eq!(r.end_ms, utc_ms("2026-10-28T21:00:00Z"));
    }

    #[test]
    fn dst_change_day_is_25_hours() {
        // Europe/Berlin leaves DST on 2026-10-25.
        let now = Berlin.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap();
        let r = current(PeriodKind::Today, &now);
        assert_eq!(r.end_ms - r.start_ms, 25 * 3_600_000);
    }
}
