//! Local SQLite storage.
//!
//! Plays keep denormalized names plus normalized keys. Listening time lives
//! in `segments`, so period totals are the overlap of segments with the
//! period and a play crossing midnight is split exactly.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::import::{ImportKind, Parsed};
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
    // v2: imports, deduplication, manual classification
    "
    CREATE TABLE imports (
        id          INTEGER PRIMARY KEY,
        kind        TEXT NOT NULL,
        files       TEXT NOT NULL,
        imported_at INTEGER NOT NULL,
        added       INTEGER NOT NULL DEFAULT 0,
        duplicates  INTEGER NOT NULL DEFAULT 0,
        skipped     INTEGER NOT NULL DEFAULT 0
    );
    ALTER TABLE plays ADD COLUMN import_id INTEGER REFERENCES imports(id) ON DELETE CASCADE;
    ALTER TABLE plays ADD COLUMN import_key TEXT;
    CREATE UNIQUE INDEX plays_import_key ON plays(import_key);
    CREATE INDEX plays_import ON plays(import_id);

    -- Manual classification by artist; service NULL means not music.
    CREATE TABLE artist_rules (
        norm_artist TEXT PRIMARY KEY,
        service     TEXT
    );
    ",
];

/// Two plays of the same track closer than this are considered the same
/// listen when deduplicating.
const DEDUP_TOLERANCE_MS: i64 = 90_000;

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

/// Outcome of one import.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub import_id: i64,
    /// New plays stored.
    pub added: usize,
    /// Plays that matched an existing play (tracked live or from another export).
    pub duplicates: usize,
    /// Plays already present from an earlier import of the same data.
    pub already_imported: usize,
    /// Entries that were not music plays.
    pub skipped: usize,
    /// New plays whose duration is estimated.
    pub estimated: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportRecord {
    pub id: i64,
    pub kind: String,
    pub files: String,
    pub imported_at: i64,
    pub added: i64,
    pub duplicates: i64,
    pub skipped: i64,
}

/// Unclassified plays of one artist, for manual assignment.
#[derive(Debug, Clone, Serialize)]
pub struct UnclassifiedGroup {
    pub norm_artist: String,
    pub artist: String,
    pub plays: i64,
    pub total_ms: i64,
    pub sample_titles: Vec<String>,
    pub source_apps: Vec<String>,
}

enum Resolved {
    Service(Service),
    NotMusic,
    Unknown,
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
        let norm_title = normalize::title(&play.title);
        let norm_artist = normalize::artist(&play.artist);
        let service = match play.service {
            Some(s) => Some(s),
            None => match self.resolve(&norm_artist, &norm_title)? {
                Resolved::Service(s) => Some(s),
                Resolved::NotMusic => return self.delete_live_play(&play.live_key),
                Resolved::Unknown => None,
            },
        };
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
                service.map(Service::as_str),
                play.title,
                play.artist,
                play.album,
                norm_title,
                norm_artist,
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

    /// Service for an unclassified play: the user's artist rule, else the
    /// service this track was most often seen on (e.g. in imported history).
    fn resolve(&self, norm_artist: &str, norm_title: &str) -> Result<Resolved> {
        let rule: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT service FROM artist_rules WHERE norm_artist = ?1",
                [norm_artist],
                |r| r.get(0),
            )
            .optional()?;
        match rule {
            Some(None) => return Ok(Resolved::NotMusic),
            Some(Some(s)) => {
                if let Some(svc) = Service::from_str_id(&s) {
                    return Ok(Resolved::Service(svc));
                }
            }
            None => {}
        }
        let known: Option<String> = self
            .conn
            .query_row(
                "SELECT service FROM plays
                 WHERE norm_artist = ?1 AND norm_title = ?2 AND service IS NOT NULL
                 GROUP BY service ORDER BY COUNT(*) DESC LIMIT 1",
                [norm_artist, norm_title],
                |r| r.get(0),
            )
            .optional()?;
        Ok(known
            .as_deref()
            .and_then(Service::from_str_id)
            .map_or(Resolved::Unknown, Resolved::Service))
    }

    /// Typical length of a track, from the tracker or exact export plays.
    pub fn known_duration_ms(&self, norm_artist: &str, norm_title: &str) -> Result<Option<i64>> {
        self.conn.query_row(
            "SELECT COALESCE(MAX(duration_ms), MAX(CASE WHEN estimated = 0 THEN played_ms END))
             FROM plays WHERE norm_artist = ?1 AND norm_title = ?2",
            [norm_artist, norm_title],
            |r| r.get(0),
        )
    }

    /// Stores parsed export plays, skipping ones already imported and
    /// linking duplicates of plays from other sources.
    ///
    /// When the same listen exists twice, the more precise source wins:
    /// Spotify export > live tracker / exact exports > estimated exports.
    /// The other one stays stored with `duplicate_of` set and is excluded
    /// from stats.
    pub fn insert_imported(
        &mut self,
        kind: ImportKind,
        files: &str,
        parsed: &Parsed,
        now_ms: i64,
    ) -> Result<ImportReport> {
        let service = kind.service();
        let origin = kind.origin();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO imports (kind, files, imported_at) VALUES (?1, ?2, ?3)",
            params![origin, files, now_ms],
        )?;
        let import_id = tx.last_insert_rowid();
        let mut report = ImportReport {
            import_id,
            skipped: parsed.skipped,
            ..ImportReport::default()
        };
        {
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO plays
                    (service, title, artist, album, norm_title, norm_artist, origin, started_at,
                     ended_at, played_ms, estimated, import_id, import_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?;
            let mut segment =
                tx.prepare("INSERT INTO segments (play_id, start_ms, end_ms) VALUES (?1, ?2, ?3)")?;
            // Only plays from other sources: back-to-back repeats within one
            // export are real separate listens.
            let mut find_same = tx.prepare(
                "SELECT id, origin, estimated, service FROM plays
                 WHERE norm_artist = ?2 AND norm_title = ?3 AND origin != ?8
                   AND (service = ?4 OR service IS NULL) AND duplicate_of IS NULL
                   AND started_at < ?6 + ?7 AND ended_at > ?5 - ?7
                 ORDER BY ABS(started_at - ?5) LIMIT 1",
            )?;
            let mut mark_dup = tx.prepare("UPDATE plays SET duplicate_of = ?2 WHERE id = ?1")?;
            let mut set_service =
                tx.prepare("UPDATE plays SET service = ?2 WHERE id = ?1 AND service IS NULL")?;

            for p in &parsed.plays {
                let norm_title = normalize::title(&p.title);
                let norm_artist = normalize::artist(&p.artist);
                let key = format!("{origin}|{}|{norm_artist}|{norm_title}", p.started_at);
                let inserted = insert.execute(params![
                    service.as_str(),
                    p.title,
                    p.artist,
                    p.album,
                    norm_title,
                    norm_artist,
                    origin,
                    p.started_at,
                    p.ended_at(),
                    p.played_ms,
                    p.estimated,
                    import_id,
                    key,
                ])?;
                if inserted == 0 {
                    report.already_imported += 1;
                    continue;
                }
                let id = tx.last_insert_rowid();
                segment.execute(params![id, p.started_at, p.ended_at()])?;

                let existing = find_same
                    .query_row(
                        params![
                            id,
                            norm_artist,
                            norm_title,
                            service.as_str(),
                            p.started_at,
                            p.ended_at(),
                            DEDUP_TOLERANCE_MS,
                            origin,
                        ],
                        |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, bool>(2)?,
                                r.get::<_, Option<String>>(3)?,
                            ))
                        },
                    )
                    .optional()?;
                match existing {
                    Some((other, other_origin, other_estimated, other_service)) => {
                        if other_service.is_none() {
                            set_service.execute(params![other, service.as_str()])?;
                        }
                        if priority(origin, p.estimated) > priority(&other_origin, other_estimated)
                        {
                            mark_dup.execute(params![other, id])?;
                            report.added += 1;
                            report.estimated += usize::from(p.estimated);
                        } else {
                            mark_dup.execute(params![id, other])?;
                        }
                        report.duplicates += 1;
                    }
                    None => {
                        report.added += 1;
                        report.estimated += usize::from(p.estimated);
                    }
                }
            }
        }
        tx.execute(
            "UPDATE imports SET added = ?2, duplicates = ?3, skipped = ?4 WHERE id = ?1",
            params![
                import_id,
                report.added as i64,
                report.duplicates as i64,
                report.skipped as i64
            ],
        )?;
        tx.commit()?;
        Ok(report)
    }

    pub fn imports(&self) -> Result<Vec<ImportRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, files, imported_at, added, duplicates, skipped
             FROM imports ORDER BY imported_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ImportRecord {
                id: r.get(0)?,
                kind: r.get(1)?,
                files: r.get(2)?,
                imported_at: r.get(3)?,
                added: r.get(4)?,
                duplicates: r.get(5)?,
                skipped: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    /// Removes an import and its plays. Plays it had marked as duplicates
    /// count again.
    pub fn delete_import(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM imports WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Unclassified plays long enough to count, grouped by artist.
    pub fn unclassified(&self, min_play_ms: i64) -> Result<Vec<UnclassifiedGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT norm_artist, COUNT(*), SUM(played_ms),
                    (SELECT artist FROM plays q WHERE q.norm_artist = p.norm_artist
                       AND q.service IS NULL ORDER BY started_at DESC LIMIT 1),
                    GROUP_CONCAT(DISTINCT title), GROUP_CONCAT(DISTINCT source_app)
             FROM plays p
             WHERE service IS NULL AND duplicate_of IS NULL AND played_ms >= ?1
             GROUP BY norm_artist ORDER BY SUM(played_ms) DESC",
        )?;
        let split = |s: Option<String>, n: usize| -> Vec<String> {
            s.map(|s| s.split(',').take(n).map(str::to_owned).collect())
                .unwrap_or_default()
        };
        let rows = stmt.query_map([min_play_ms], |r| {
            Ok(UnclassifiedGroup {
                norm_artist: r.get(0)?,
                plays: r.get(1)?,
                total_ms: r.get(2)?,
                artist: r.get(3)?,
                sample_titles: split(r.get(4)?, 3),
                source_apps: split(r.get(5)?, 3),
            })
        })?;
        rows.collect()
    }

    /// Assigns all unclassified plays of an artist to `service`, or deletes
    /// them when `None` (not music). Future plays follow the same rule.
    pub fn assign_artist(&mut self, norm_artist: &str, service: Option<Service>) -> Result<()> {
        let tx = self.conn.transaction()?;
        let svc = service.map(Service::as_str);
        tx.execute(
            "INSERT INTO artist_rules (norm_artist, service) VALUES (?1, ?2)
             ON CONFLICT(norm_artist) DO UPDATE SET service = excluded.service",
            params![norm_artist, svc],
        )?;
        match svc {
            Some(s) => tx.execute(
                "UPDATE plays SET service = ?2 WHERE norm_artist = ?1 AND service IS NULL",
                params![norm_artist, s],
            )?,
            None => tx.execute(
                "DELETE FROM plays WHERE norm_artist = ?1 AND service IS NULL",
                [norm_artist],
            )?,
        };
        tx.commit()
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

/// Higher wins when the same listen comes from two sources.
fn priority(origin: &str, estimated: bool) -> u8 {
    match (origin, estimated) {
        ("spotify_export", _) => 3,
        (_, true) => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportedPlay;

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
        // Windows can't delete a file that is still open.
        drop(db);
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
        let mut unknown = play("b|1", None, vec![(0, 90 * S)]);
        unknown.artist = "Unknown Artist".into();
        db.save_live_play(&unknown, 0).unwrap();
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

    fn imported(title: &str, artist: &str, start: i64, ms: i64, estimated: bool) -> ImportedPlay {
        ImportedPlay {
            title: title.into(),
            artist: artist.into(),
            album: None,
            started_at: start,
            played_ms: ms,
            estimated,
        }
    }

    fn parsed(plays: Vec<ImportedPlay>) -> Parsed {
        Parsed { plays, skipped: 0 }
    }

    fn all(db: &Db) -> ServiceTotals {
        db.totals(Range::EVERYTHING, DEFAULT_MIN_PLAY_MS).unwrap()
    }

    #[test]
    fn reimport_is_idempotent() {
        let mut db = Db::open_in_memory().unwrap();
        let data = parsed(vec![imported("Get Lucky", "Daft Punk", 0, 200 * S, false)]);
        let r1 = db
            .insert_imported(ImportKind::Spotify, "a.json", &data, 1)
            .unwrap();
        let r2 = db
            .insert_imported(ImportKind::Spotify, "a.json", &data, 2)
            .unwrap();
        assert_eq!((r1.added, r1.already_imported), (1, 0));
        assert_eq!((r2.added, r2.already_imported), (0, 1));
        assert_eq!(all(&db).spotify, 200 * S);
    }

    #[test]
    fn repeats_within_one_export_are_kept() {
        let mut db = Db::open_in_memory().unwrap();
        let data = parsed(vec![
            imported("Get Lucky", "Daft Punk", 0, 200 * S, false),
            imported("Get Lucky", "Daft Punk", 200 * S, 200 * S, false),
        ]);
        let r = db
            .insert_imported(ImportKind::Spotify, "a.json", &data, 1)
            .unwrap();
        assert_eq!((r.added, r.duplicates), (2, 0));
        assert_eq!(all(&db).spotify, 400 * S);
    }

    #[test]
    fn spotify_export_supersedes_live_play() {
        let mut db = Db::open_in_memory().unwrap();
        // Tracker saw 190 s; the export says 200 s, starting 20 s earlier.
        db.save_live_play(
            &play("s|1", Some(Service::Spotify), vec![(20 * S, 210 * S)]),
            0,
        )
        .unwrap();
        let r = db
            .insert_imported(
                ImportKind::Spotify,
                "x.json",
                &parsed(vec![imported("Get Lucky", "Daft Punk", 0, 200 * S, false)]),
                1,
            )
            .unwrap();
        assert_eq!((r.added, r.duplicates), (1, 1));
        assert_eq!(all(&db).spotify, 200 * S, "counted once, from the export");
    }

    #[test]
    fn live_play_beats_estimated_takeout() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(
            &play("c|1", Some(Service::YoutubeMusic), vec![(0, 150 * S)]),
            0,
        )
        .unwrap();
        let r = db
            .insert_imported(
                ImportKind::Takeout,
                "watch-history.json",
                &parsed(vec![
                    // "Get Lucky (feat. Pharrell)" by "Daft Punk - Topic" normalizes the same.
                    imported("Get Lucky", "Daft Punk - Topic", 2 * S, 210 * S, true),
                    imported("Strobe", "deadmau5", 600 * S, 210 * S, true),
                ]),
                1,
            )
            .unwrap();
        assert_eq!((r.added, r.duplicates, r.estimated), (1, 1, 1));
        assert_eq!(all(&db).youtube_music, 150 * S + 210 * S);
    }

    #[test]
    fn import_classifies_overlapping_unclassified_live_play() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(&play("c|1", None, vec![(0, 150 * S)]), 0)
            .unwrap();
        db.insert_imported(
            ImportKind::Takeout,
            "w.json",
            &parsed(vec![imported("Get Lucky", "Daft Punk", 0, 210 * S, true)]),
            1,
        )
        .unwrap();
        let t = all(&db);
        assert_eq!(t.unclassified, 0);
        assert_eq!(t.youtube_music, 150 * S, "live play kept, now classified");
    }

    #[test]
    fn deleting_import_restores_superseded_plays() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(&play("s|1", Some(Service::Spotify), vec![(0, 190 * S)]), 0)
            .unwrap();
        let r = db
            .insert_imported(
                ImportKind::Spotify,
                "x.json",
                &parsed(vec![imported("Get Lucky", "Daft Punk", 0, 200 * S, false)]),
                1,
            )
            .unwrap();
        db.delete_import(r.import_id).unwrap();
        assert_eq!(all(&db).spotify, 190 * S);
        assert!(db.imports().unwrap().is_empty());
    }

    #[test]
    fn known_track_classifies_new_live_plays() {
        let mut db = Db::open_in_memory().unwrap();
        db.insert_imported(
            ImportKind::Soundcloud,
            "h.csv",
            &parsed(vec![imported("Get Lucky", "Daft Punk", 0, 200 * S, false)]),
            1,
        )
        .unwrap();
        db.save_live_play(&play("c|9", None, vec![(10_000 * S, 10_100 * S)]), 0)
            .unwrap();
        assert_eq!(
            db.live_play_service("c|9").unwrap(),
            Some(Some("soundcloud".into()))
        );
    }

    #[test]
    fn artist_rules() {
        let mut db = Db::open_in_memory().unwrap();
        db.save_live_play(&play("c|1", None, vec![(0, 60 * S)]), 0)
            .unwrap();
        let groups = db.unclassified(DEFAULT_MIN_PLAY_MS).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].artist, "Daft Punk");
        assert_eq!(groups[0].total_ms, 60 * S);

        db.assign_artist("daft punk", Some(Service::Soundcloud))
            .unwrap();
        assert_eq!(all(&db).soundcloud, 60 * S);
        assert!(db.unclassified(DEFAULT_MIN_PLAY_MS).unwrap().is_empty());

        // Later unclassified plays by the same artist follow the rule.
        db.save_live_play(&play("c|2", None, vec![(100 * S, 160 * S)]), 0)
            .unwrap();
        assert_eq!(all(&db).soundcloud, 120 * S);

        // "Not music" removes them and drops future ones.
        let mut other = play("c|3", None, vec![(200 * S, 260 * S)]);
        other.artist = "Podcast Host".into();
        db.save_live_play(&other, 0).unwrap();
        db.assign_artist("podcast host", None).unwrap();
        other.live_key = "c|4".into();
        db.save_live_play(&other, 0).unwrap();
        assert_eq!(all(&db).unclassified, 0);
        assert_eq!(db.play_count().unwrap(), 2);
    }

    #[test]
    fn known_duration_prefers_tracker_length() {
        let mut db = Db::open_in_memory().unwrap();
        assert_eq!(
            db.known_duration_ms("daft punk", "get lucky").unwrap(),
            None
        );
        db.save_live_play(&play("s|1", Some(Service::Spotify), vec![(0, 60 * S)]), 0)
            .unwrap();
        assert_eq!(
            db.known_duration_ms("daft punk", "get lucky").unwrap(),
            Some(248_000)
        );
    }
}
