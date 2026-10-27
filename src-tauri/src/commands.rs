use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tunelog_core::db::{
    DEFAULT_MIN_PLAY_MS, Db, ImportRecord, ImportReport, ServiceTotals, UnclassifiedGroup,
};
use tunelog_core::import::{self, ImportKind, InputFile, soundcloud};
use tunelog_core::model::Service;
use tunelog_core::normalize;
use tunelog_core::period::{self, PeriodKind};
use tunelog_core::tracker::Play;

use crate::tracking::{EVENT_STATS_UPDATED, Shared, now_ms};

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    platform: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}

#[derive(Serialize)]
pub struct PeriodTotals {
    period: PeriodKind,
    totals: ServiceTotals,
}

#[derive(Serialize)]
pub struct Overview {
    periods: Vec<PeriodTotals>,
    /// Name of the live tracking backend, if it is running.
    backend: Option<&'static str>,
    /// Why live tracking is not running.
    tracking_error: Option<String>,
}

#[tauri::command]
pub fn get_overview(shared: State<'_, Arc<Shared>>) -> CmdResult<Overview> {
    let now = now_ms();
    shared.checkpoint(now);
    let local_now = chrono::Local::now();
    let db = shared.db.lock().unwrap();
    let periods = PeriodKind::ALL
        .into_iter()
        .map(|kind| {
            let range = period::current(kind, &local_now);
            db.totals(range, DEFAULT_MIN_PLAY_MS)
                .map(|totals| PeriodTotals {
                    period: kind,
                    totals,
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let backend = shared.backend.lock().unwrap().clone();
    Ok(Overview {
        periods,
        backend: backend.as_ref().ok().copied(),
        tracking_error: backend.err(),
    })
}

#[tauri::command]
pub fn get_now_playing(shared: State<'_, Arc<Shared>>) -> Option<Play> {
    shared.tracker.lock().unwrap().now_playing(now_ms())
}

fn read(kind: ImportKind, paths: &[String]) -> Result<Vec<InputFile>, String> {
    let wanted = match kind {
        ImportKind::Spotify => import::spotify::wanted,
        ImportKind::Takeout => import::takeout::wanted,
        ImportKind::Soundcloud => soundcloud::wanted,
    };
    import::read_inputs(paths, wanted).map_err(|e| e.to_string())
}

fn known_duration(db: &Db) -> impl Fn(&str, &str) -> Option<i64> + '_ {
    |artist, title| {
        db.known_duration_ms(&normalize::artist(artist), &normalize::title(title))
            .ok()
            .flatten()
    }
}

fn file_label(paths: &[String]) -> String {
    let names: Vec<&str> = paths
        .iter()
        .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p))
        .collect();
    match names.as_slice() {
        [one] => (*one).to_owned(),
        [first, rest @ ..] => format!("{first} +{} more", rest.len()),
        [] => String::new(),
    }
}

/// Imports export files. Runs off the main thread: large Spotify histories
/// take a few seconds.
#[tauri::command]
pub async fn import_files(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    kind: ImportKind,
    paths: Vec<String>,
    mapping: Option<soundcloud::Mapping>,
) -> CmdResult<ImportReport> {
    let shared = shared.inner().clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> CmdResult<ImportReport> {
        let files = read(kind, &paths)?;
        let mut db = shared.db.lock().unwrap();
        let parsed = {
            let known = known_duration(&db);
            match kind {
                ImportKind::Spotify => import::spotify::parse(&files),
                ImportKind::Takeout => import::takeout::parse(&files, &known),
                ImportKind::Soundcloud => soundcloud::parse(&files, mapping.as_ref(), &known),
            }
            .map_err(|e| e.to_string())?
        };
        db.insert_imported(kind, &file_label(&paths), &parsed, now_ms())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit(EVENT_STATS_UPDATED, ());
    Ok(report)
}

#[tauri::command]
pub async fn preview_soundcloud(
    paths: Vec<String>,
    mapping: Option<soundcloud::Mapping>,
) -> CmdResult<soundcloud::Preview> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = read(ImportKind::Soundcloud, &paths)?;
        soundcloud::preview(&files, mapping.as_ref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_imports(shared: State<'_, Arc<Shared>>) -> CmdResult<Vec<ImportRecord>> {
    shared
        .db
        .lock()
        .unwrap()
        .imports()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_import(app: AppHandle, shared: State<'_, Arc<Shared>>, id: i64) -> CmdResult<()> {
    shared
        .db
        .lock()
        .unwrap()
        .delete_import(id)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_STATS_UPDATED, ());
    Ok(())
}

#[tauri::command]
pub fn get_unclassified(shared: State<'_, Arc<Shared>>) -> CmdResult<Vec<UnclassifiedGroup>> {
    shared.checkpoint(now_ms());
    shared
        .db
        .lock()
        .unwrap()
        .unclassified(DEFAULT_MIN_PLAY_MS)
        .map_err(|e| e.to_string())
}

/// `service: None` marks the artist as not music.
#[tauri::command]
pub fn assign_artist(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    norm_artist: String,
    service: Option<Service>,
) -> CmdResult<()> {
    shared
        .db
        .lock()
        .unwrap()
        .assign_artist(&norm_artist, service)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_STATS_UPDATED, ());
    Ok(())
}
