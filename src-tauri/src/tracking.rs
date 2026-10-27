//! Background tracking: polls the OS media sessions, feeds the tracker and
//! persists plays. Runs on its own thread; the UI may not exist at all.

use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, Offset, TimeZone};
use tauri::{AppHandle, Emitter};
use tunelog_core::db::Db;
use tunelog_core::tracker::{Play, Tracker, Update};

use crate::tray;

/// Safety-net poll interval; changes normally arrive as OS events.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Coalesces bursts of OS change events into one poll.
const DEBOUNCE: Duration = Duration::from_millis(250);
/// How often in-progress plays are written to disk.
const CHECKPOINT_EVERY_MS: i64 = 60_000;

pub const EVENT_NOW_PLAYING: &str = "now-playing";
pub const EVENT_STATS_UPDATED: &str = "stats-updated";

pub struct Shared {
    pub db: Mutex<Db>,
    pub tracker: Mutex<Tracker>,
    /// Backend name, or why live tracking is unavailable.
    pub backend: Mutex<Result<&'static str, String>>,
}

impl Shared {
    pub fn new(db: Db) -> Self {
        Shared {
            db: Mutex::new(db),
            tracker: Mutex::new(Tracker::default()),
            backend: Mutex::new(Err("starting".into())),
        }
    }

    /// Writes in-progress plays so stats include the current track.
    pub fn checkpoint(&self, now: i64) {
        let plays = self.tracker.lock().unwrap().in_progress(now);
        let mut db = self.db.lock().unwrap();
        for p in &plays {
            save(&mut db, p);
        }
    }

    /// Ends and saves all plays; called on exit.
    pub fn finish_all(&self) {
        let plays = self.tracker.lock().unwrap().finish_all(now_ms());
        let mut db = self.db.lock().unwrap();
        for p in &plays {
            save(&mut db, p);
        }
    }

    fn persist(&self, update: &Update) {
        let mut db = self.db.lock().unwrap();
        for p in &update.finished {
            save(&mut db, p);
        }
        for key in &update.discarded {
            if let Err(e) = db.delete_live_play(key) {
                log::warn!("failed to delete play {key}: {e}");
            }
        }
    }
}

pub fn spawn(app: AppHandle, shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("tracker".into())
        .spawn(move || run(app, shared))
        .expect("failed to spawn tracker thread");
}

fn run(app: AppHandle, shared: Arc<Shared>) {
    let mut source = match tunelog_media::platform_source() {
        Ok(s) => s,
        Err(e) => {
            log::error!("live tracking unavailable: {e}");
            *shared.backend.lock().unwrap() = Err(e.to_string());
            return;
        }
    };
    let (tx, rx) = sync_channel::<()>(1);
    if let Err(e) = source.subscribe(Box::new(move || {
        let _ = tx.try_send(());
    })) {
        // Polling alone still works, just with more latency.
        log::warn!("media change events unavailable: {e}");
    }
    *shared.backend.lock().unwrap() = Ok(source.name());

    let mut last_checkpoint = 0;
    let mut last_now_playing: Option<(String, String, bool)> = None;
    // Poll right away so a track already playing at startup counts fully.
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            wait_for_change(&rx);
        }

        let sessions = match source.sessions() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("failed to read media sessions: {e}");
                continue;
            }
        };
        let now = now_ms();
        let (update, now_playing) = {
            let mut tracker = shared.tracker.lock().unwrap();
            let update = tracker.update(now, &sessions);
            (update, tracker.now_playing(now))
        };
        shared.persist(&update);

        let mut stats_changed = !update.finished.is_empty() || !update.discarded.is_empty();
        if now - last_checkpoint >= CHECKPOINT_EVERY_MS {
            shared.checkpoint(now);
            last_checkpoint = now;
            stats_changed = true;
        }
        if stats_changed {
            let _ = app.emit(EVENT_STATS_UPDATED, ());
        }

        let key = now_playing
            .as_ref()
            .map(|p| (p.title.clone(), p.artist.clone(), p.service.is_some()));
        if key != last_now_playing {
            tray::set_now_playing(&app, now_playing.as_ref());
            let _ = app.emit(EVENT_NOW_PLAYING, &now_playing);
            last_now_playing = key;
        }
    }
}

/// Blocks until a media change event arrives or the poll interval passes.
fn wait_for_change(rx: &Receiver<()>) {
    match rx.recv_timeout(POLL_INTERVAL) {
        Ok(()) => {
            std::thread::sleep(DEBOUNCE);
            while rx.try_recv().is_ok() {}
        }
        Err(RecvTimeoutError::Timeout) => {}
        Err(RecvTimeoutError::Disconnected) => std::thread::sleep(POLL_INTERVAL),
    }
}

fn save(db: &mut Db, play: &Play) {
    if let Err(e) = db.save_live_play(play, utc_offset_min(play.started_at)) {
        log::warn!("failed to save play {}: {e}", play.live_key);
    }
}

fn utc_offset_min(ts_ms: i64) -> i32 {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map_or(0, |d| d.offset().fix().local_minus_utc() / 60)
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
