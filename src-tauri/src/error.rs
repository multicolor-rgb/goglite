// goglite — typy błędów

use std::fmt;

/// Główny typ błędu aplikacji.
/// Implementuje `std::error::Error` oraz konwersję do `String` dla komend Tauri.
#[derive(Debug)]
pub enum AppError {
    /// Wymagane ponowne logowanie do GOG.
    AuthRequired,
    /// Błąd żądania HTTP (np. timeout, przerwane połączenie).
    Reqwest(reqwest::Error),
    /// Błąd odpowiedzi z API GOG (nieoczekiwany status).
    GogStatus(reqwest::StatusCode),
    /// Nieoczekiwana lub nieprawidłowa odpowiedź JSON z GOG.
    GogPayload(String),
    /// Błąd bazy danych SQLite.
    Sql(rusqlite::Error),
    /// Błąd wejścia/wyjścia (zapis pliku, tworzenie katalogów).
    Io(std::io::Error),
    /// Błąd parsowania adresu URL.
    Url(url::ParseError),
    /// Błąd ogólny z tekstem.
    Msg(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthRequired => write!(f, "Wymagane logowanie do GOG."),
            Self::Reqwest(e) => write!(f, "Błąd sieci: {e}"),
            Self::GogStatus(code) => write!(f, "GOG odpowiedział HTTP {code}"),
            Self::GogPayload(s) => write!(f, "Nieprawidłowa odpowiedź GOG: {s}"),
            Self::Sql(e) => write!(f, "Błąd bazy danych: {e}"),
            Self::Io(e) => write!(f, "Błąd zapisu/odczytu: {e}"),
            Self::Url(e) => write!(f, "Nieprawidłowy URL: {e}"),
            Self::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reqwest(e) => Some(e),
            Self::Sql(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Url(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self { AppError::Reqwest(e) }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self { AppError::Sql(e) }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e) }
}
impl From<url::ParseError> for AppError {
    fn from(e: url::ParseError) -> Self { AppError::Url(e) }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { AppError::GogPayload(e.to_string()) }
}

impl From<AppError> for String {
    fn from(e: AppError) -> String { e.to_string() }
}

/// Wynik zwracany przez większość funkcji aplikacji.
pub type Result<T> = std::result::Result<T, AppError>;