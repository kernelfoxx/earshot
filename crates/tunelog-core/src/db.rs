//! Local SQLite storage.
//!
//! Plays keep denormalized names plus normalized keys. Listening time lives
//! in `segments`, so period totals are the overlap of segments with the
//! period and a play crossing midnight is split exactly.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::model::Service;
use crate::normalize;
use crate::period::Range;
use crate::tracker::Play;

pub type Result<T> = rusqlite::Result<T>;

/// Default minimum play length for a play to count in stats.
pub const DEFAULT_MIN_PLAY_MS: i64 = 30_000;

const MIGRATIONS: &[&str] = &[
    // v1
    "
    CREATE TABLE plays (
        id             INTEGER PRIMARY KEY,
        live_key       TEXT UNIQUE,
        service        TEXT,
        title          TEXT NOT NULL,
        artist         TEXT NOT NULL,
        album          TEXT,
        norm_title     TEXT NOT NULL,
        norm_artist    TEXT NOT NULL,
        source_app     TEXT,
        origin         TEXT NOT NULL,
        started_at     INTEGER NOT NULL,
        ended_at       INTEGER NOT NULL,
        played_ms      INTEGER NOT NULL,
        duration_ms    INTEGER,
        utc_offset_min INTEGER,
        estimated      INTEGER NOT NULL DEFAULT 0,
        duplicate_of   INTEGER REFERENCES plays(id) ON DELETE SET NULL
    );
    CREATE INDEX plays_started_at ON plays(started_at);
    CREATE INDEX plays_track ON plays(norm_artist, norm_title);

    CREATE TABLE segments (
        play_id  INTEGER NOT NULL REFERENCES plays(id) ON DELETE CASCADE,
        start_ms INTEGER NOT NULL,
        end_ms   INTEGER NOT NULL
    );
    CREATE INDEX segments_play ON segments(play_id);
    CREATE INDEX segments_time ON segments(start_ms, end_ms);
    ",
];

/// Listening time per service for one range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ServiceTotals {
    pub spotify: i64,
    pub youtube_music: i64,
    pub soundcloud: i64,
    /// Browser audio whose service is unknown. Not part of `total`.
    pub unclassified: i64,
    pub total: i64,
}

impl ServiceTotals {
    fn add(&mut self, service: Option<Service>, ms: i64) {
        match service {
            Some(Service::Spotify) => self.spotify += ms,
            Some(Service::YoutubeMusic) => self.youtube_music += ms,
            Some(Service::Soundcloud) => self.soundcloud += ms,
            None => self.unclassified += ms,
        }
        if service.is_some() {
            self.total += ms;
        }
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Inserts or updates a live play (keyed by `live_key`) with its segments.
    /// Called both for in-progress checkpoints and for finished plays.
    pub fn save_live_play(&mut self, play: &Play, utc_offset_min: i32) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id: i64 = tx.query_row(
            "INSERT INTO plays (live_key, service, title, artist, album, norm_title, norm_artist,
                                source_app, origin, started_at, ended_at, played_ms, duration_ms,
                                utc_offset_min)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'live', ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(live_key) DO UPDATE SET
                service = excluded.service, album = excluded.album,
                ended_at = excluded.ended_at, played_ms = excluded.played_ms,
                duration_ms = excluded.duration_ms
             RETURNING id",
            params![
                play.live_key,
                play.service.map(Service::as_str),
                play.title,
                play.artist,
                play.album,
                normalize::title(&play.title),
                normalize::artist(&play.artist),
                play.source_app,
                play.started_at,
                play.ended_at,
                play.played_ms,
                play.duration_ms.map(|d| d as i64),
                utc_offset_min,
            ],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM segments WHERE play_id = ?1", [id])?;
        {
            let mut ins =
                tx.prepare("INSERT INTO segments (play_id, start_ms, end_ms) VALUES (?1, ?2, ?3)")?;
            for (start, end) in &play.segments {
                ins.execute(params![id, start, end])?;
            }
        }
        tx.commit()
    }

    /// Removes a live play that turned out not to be music.
    pub fn delete_live_play(&mut self, live_key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM plays WHERE live_key = ?1", [live_key])?;
        Ok(())
    }

    /// Listening time inside `range`, counting only plays of at least
    /// `min_play_ms`.
    pub fn totals(&self, range: Range, min_play_ms: i64) -> Result<ServiceTotals> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT p.service,
                    SUM(MAX(0, MIN(s.end_ms, ?2) - MAX(s.start_ms, ?1)))
             FROM segments s JOIN plays p ON p.id = s.play_id
             WHERE s.end_ms > ?1 AND s.start_ms < ?2
               AND p.played_ms >= ?3 AND p.duplicate_of IS NULL
             GROUP BY p.service",
        )?;
        let mut totals = ServiceTotals::default();
        let rows = stmt.query_map(params![range.start_ms, range.end_ms, min_play_ms], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (service, ms) = row?;
            totals.add(service.as_deref().and_then(Service::from_str_id), ms);
        }
        Ok(totals)
    }

    /// Number of stored plays (all origins), mainly for tests and diagnostics.
    pub fn play_count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM plays", [], |r| r.get(0))
    }

    pub fn live_play_service(&self, live_key: &str) -> Result<Option<Option<String>>> {
        self.conn
            .query_row(
                "SELECT service FROM plays WHERE live_key = ?1",
                [live_key],
                |r| r.get(0),
            )
            .optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 1_000;

    fn play(key: &str, service: Option<Service>, segments: Vec<(i64, i64)>) -> Play {
        Play {
            live_key: key.into(),
            service,
            title: "Get Lucky (feat. Pharrell)".into(),
            artist: "Daft Punk".into(),
            album: None,
            source_app: "Spotify.exe".into(),
            duration_ms: Some(248_000),
            started_at: segments[0].0,
            ended_at: segments.last().unwrap().1,
            played_ms: segments.iter().map(|(a, b)| b - a).sum(),
            segments,
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = std::env::temp_dir().join(format!("tunelog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        Db::open(&path).unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(db.play_count().unwrap(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn checkpoint_then_finish_updates_in_place() {
        let mut db = Db::open_in_memory().unwrap();
        let mut p = play("s|0", None, vec![(0, 20 * S)]);
        db.save_live_play(&p, 180).unwrap();
        p.service = Some(Service::Spotify);
        p.segments = vec![(0, 20 * S), (30 * S, 70 * S)];
        p.played_ms = 60 * S;
        p.ended_at = 70 * S;
        db.save_live_play(&p, 180).unwrap();

        assert_eq!(db.play_count().unwrap(), 1);
        assert_eq!(
            db.live_play_service("s|0").unwrap(),
            Some(Some("spotify".into()))
        );
        let t = db.totals(Range::EVERYTHING, DEFAULT_MIN_PLAY_MS).unwrap();
        assert_eq!(t.spotify, 60 * S);
        assert_eq!(t.total, 60 * S);
    }

    #[test]
    fn totals_clip_segments_to_range() {
        let mut db = Db::open_in_memory().unwrap();
        // 23:59:00 → 00:02:00 across "midnight" at 100_000 s.
        let midnight = 100_000 * S;
        db.save_live_play(
            &play(
                "a|1",
                Some(Service::Spotify),
                vec![(midnight - 60 * S, midnight + 120 * S)],
            ),
            0,
        )
        .unwrap();
        let before = Range {
            start_ms: 0,
            end_ms: midnight,
        };
        let after = Range {
            start_ms: midnight,
            end_ms: midnight + 86_400 * S,
        };
        assert_eq!(
            db.totals(before, DEFAULT_MIN_PLAY_MS).unwrap().spotify,
            60 * S
        );
        assert_eq!(
            db.totals(after, DEFAULT_MIN_PLAY_MS).unwrap().spotify,
            120 * S
        );
    }

    #[test]
    fn short_plays_and_unclassified_are_excluded_from_total() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(
            &play("a|1", Some(Service::Soundcloud), vec![(0, 29 * S)]),
            0,
        )
        .unwrap();
        db.save_live_play(
            &play("a|2", Some(Service::Soundcloud), vec![(100 * S, 140 * S)]),
            0,
        )
        .unwrap();
        db.save_live_play(&play("b|1", None, vec![(0, 90 * S)]), 0)
            .unwrap();
        db.save_live_play(
            &play("c|1", Some(Service::YoutubeMusic), vec![(0, 31 * S)]),
            0,
        )
        .unwrap();

        let t = db.totals(Range::EVERYTHING, DEFAULT_MIN_PLAY_MS).unwrap();
        assert_eq!(t.soundcloud, 40 * S, "29 s play is below the threshold");
        assert_eq!(t.youtube_music, 31 * S);
        assert_eq!(t.unclassified, 90 * S);
        assert_eq!(t.total, 71 * S);

        // The threshold is applied at query time.
        let t = db.totals(Range::EVERYTHING, 0).unwrap();
        assert_eq!(t.soundcloud, 69 * S);
    }

    #[test]
    fn delete_removes_play_and_segments() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(&play("x|1", None, vec![(0, 60 * S)]), 0)
            .unwrap();
        db.delete_live_play("x|1").unwrap();
        assert_eq!(db.play_count().unwrap(), 0);
        let segs: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM segments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(segs, 0);
    }
}
