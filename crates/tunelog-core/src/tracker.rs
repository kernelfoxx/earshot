//! Turns a stream of media session snapshots into plays.
//!
//! Only wall-clock time spent in `Playing` counts: browsers update the
//! reported playback position rarely, so position is only used to detect a
//! track being restarted. Every play keeps its real playing intervals
//! (segments), which lets stats split a play exactly across day boundaries.
//!
//! The tracker is pure: the caller passes the current time with every call.

use std::collections::HashMap;

use serde::Serialize;

use crate::classify::{Classification, classify};
use crate::media::{MediaSnapshot, PlaybackStatus};
use crate::model::Service;

/// If two updates are further apart than this, the machine was probably
/// asleep: the open segment is closed at the previous update.
pub const DEFAULT_MAX_GAP_MS: i64 = 60_000;

/// Position drops below this after being past [`RESTART_FROM_MS`] mean the
/// same track started over (repeat).
const RESTART_TO_MS: u64 = 3_000;
const RESTART_FROM_MS: u64 = 10_000;

/// A play that is in progress or has finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Play {
    /// Unique, stable id of a live play: `session_id|started_at`.
    pub live_key: String,
    /// `None` while the service could not be determined.
    pub service: Option<Service>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub source_app: String,
    pub duration_ms: Option<u64>,
    pub started_at: i64,
    pub ended_at: i64,
    pub played_ms: i64,
    /// Playing intervals `[start, end)` in UTC ms.
    pub segments: Vec<(i64, i64)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Update {
    /// Plays that ended in this update.
    pub finished: Vec<Play>,
    /// Live keys of plays that turned out not to be music and must be
    /// removed if they were already saved.
    pub discarded: Vec<String>,
}

#[derive(Debug)]
struct Active {
    title: String,
    artist: String,
    album: Option<String>,
    source_app: String,
    service: Option<Service>,
    duration_ms: Option<u64>,
    segments: Vec<(i64, i64)>,
    open_since: Option<i64>,
    last_position: Option<u64>,
    session_id: String,
}

impl Active {
    fn new(s: &MediaSnapshot, service: Option<Service>) -> Self {
        Active {
            title: s.title.clone(),
            artist: s.artist.clone(),
            album: non_empty(&s.album),
            source_app: s.source_app.clone(),
            service,
            duration_ms: s.duration_ms,
            segments: Vec::new(),
            open_since: None,
            last_position: s.position_ms,
            session_id: s.session_id.clone(),
        }
    }

    fn same_track(&self, s: &MediaSnapshot) -> bool {
        self.title == s.title && self.artist == s.artist
    }

    fn close(&mut self, at: i64) {
        if let Some(start) = self.open_since.take() {
            if at > start {
                self.segments.push((start, at));
            }
        }
    }

    fn started_at(&self) -> Option<i64> {
        self.segments.first().map(|s| s.0).or(self.open_since)
    }

    fn live_key(&self) -> Option<String> {
        self.started_at()
            .map(|t| format!("{}|{}", self.session_id, t))
    }

    /// Snapshot of this play, counting an open segment up to `now`.
    fn to_play(&self, now: i64) -> Option<Play> {
        let mut segments = self.segments.clone();
        if let Some(start) = self.open_since {
            if now > start {
                segments.push((start, now));
            }
        }
        let started_at = segments.first()?.0;
        let ended_at = segments.last()?.1;
        Some(Play {
            live_key: format!("{}|{}", self.session_id, started_at),
            service: self.service,
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            source_app: self.source_app.clone(),
            duration_ms: self.duration_ms,
            started_at,
            ended_at,
            played_ms: segments.iter().map(|(a, b)| b - a).sum(),
            segments,
        })
    }
}

#[derive(Debug)]
pub struct Tracker {
    active: HashMap<String, Active>,
    last_update: Option<i64>,
    max_gap_ms: i64,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_GAP_MS)
    }
}

impl Tracker {
    pub fn new(max_gap_ms: i64) -> Self {
        Tracker {
            active: HashMap::new(),
            last_update: None,
            max_gap_ms,
        }
    }

    /// Feed the full list of current sessions observed at `now`.
    pub fn update(&mut self, now: i64, sessions: &[MediaSnapshot]) -> Update {
        let mut out = Update::default();

        // Machine slept or the process was frozen: don't count the gap.
        if let Some(last) = self.last_update {
            if now - last > self.max_gap_ms {
                for a in self.active.values_mut() {
                    a.close(last);
                }
            }
        }
        self.last_update = Some(now);

        let mut seen = Vec::with_capacity(sessions.len());
        for s in sessions {
            seen.push(s.session_id.as_str());
            self.apply(now, s, &mut out);
        }

        // Sessions that disappeared.
        let gone: Vec<String> = self
            .active
            .keys()
            .filter(|k| !seen.contains(&k.as_str()))
            .cloned()
            .collect();
        for id in gone {
            if let Some(a) = self.active.remove(&id) {
                finish(a, now, &mut out);
            }
        }
        out
    }

    fn apply(&mut self, now: i64, s: &MediaSnapshot, out: &mut Update) {
        let service = match classify(s) {
            Classification::Music(svc) => Some(svc),
            Classification::Unclassified => None,
            Classification::Ignored => {
                if let Some(a) = self.active.remove(&s.session_id) {
                    if a.same_track(s) {
                        // Late artwork revealed it's a video: drop the play.
                        out.discarded.extend(a.live_key());
                    } else {
                        finish(a, now, out);
                    }
                }
                return;
            }
        };

        let restart = match self.active.get(&s.session_id) {
            Some(a) if !a.same_track(s) => true,
            Some(a) => matches!(
                (a.last_position, s.position_ms),
                (Some(prev), Some(cur)) if prev >= RESTART_FROM_MS && cur < RESTART_TO_MS
            ),
            None => false,
        };
        if restart {
            if let Some(a) = self.active.remove(&s.session_id) {
                finish(a, now, out);
            }
        }

        let a = self
            .active
            .entry(s.session_id.clone())
            .or_insert_with(|| Active::new(s, service));
        // Classification can improve while the same track plays
        // (e.g. artwork arrives after the title).
        if service.is_some() {
            a.service = service;
        }
        if a.album.is_none() {
            a.album = non_empty(&s.album);
        }
        if s.duration_ms.is_some() {
            a.duration_ms = s.duration_ms;
        }
        if s.position_ms.is_some() {
            a.last_position = s.position_ms;
        }
        match s.status {
            PlaybackStatus::Playing => {
                a.open_since.get_or_insert(now);
            }
            PlaybackStatus::Paused | PlaybackStatus::Stopped => a.close(now),
        }
    }

    /// Ends every play, e.g. on shutdown.
    pub fn finish_all(&mut self, now: i64) -> Vec<Play> {
        let mut out = Update::default();
        for (_, a) in self.active.drain() {
            finish(a, now, &mut out);
        }
        out.finished
    }

    /// Plays in progress (with time counted up to `now`), for checkpointing
    /// and the "now playing" view. Plays that never reached `Playing` are
    /// omitted.
    pub fn in_progress(&self, now: i64) -> Vec<Play> {
        self.active
            .values()
            .filter_map(|a| a.to_play(now))
            .collect()
    }

    /// The play currently in `Playing` state, if any.
    pub fn now_playing(&self, now: i64) -> Option<Play> {
        self.active
            .values()
            .filter(|a| a.open_since.is_some())
            .filter_map(|a| a.to_play(now))
            .max_by_key(|p| p.ended_at)
    }
}

fn finish(mut a: Active, now: i64, out: &mut Update) {
    a.close(now);
    if let Some(p) = a.to_play(now) {
        out.finished.push(p);
    }
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PlaybackStatus::*;

    const S: i64 = 1_000;

    fn spotify(title: &str, status: PlaybackStatus) -> MediaSnapshot {
        MediaSnapshot {
            session_id: "Spotify.exe".into(),
            source_app: "Spotify.exe".into(),
            status,
            title: title.into(),
            artist: "Artist".into(),
            album: Some("Album".into()),
            art_url: None,
            art_size: None,
            duration_ms: Some(200 * S as u64),
            position_ms: None,
        }
    }

    fn at_pos(mut s: MediaSnapshot, pos_s: u64) -> MediaSnapshot {
        s.position_ms = Some(pos_s * 1000);
        s
    }

    #[test]
    fn counts_only_playing_time() {
        let mut t = Tracker::default();
        let base = 1_000_000;
        assert!(t.update(base, &[spotify("A", Playing)]).finished.is_empty());
        t.update(base + 40 * S, &[spotify("A", Paused)]);
        t.update(base + 50 * S, &[spotify("A", Paused)]);
        t.update(base + 60 * S, &[spotify("A", Playing)]);
        let u = t.update(base + 80 * S, &[spotify("B", Playing)]);

        assert_eq!(u.finished.len(), 1);
        let p = &u.finished[0];
        assert_eq!(p.title, "A");
        assert_eq!(p.service, Some(Service::Spotify));
        assert_eq!(p.played_ms, 60 * S);
        assert_eq!(
            p.segments,
            vec![(base, base + 40 * S), (base + 60 * S, base + 80 * S)]
        );
        assert_eq!(p.started_at, base);
        assert_eq!(p.ended_at, base + 80 * S);
    }

    #[test]
    fn track_change_starts_new_play() {
        let mut t = Tracker::default();
        t.update(0, &[spotify("A", Playing)]);
        let u = t.update(30 * S, &[spotify("B", Playing)]);
        assert_eq!(u.finished[0].played_ms, 30 * S);
        let u = t.update(45 * S, &[]);
        assert_eq!(u.finished[0].title, "B");
        assert_eq!(u.finished[0].played_ms, 15 * S);
    }

    #[test]
    fn seek_does_not_split_but_restart_does() {
        let mut t = Tracker::default();
        t.update(0, &[at_pos(spotify("A", Playing), 0)]);
        // Seek forward: same play.
        assert!(
            t.update(10 * S, &[at_pos(spotify("A", Playing), 120)])
                .finished
                .is_empty()
        );
        // Seek back somewhere in the middle: same play.
        assert!(
            t.update(20 * S, &[at_pos(spotify("A", Playing), 60)])
                .finished
                .is_empty()
        );
        // Back to the start (repeat one): new play.
        let u = t.update(30 * S, &[at_pos(spotify("A", Playing), 1)]);
        assert_eq!(u.finished.len(), 1);
        assert_eq!(u.finished[0].played_ms, 30 * S);
        let p = t.now_playing(40 * S).unwrap();
        assert_eq!(p.started_at, 30 * S);
        assert_eq!(p.played_ms, 10 * S);
    }

    #[test]
    fn paused_from_start_is_not_a_play() {
        let mut t = Tracker::default();
        t.update(0, &[spotify("A", Paused)]);
        let u = t.update(100 * S, &[spotify("B", Paused)]);
        assert!(u.finished.is_empty());
        assert!(t.in_progress(200 * S).is_empty());
        assert!(t.now_playing(200 * S).is_none());
    }

    #[test]
    fn sleep_gap_is_not_counted() {
        let mut t = Tracker::default();
        t.update(0, &[spotify("A", Playing)]);
        t.update(5 * S, &[spotify("A", Playing)]);
        // Laptop lid closed for an hour; still "Playing" on wake.
        t.update(3_605 * S, &[spotify("A", Playing)]);
        let u = t.update(3_615 * S, &[]);
        assert_eq!(u.finished[0].played_ms, 15 * S);
        assert_eq!(u.finished[0].segments.len(), 2);
    }

    #[test]
    fn in_progress_counts_open_segment() {
        let mut t = Tracker::default();
        t.update(0, &[spotify("A", Playing)]);
        let p = t.in_progress(25 * S);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].played_ms, 25 * S);
        assert_eq!(p[0].live_key, "Spotify.exe|0");
        // The key stays stable while the play continues.
        t.update(50 * S, &[spotify("A", Playing)]);
        assert_eq!(t.in_progress(60 * S)[0].live_key, "Spotify.exe|0");
    }

    #[test]
    fn finish_all_closes_everything() {
        let mut t = Tracker::default();
        t.update(0, &[spotify("A", Playing)]);
        let done = t.finish_all(42 * S);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].played_ms, 42 * S);
        assert!(t.in_progress(50 * S).is_empty());
    }

    fn chrome(title: &str, art: Option<(u32, u32)>, album: Option<&str>) -> MediaSnapshot {
        MediaSnapshot {
            session_id: "Chrome".into(),
            source_app: "Chrome".into(),
            status: Playing,
            title: title.into(),
            artist: "Someone".into(),
            album: album.map(Into::into),
            art_url: None,
            art_size: art,
            duration_ms: None,
            position_ms: None,
        }
    }

    #[test]
    fn late_artwork_upgrades_classification() {
        let mut t = Tracker::default();
        t.update(0, &[chrome("Song", None, None)]);
        assert_eq!(t.now_playing(S).unwrap().service, None);
        t.update(2 * S, &[chrome("Song", Some((500, 500)), None)]);
        let p = t.now_playing(3 * S).unwrap();
        assert_eq!(p.service, Some(Service::Soundcloud));
        assert_eq!(p.started_at, 0);
    }

    #[test]
    fn late_artwork_revealing_video_discards_play() {
        let mut t = Tracker::default();
        t.update(0, &[chrome("Vlog #12", None, None)]);
        let u = t.update(2 * S, &[chrome("Vlog #12", Some((1280, 720)), None)]);
        assert!(u.finished.is_empty());
        assert_eq!(u.discarded, vec!["Chrome|0".to_string()]);
        assert!(t.now_playing(3 * S).is_none());
    }

    #[test]
    fn switching_to_video_finishes_music_play() {
        let mut t = Tracker::default();
        t.update(0, &[chrome("Song", Some((544, 544)), Some("LP"))]);
        let u = t.update(40 * S, &[chrome("Vlog", Some((1280, 720)), None)]);
        assert_eq!(u.finished.len(), 1);
        assert_eq!(u.finished[0].service, Some(Service::YoutubeMusic));
        assert!(u.discarded.is_empty());
    }

    #[test]
    fn independent_sessions() {
        let mut t = Tracker::default();
        let mut c = chrome("Song", Some((544, 544)), Some("LP"));
        t.update(0, &[spotify("A", Playing), c.clone()]);
        c.status = Paused;
        t.update(10 * S, &[spotify("A", Playing), c.clone()]);
        let mut done = t.finish_all(30 * S);
        done.sort_by_key(|p| p.played_ms);
        assert_eq!(done[0].played_ms, 10 * S);
        assert_eq!(done[1].played_ms, 30 * S);
    }
}
