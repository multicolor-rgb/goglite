// goglite — biblioteka główna
// Łączy wszystkie moduły i uruchamia aplikację Tauri.

mod auth;
mod commands;
mod db;
mod downloads;
mod error;
mod gog;
mod platform;
mod settings;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

pub const USER_AGENT: &str = concat!("goglite/", env!("CARGO_PKG_VERSION"), " (+https://github.com/example/goglite)");

/// Pobiera ścieżkę katalogu danych aplikacji.
fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("goglite")
}

/// Uruchamia aplikację Tauri.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let data = data_dir();
    let _ = std::fs::create_dir_all(&data);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Katalog pobierań: zmienna środowiskowa > zapisane ustawienie > systemowy „Pobrane”.
            let settings_path = data.join("settings.json");
            let env_dir = std::env::var("GOGLITE_DOWNLOAD_DIR").ok().map(PathBuf::from);
            let download_dir = env_dir
                .or_else(|| settings::load(&settings_path).download_dir.map(PathBuf::from))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| dirs::download_dir().unwrap_or_else(std::env::temp_dir));
            let _ = std::fs::create_dir_all(&download_dir);

            let http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| Box::<dyn std::error::Error>::from(format!("reqwest: {e}")))?;

            // Osobny klient dla pobierań: instalatory mają po kilkaset MB/GB, więc globalny
            // limit czasu całego żądania (jak przy wywołaniach API) przerywałby pobieranie.
            // Zamiast tego pilnujemy tylko bezczynności odczytu (read_timeout).
            let dl_http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(20))
                .read_timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| Box::<dyn std::error::Error>::from(format!("reqwest: {e}")))?;

            let db = db::Database::open(&data.join("goglite.db"))?;
            let auth = auth::AuthState::load(&data.join("auth.json"), &http);
            let dl_mgr = downloads::DownloadManager::new(
                dl_http.clone(),
                db.clone(),
                app.handle().clone(),
                download_dir.clone(),
            );
            // Oznacz niedokończone pobierania jako wstrzymane
            let _ = db.mark_incomplete_paused();

            let state = commands::AppState {
                db,
                auth,
                http,
                downloads: dl_mgr,
                download_dir: Arc::new(Mutex::new(download_dir)),
                settings_path,
            };
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_auth_status,
            commands::login_via_www,
            commands::logout,
            commands::sync_library,
            commands::get_games,
            commands::get_installers,
            commands::get_game_details,
            commands::start_download,
            commands::pause_download,
            commands::cancel_download,
            commands::get_downloads,
            commands::get_download_dir,
            commands::pick_download_dir,
            commands::get_settings,
            commands::set_language,
            commands::set_theme,
            commands::detect_os,
        ])
        .run(tauri::generate_context!())
        .expect("Błąd krytyczny podczas uruchamiania aplikacji");
}