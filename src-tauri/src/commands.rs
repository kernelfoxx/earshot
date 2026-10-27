use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use tunelog_core::db::{DEFAULT_MIN_PLAY_MS, ServiceTotals};
use tunelog_core::period::{self, PeriodKind};
use tunelog_core::tracker::Play;

use crate::tracking::{Shared, now_ms};

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
