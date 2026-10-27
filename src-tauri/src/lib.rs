mod commands;
mod tracking;
mod tray;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tunelog_core::db::Db;

use crate::tracking::Shared;

/// Passed by the autostart entry so the app boots straight into the tray.
pub const MINIMIZED_ARG: &str = "--minimized";

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let app = tauri::Builder::default()
        // Must be registered first: a second launch just focuses the first one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_overview,
            commands::get_now_playing,
        ])
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Db::open(&dir.join("tunelog.db"))?;
            let shared = Arc::new(Shared::new(db));
            app.manage(shared.clone());
            tracking::spawn(app.handle().clone(), shared);

            tray::create(app.handle())?;
            if !std::env::args().any(|a| a == MINIMIZED_ARG) {
                tray::show_main_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // The window is destroyed on close to free the webview's memory;
            // tracking keeps running and the tray recreates the window.
            if let WindowEvent::Destroyed = event {
                log::debug!("window {} closed", window.label());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building TuneLog");

    app.run(|app, event| match event {
        // Closing the last window must not quit: "Quit" lives in the tray.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => app.state::<Arc<Shared>>().finish_all(),
        _ => {}
    });
}
