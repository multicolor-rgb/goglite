// goglite — wykrywanie systemu operacyjnego (Windows / Linux / macOS).
// Używane do domyślnego ustawienia filtra platformy w bibliotece.

use serde::{Deserialize, Serialize};

/// Obsługiwane systemy operacyjne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Linux,
    Macos,
}

/// Informacje o systemie przekazywane do frontendu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub os: Os,
    pub arch: String,
}

/// Bieżący system operacyjny.
pub fn current_os() -> Os {
    if cfg!(target_os = "windows") {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    }
}

/// Informacje o systemie (OS + architektura).
pub fn os_info() -> OsInfo {
    OsInfo {
        os: current_os(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zwraca_aktualny_system() {
        let info = os_info();
        assert!(!info.arch.is_empty());
        // Wartość musi być jedną z obsługiwanych platform.
        let ok = matches!(info.os, Os::Windows | Os::Linux | Os::Macos);
        assert!(ok);
    }
}