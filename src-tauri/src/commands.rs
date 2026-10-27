use serde::Serialize;

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
