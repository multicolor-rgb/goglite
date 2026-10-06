// goglite — trwałe ustawienia użytkownika (m.in. folder zapisu pobierań)

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Ustawienia aplikacji zapisywane w `settings.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Wybrany przez użytkownika folder docelowy pobierań.
    #[serde(default)]
    pub download_dir: Option<String>,
    /// Język interfejsu: `"pl"` lub `"en"` (puste = wykryj automatycznie).
    #[serde(default)]
    pub language: Option<String>,
    /// Motyw interfejsu: `"dark"` lub `"light"` (puste = domyślny ciemny).
    #[serde(default)]
    pub theme: Option<String>,
}

/// Wczytuje ustawienia z pliku; przy braku/błędzie zwraca wartość domyślną.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default()
}

/// Zapisuje ustawienia (tworzy katalog nadrzędny, jeśli trzeba).
pub fn save(path: &Path, settings: &Settings) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(path, data);
    }
}
