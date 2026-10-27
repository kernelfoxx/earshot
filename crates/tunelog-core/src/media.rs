//! OS-agnostic description of what a system media session is playing.
//!
//! Platform backends (`tunelog-media`) translate GSMTC / MPRIS data into
//! these types; the tracker in this crate consumes them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// A point-in-time view of one media session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSnapshot {
    /// Stable id of the session within its backend (AUMID on Windows,
    /// D-Bus bus name on Linux).
    pub session_id: String,
    /// The application that owns the session, e.g. `Spotify.exe`,
    /// `chrome.exe`, `org.mpris.MediaPlayer2.firefox.instance_1_42`.
    pub source_app: String,
    pub status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// Artwork URL, when the backend exposes one (MPRIS). Firefox passes the
    /// original `https://` URL, Chromium a local `file://` copy.
    pub art_url: Option<String>,
    /// Artwork size in pixels, when the backend could read it. Used to tell
    /// 16:9 video thumbnails from square album covers.
    pub art_size: Option<(u32, u32)>,
    pub duration_ms: Option<u64>,
    pub position_ms: Option<u64>,
}
