//! Parsers for users' own data exports.
//!
//! Parsers are pure: they turn file contents into [`ImportedPlay`]s.
//! Storing and deduplicating them against tracked plays is done by
//! [`crate::db::Db::insert_imported`].

mod archive;
pub mod soundcloud;
pub mod spotify;
pub mod takeout;
mod time;

pub use archive::{InputFile, read_inputs};

use serde::{Deserialize, Serialize};

use crate::model::Service;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// Spotify Extended Streaming History (or the shorter account data export).
    Spotify,
    /// Google Takeout YouTube watch history (YouTube Music entries only).
    Takeout,
    /// SoundCloud GDPR export.
    Soundcloud,
}

impl ImportKind {
    pub fn service(self) -> Service {
        match self {
            ImportKind::Spotify => Service::Spotify,
            ImportKind::Takeout => Service::YoutubeMusic,
            ImportKind::Soundcloud => Service::Soundcloud,
        }
    }

    /// Stored in `plays.origin`.
    pub fn origin(self) -> &'static str {
        match self {
            ImportKind::Spotify => "spotify_export",
            ImportKind::Takeout => "takeout",
            ImportKind::Soundcloud => "soundcloud_export",
        }
    }
}

/// One play from an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPlay {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub started_at: i64,
    pub played_ms: i64,
    /// `played_ms` is a guess (the export has no duration).
    pub estimated: bool,
}

impl ImportedPlay {
    pub fn ended_at(&self) -> i64 {
        self.started_at + self.played_ms
    }
}

/// Result of parsing one or more files.
#[derive(Debug, Default)]
pub struct Parsed {
    pub plays: Vec<ImportedPlay>,
    /// Entries that are not music plays (podcasts, videos, removed items…).
    pub skipped: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("could not read {0}: {1}")]
    Io(String, std::io::Error),
    #[error("{0} is not a valid archive: {1}")]
    Zip(String, zip::result::ZipError),
    #[error("{0}: {1}")]
    Format(String, String),
    #[error("no {0} files found in the selection")]
    NothingFound(&'static str),
}

/// Duration estimation for exports that only record when a track started.
pub mod estimate {
    /// A gap below this to the next play means the track was skipped.
    pub const MAX_GAP_MS: i64 = 6 * 60_000;
    /// Used when nothing better is known.
    pub const DEFAULT_MS: i64 = 210_000;

    /// Estimates how long a play lasted.
    ///
    /// * `gap_ms` — time until the next play in the same export, if any;
    /// * `known_ms` — the track's real duration, if seen by the tracker.
    pub fn played_ms(gap_ms: Option<i64>, known_ms: Option<i64>) -> i64 {
        match (gap_ms.filter(|g| *g > 0), known_ms.filter(|k| *k > 0)) {
            (Some(gap), Some(known)) => gap.min(known),
            (None, Some(known)) => known,
            (Some(gap), None) if gap <= MAX_GAP_MS => gap,
            _ => DEFAULT_MS,
        }
    }
}

/// Fills `played_ms` for plays that only have a start time, using the gap
/// to the next play. `known` looks up a track's real duration by raw
/// artist and title. Sorts `plays` by start time.
fn estimate_in_order(plays: &mut [ImportedPlay], known: &dyn Fn(&str, &str) -> Option<i64>) {
    plays.sort_by_key(|p| p.started_at);
    for i in 0..plays.len() {
        let gap = plays.get(i + 1).map(|n| n.started_at - plays[i].started_at);
        let p = &plays[i];
        let known_ms = known(&p.artist, &p.title);
        let played = estimate::played_ms(gap, known_ms);
        plays[i].played_ms = played;
        plays[i].estimated = true;
    }
}

#[cfg(test)]
mod tests {
    use super::estimate::*;

    #[test]
    fn estimation_rules() {
        // Next play 2 min later, real length 3 min: listened 2 min.
        assert_eq!(played_ms(Some(120_000), Some(180_000)), 120_000);
        // Long gap, known length: the whole track.
        assert_eq!(played_ms(Some(3_600_000), Some(180_000)), 180_000);
        // Unknown length, short gap: up to the next play.
        assert_eq!(played_ms(Some(200_000), None), 200_000);
        // Skipped after 10 s.
        assert_eq!(played_ms(Some(10_000), None), 10_000);
        // Unknown length, long gap or last entry: default.
        assert_eq!(played_ms(Some(3_600_000), None), DEFAULT_MS);
        assert_eq!(played_ms(None, None), DEFAULT_MS);
    }
}
