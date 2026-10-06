// goglite — uwierzytelnianie GOG (Auth Code flow jak GogDownloader)

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{AppError, Result};

pub const CLIENT_ID: &str = "46899977096215655";
/// Sekret klienta GOG Galaxy — WYMAGANY przez endpoint tokenów
/// (`https://auth.gog.com/token`). Bez niego GOG odpowiada
/// `invalid_client` i logowanie się nie udaje.
pub const CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
pub const AUTH_URL: &str = "https://auth.gog.com/auth";
pub const TOKEN_URL: &str = "https://auth.gog.com/token";
pub const REDIRECT_URI: &str = "https://embed.gog.com/on_login_success?origin=client";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GogToken {
    pub access_token: String,
    #[serde(default)] pub token_type: String,
    #[serde(default = "de")] pub expires_in: u64,
    pub issued_at: u64,
    #[serde(default)] pub refresh_token: Option<String>,
}
fn de() -> u64 { 3600 }

impl GogToken {
    pub fn expired(&self) -> bool {
        let d = SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok();
        let now = d.map(|d| d.as_secs()).unwrap_or(0);
        now >= self.issued_at.saturating_add(self.expires_in).saturating_sub(60)
    }
}

struct TokenStore { path: PathBuf }
impl TokenStore {
    fn new(path: PathBuf) -> Self { Self { path } }
    fn load(&self) -> Option<GogToken> {
        let d = std::fs::read(&self.path).ok()?;
        let t: GogToken = serde_json::from_slice(&d).ok()?;
        if t.expired() && t.refresh_token.is_none() { let _ = std::fs::remove_file(&self.path); None }
        else { Some(t) }
    }
    fn save(&self, tok: &GogToken) {
        if let Some(p) = self.path.parent() { let _ = std::fs::create_dir_all(p); }
        let Ok(d) = serde_json::to_vec(tok) else { return };
        let _ = std::fs::write(&self.path, &d);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
    }
    fn clear(&self) { let _ = std::fs::remove_file(&self.path); }
}

#[derive(Clone)]
pub struct AuthState {
    inner: Arc<RwLock<Option<GogToken>>>,
    store: Arc<TokenStore>,
    http: reqwest::Client,
}

impl AuthState {
    pub fn load(path: &PathBuf, http: &reqwest::Client) -> Self {
        let s = Arc::new(TokenStore::new(path.clone()));
        let t = s.load();
        if t.is_some() { log::info!("Token wczytany."); }
        Self { inner: Arc::new(RwLock::new(t)), store: s, http: http.clone() }
    }
    pub fn token(&self) -> Result<GogToken> {
        let g = self.inner.read().unwrap();
        match &*g { Some(t) if !t.expired() => Ok(t.clone()), _ => Err(AppError::AuthRequired) }
    }
    pub fn set_token(&self, tok: GogToken) {
        *self.inner.write().unwrap() = Some(tok.clone()); self.store.save(&tok);
    }
    pub fn clear(&self) { *self.inner.write().unwrap() = None; self.store.clear(); }

    pub async fn refresh_token(&self) -> Result<()> {
        let g = self.inner.read().unwrap();
        let rt = match &*g {
            Some(t) if t.refresh_token.is_some() => t.refresh_token.clone().unwrap(),
            _ => return Err(AppError::AuthRequired),
        };
        let b = refresh_body(&rt);
        let resp = self.http.post(TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded").body(b).send().await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(AppError::GogStatus(resp.status()));
        }
        let Ok(j) = resp.json::<serde_json::Value>().await else { return Err(AppError::AuthRequired) };
        let at = j.get("access_token").and_then(|v| v.as_str());
        if at.is_none() { return Err(AppError::AuthRequired); }
        let ei = j.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);
        let nr = j.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_string());
        let issued_at = SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs()).unwrap_or(0);
        let tok = GogToken {
            access_token: at.unwrap().to_string(),
            token_type: "bearer".into(), expires_in: ei,
            issued_at, refresh_token: match nr { Some(s) => Some(s), None => Some(rt) },
        };
        self.set_token(tok);
        Ok(())
    }
}

// ========== WYMIANA KODU NA TOKEN ==========

/// Buduje ciało żądania wymiany kodu autoryzacyjnego na token
/// (`application/x-www-form-urlencoded`). `client_secret` jest wymagany przez GOG.
fn authorization_code_body(code: &str) -> String {
    let mut b = String::new();
    url::form_urlencoded::Serializer::new(&mut b)
        .append_pair("client_id", CLIENT_ID)
        .append_pair("client_secret", CLIENT_SECRET)
        .append_pair("code", code)
        .append_pair("grant_type", "authorization_code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .finish();
    b
}

/// Buduje ciało żądania odświeżenia tokenu. `client_secret` jest wymagany przez GOG.
fn refresh_body(refresh_token: &str) -> String {
    let mut b = String::new();
    url::form_urlencoded::Serializer::new(&mut b)
        .append_pair("client_id", CLIENT_ID)
        .append_pair("client_secret", CLIENT_SECRET)
        .append_pair("grant_type", "refresh_token")
        .append_pair("refresh_token", refresh_token)
        .finish();
    b
}

async fn exchange_code(code: String, http: &reqwest::Client) -> Result<GogToken> {
    let b = authorization_code_body(&code);
    let resp = http.post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded").body(b).send().await?;
    let status = resp.status();
    if status != reqwest::StatusCode::OK {
        let txt = resp.text().await?;
        return Err(AppError::Msg(format!("Wymiana kodu nieudana: HTTP {} - {}", status, txt)));
    }
    let j: serde_json::Value = resp.json().await?;
    let at = j.get("access_token").and_then(|v| v.as_str());
    if at.is_none() { return Err(AppError::GogPayload("Brak access_token w odpowiedzi".into())); }
    let ei = j.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);
    let rt = j.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_string());
    let issued_at = SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs()).unwrap_or(0);
    Ok(GogToken {
        access_token: at.unwrap().to_string(),
        token_type: "bearer".into(), expires_in: ei,
        issued_at, refresh_token: rt,
    })
}

// ========== PARSOWANIE KODU Z URL ==========

pub fn parse_code_from_url(url: &str) -> Option<String> {
    let (_, q) = url.split_once('?')?;
    for pair in q.split('&') {
        // Parametr bez znaku '=' nie może przerwać wyszukiwania kodu.
        if let Some((k, v)) = pair.split_once('=') {
            if k == "code" && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ========== OKNO LOGOWANIA ==========

pub fn open_login_window(
    app: &AppHandle,
    http: &reqwest::Client,
    on_token: impl Fn(GogToken) + Send + Sync + 'static,
) -> tauri::Result<()> {
    if app.webview_windows().values().any(|w| w.label() == "gog-login") { return Ok(()); }

    let mut b = String::new();
    url::form_urlencoded::Serializer::new(&mut b)
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("layout", "client2")
        .finish();
    let url = format!("{}?{}", AUTH_URL, b).parse::<url::Url>().expect("URL");

    let http2 = http.clone();
    let cb = Arc::new(on_token);
    let app2 = app.clone();

    WebviewWindowBuilder::new(app, "gog-login", WebviewUrl::External(url))
        .title("Logowanie do GOG.com").inner_size(950.0, 720.0).resizable(false)
        .on_navigation(move |url| {
            let u = url.as_str();
            if !u.starts_with("https://embed.gog.com/on_login_success") { return true; }
            let code = parse_code_from_url(u);
            if code.is_none() { return true; }
            // Mamy kod — zatrzymaj nawigacje do bialej strony
            let h2 = http2.clone();
            let cb2 = cb.clone();
            let app3 = app2.clone();
            tauri::async_runtime::spawn(async move {
                match exchange_code(code.unwrap(), &h2).await {
                    Ok(token) => {
                        // Zamknij okno logowania
                        if let Some(win) = app3.get_webview_window("gog-login") {
                            let _ = win.close();
                        }
                        cb2(token);
                    }
                    Err(e) => {
                        // Nie połykaj błędu — poinformuj UI i loguj.
                        log::error!("Wymiana kodu na token nieudana: {e}");
                        let _ = app3.emit("auth:error", serde_json::json!({
                            "message": e.to_string()
                        }));
                    }
                }
            });
            return false; // Nie laduj bialej strony
        })
        .build()?;
    Ok(())
}
// ========== TESTY ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsuje_kod_z_typowego_przekierowania_gog() {
        let url = "https://embed.gog.com/on_login_success?origin=client&code=ABC123";
        assert_eq!(parse_code_from_url(url).as_deref(), Some("ABC123"));
    }

    #[test]
    fn parsuje_kod_gdy_jest_pierwszy() {
        let url = "https://embed.gog.com/on_login_success?code=XYZ789&origin=client";
        assert_eq!(parse_code_from_url(url).as_deref(), Some("XYZ789"));
    }

    #[test]
    fn ignoruje_parametry_bez_znaku_rownosci() {
        // Wcześniej taki parametr przerywał wyszukiwanie (błąd przez `?` w pętli).
        let url = "https://embed.gog.com/on_login_success?flag&origin=client&code=QWE";
        assert_eq!(parse_code_from_url(url).as_deref(), Some("QWE"));
    }

    #[test]
    fn zwraca_none_bez_kodu() {
        assert!(parse_code_from_url("https://embed.gog.com/on_login_success?origin=client").is_none());
        assert!(parse_code_from_url("https://embed.gog.com/on_login_success?code=").is_none());
        assert!(parse_code_from_url("https://embed.gog.com/on_login_success").is_none());
    }

    #[test]
    fn wymiana_kodu_zawiera_wymagany_client_secret() {
        // GOG odrzuca wymianę kodu bez client_secret (`invalid_client`).
        let body = authorization_code_body("KOD");
        assert!(body.contains(&format!("client_secret={CLIENT_SECRET}")));
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("code=KOD"));
        assert!(body.contains("redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient"));
    }

    #[test]
    fn odswiezanie_zawiera_wymagany_client_secret() {
        let body = refresh_body("RT");
        assert!(body.contains(&format!("client_secret={CLIENT_SECRET}")));
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("refresh_token=RT"));
    }

    #[test]
    fn token_wygasa_z_zapasem_60s() {
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let mut t = GogToken {
            access_token: "a".into(),
            token_type: "bearer".into(),
            expires_in: 3600,
            issued_at: now,
            refresh_token: None,
        };
        assert!(!t.expired());
        // Wygasa dokładnie w oknie 60 s od teraz → traktowany jako wygasły.
        t.issued_at = now - 3600 + 30;
        assert!(t.expired());
        // Wydany dawno temu.
        t.issued_at = 0;
        assert!(t.expired());
    }
}
