//! Lenient timestamp parsing for export files. Times without an offset are
//! treated as UTC, which is what Spotify, Google and SoundCloud use.

use chrono::{DateTime, NaiveDateTime};

pub fn parse_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.and_utc().timestamp_millis());
        }
    }
    for fmt in ["%Y-%m-%d %H:%M:%S %z", "%Y-%m-%d %H:%M:%S %Z"] {
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Some(dt.timestamp_millis());
        }
    }
    // Unix epoch in seconds or milliseconds.
    let n: i64 = s.parse().ok()?;
    Some(if n > 100_000_000_000 { n } else { n * 1000 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let expected = 1_704_112_496_000; // 2024-01-01T12:34:56Z
        for s in [
            "2024-01-01T12:34:56Z",
            "2024-01-01T12:34:56.000Z",
            "2024-01-01T14:34:56+02:00",
            "2024-01-01 12:34:56",
            "2024-01-01T12:34:56",
            "2024-01-01 12:34:56 +0000",
            "1704112496",
            "1704112496000",
        ] {
            assert_eq!(parse_ms(s), Some(expected), "{s}");
        }
        assert_eq!(parse_ms("2024-01-01 12:34"), Some(expected - 56_000));
        assert_eq!(parse_ms("yesterday"), None);
        assert_eq!(parse_ms(""), None);
    }
}
