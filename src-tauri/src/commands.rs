// goglite — komendy Tauri (most IPC Rust ↔ Frontend)
// Aplikacja ogranicza się do przeglądania biblioteki GOG i pobierania instalatorów offline.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::auth::{self, AuthState};
use crate::db::{Database, DownloadRow, GameRow};
use crate::downloads::DownloadManager;
use crate::error::AppError;
use crate::gog::{self, GogClient, Installer, ResolvedFile};
use crate::platform;
use crate::settings;

pub struct AppState {
    pub db: Database,
    pub auth: AuthState,
    pub http: reqwest::Client,
    pub downloads: DownloadManager,
    /// Aktualny folder docelowy pobierań (można go zmienić w trakcie działania aplikacji).
    pub download_dir: Arc<Mutex<PathBuf>>,
    /// Ścieżka pliku `settings.json` (trwały zapis folderu).
    pub settings_path: PathBuf,
}

// —————————— AUTH ——————————

#[tauri::command]
pub fn get_auth_status(state: State<'_, AppState>) -> serde_json::Value {
    let logged_in = state.auth.token().is_ok();
    serde_json::json!({ "loggedIn": logged_in })
}

#[tauri::command]
pub fn login_via_www(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let auth = state.auth.clone();
    let app_clone = app.clone();
    auth::open_login_window(&app, &state.http, move |token| {
        auth.set_token(token);
        let _ = app_clone.emit("auth:changed", serde_json::json!({ "loggedIn": true }));
    }).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) {
    state.auth.clear();
}

// —————————— KATALOG POBIERAŃ ——————————

#[tauri::command]
pub fn get_download_dir(state: State<'_, AppState>) -> String {
    state.download_dir.lock().unwrap().to_string_lossy().to_string()
}

/// Otwiera natywne okno wyboru folderu i zapamiętuje wybór (trwale w `settings.json`).
/// Zwraca `None`, jeśli użytkownik anulował.
#[tauri::command]
pub async fn pick_download_dir(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let start = state.download_dir.lock().unwrap().clone();
    // Blokujące okno dialogowe — async command wykonuje się poza wątkiem głównym.
    let picked = app
        .dialog()
        .file()
        .set_directory(&start)
        .blocking_pick_folder();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    *state.download_dir.lock().unwrap() = path.clone();
    // Zachowujemy pozostałe ustawienia (język, motyw).
    let mut saved = settings::load(&state.settings_path);
    saved.download_dir = Some(path.to_string_lossy().to_string());
    settings::save(&state.settings_path, &saved);
    Ok(Some(path.to_string_lossy().to_string()))
}

// —————————— USTAWIENIA ——————————

/// Zwraca aktualne ustawienia aplikacji dla frontendu.
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> serde_json::Value {
    let saved = settings::load(&state.settings_path);
    let download_dir = state.download_dir.lock().unwrap().to_string_lossy().to_string();
    serde_json::json!({
        "downloadDir": download_dir,
        "language": saved.language,
        "theme": saved.theme,
    })
}

/// Zapisuje wybrany język interfejsu (`"pl"`/`"en"`), zachowując folder pobierań.
#[tauri::command]
pub fn set_language(state: State<'_, AppState>, language: String) -> Result<(), String> {
    let mut saved = settings::load(&state.settings_path);
    saved.language = Some(language);
    settings::save(&state.settings_path, &saved);
    Ok(())
}

/// Zapisuje wybrany motyw interfejsu (`"dark"`/`"light"`), zachowując pozostałe ustawienia.
#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> Result<(), String> {
    let mut saved = settings::load(&state.settings_path);
    saved.theme = Some(theme);
    settings::save(&state.settings_path, &saved);
    Ok(())
}

/// Awaryjna nazwa pliku, gdy serwer nie poda `Content-Disposition`.
fn fallback_filename(installer: &Installer, index: usize, total: usize) -> String {
    let base: String = installer
        .name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect();
    let base = if base.trim().is_empty() { "installer".to_string() } else { base };
    if total <= 1 || index == 0 {
        format!("{base}.exe")
    } else {
        format!("{base}-{index}.bin")
    }
}

// —————————— SYSTEM ——————————

/// Zwraca informacje o systemie (Windows/Linux/macOS + architektura).
#[tauri::command]
pub fn detect_os() -> platform::OsInfo {
    platform::os_info()
}

// —————————— BIBLIOTEKA ——————————

#[tauri::command]
pub async fn sync_library(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<GameRow>, String> {
    let token = state.auth.token().map_err(|e| e.to_string())?;
    let client = GogClient::new(token.access_token, state.http.clone(), app);
    let games = client.fetch_library().await.map_err(|e| e.to_string())?;
    let rows: Vec<GameRow> = games.into_iter().map(|g| GameRow {
        id: g.id, title: g.title, slug: g.slug.unwrap_or_default(),
        image_url: g.image, icon_url: g.icon,
        genre: g.category,
        release_date: gog::release_date_text(g.release_date),
        // Platformy dostępne dla gry (Windows/Linux/macOS) — do filtrowania biblioteki.
        platforms: g.platforms,
        // Producenci/wydawcy są znani dopiero po zapytaniu do v2 API (przy otwarciu gry).
        developers: Vec::new(), publishers: Vec::new(),
    }).collect();
    let count = rows.len();
    state.db.upsert_games(&rows).map_err(|e| e.to_string())?;
    log::info!("Zsynchronizowano {} gier.", count);
    Ok(rows)
}

#[tauri::command]
pub fn get_games(state: State<'_, AppState>) -> Result<Vec<GameRow>, String> {
    state.db.list_games().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_installers(app: AppHandle, state: State<'_, AppState>, game_id: i64) -> Result<Vec<Installer>, String> {
    let token = state.auth.token().map_err(|e| e.to_string())?;
    let client = GogClient::new(token.access_token, state.http.clone(), app);
    client.installers(game_id).await.map_err(|e| e.to_string())
}

/// Pobiera metadane gry (opis, producenci, wydawcy, gatunki) z **publicznego**
/// endpointu `api.gog.com/v2/games/{id}`. Nie wymaga aktywnego tokena — działa
/// nawet po jego wygaśnięciu. Producenci/wydawcy są cache'owani w bazie.
#[tauri::command]
pub async fn get_game_details(
    app: AppHandle, state: State<'_, AppState>, game_id: i64, language: Option<String>,
) -> Result<gog::GameMeta, String> {
    let token = state.auth.token().map(|t| t.access_token).unwrap_or_default();
    let client = GogClient::new(token, state.http.clone(), app);
    let locale = match language.as_deref() {
        Some(l) if l.to_ascii_lowercase().starts_with("pl") => "pl-PL",
        _ => "en-US",
    };
    let meta = client.game_meta(game_id, locale).await.map_err(|e| e.to_string())?;
    if !meta.developers.is_empty() || !meta.publishers.is_empty() {
        let _ = state.db.set_game_meta(game_id, &meta.developers, &meta.publishers);
    }
    Ok(meta)
}

// —————————— POBIERANIE ——————————

#[tauri::command]
pub async fn start_download(
    app: AppHandle, state: State<'_, AppState>,
    game_id: i64, installer_id: String,
) -> Result<Vec<DownloadRow>, String> {
    let token = state.auth.token().map_err(|e| e.to_string())?;
    let client = GogClient::new(token.access_token.clone(), state.http.clone(), app);
    let installers = client.installers(game_id).await.map_err(|e| e.to_string())?;
    let installer = installers.into_iter()
        .find(|i| i.download_id == installer_id)
        .ok_or_else(|| "Nie znaleziono instalatora".to_string())?;

    let dir = state.download_dir.lock().unwrap().clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let total_files = installer.files.iter().filter(|f| !f.downlink.is_empty()).count();
    let mut index = 0usize;
    for file in &installer.files {
        if file.downlink.is_empty() { continue; }
        // `file.downlink` to endpoint metadanych GOG — rozwiązujemy go na bezpośredni URL pliku.
        // Bez tego kroku pobierany jest sam JSON, a nie instalator.
        let resolved = match client.resolve_download(&file.downlink).await {
            Ok(r) => r,
            Err(AppError::AuthRequired) => return Err(AppError::AuthRequired.to_string()),
            Err(e) => {
                log::warn!("Nie udało się rozwiązać linku instalatora ({e}); używam surowego linku.");
                ResolvedFile {
                    url: file.downlink.clone(),
                    checksum_url: None,
                    filename: fallback_filename(&installer, index, total_files),
                }
            }
        };
        if let Some(cs) = resolved.checksum_url.as_deref() {
            log::debug!("Suma kontrolna dla {}: {cs}", resolved.filename);
        }
        let filename = resolved.filename;
        let target = dir.join(&filename);
        let row_id = state.db.upsert_download(
            game_id, &installer.download_id, &filename,
            &resolved.url, &target, file.size,
        ).map_err(|e| e.to_string())?;
        state.downloads.start(row_id, resolved.url, target, token.access_token.clone())
            .map_err(|e| e.to_string())?;
        index += 1;
    }
    state.db.list_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadRow>, String> {
    state.db.list_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause_download(state: State<'_, AppState>, download_id: i64) {
    state.downloads.pause(download_id);
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, download_id: i64) {
    state.downloads.cancel(download_id);
}