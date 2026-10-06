// goglite — klient API GOG.com
// Umożliwia pobranie listy gier i instalatorów.
// Wymaga ważnego tokena (Bearer).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, Result};

// ========================================================================
// MODELE ODPOWIEDZI API
// ========================================================================

/// Pojedyncza gra z endpointu `/account/getFilteredProducts`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Adres strony produktu w sklepie, np. `https://www.gog.com/game/xyz`.
    #[serde(default)]
    pub url: Option<String>,
    /// Gatunek zwracany przez `getFilteredProducts` (np. „Role-playing”).
    #[serde(default)]
    pub category: Option<String>,
    /// Data premiery (surowa wartość z API — obiekt `{ "date": "…" }` lub tekst).
    #[serde(default)]
    pub release_date: Option<serde_json::Value>,
    /// Dostępność systemów z `getFilteredProducts` (np. `{ "Windows": true, "Mac": false }`).
    #[serde(default)]
    pub works_on: Option<WorksOn>,
    /// Lista systemów zwracana przez API (np. `["windows","mac"]`).
    #[serde(default)]
    pub supported_operating_systems: Vec<String>,
    /// Znormalizowane platformy (`windows`/`linux`/`macos`) — uzupełniane przy synchronizacji.
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Dostępność systemów z pola `worksOn` w odpowiedzi `getFilteredProducts`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorksOn {
    #[serde(default, rename = "Windows")]
    pub windows: bool,
    #[serde(default, rename = "Mac")]
    pub mac: bool,
    #[serde(default, rename = "Linux")]
    pub linux: bool,
}

/// Zamienia informacje o platformach z GOG na znormalizowaną, unikalną listę
/// (`"windows"` / `"linux"` / `"macos"`). Preferuje `supportedOperatingSystems`,
/// a przy jego braku korzysta z `worksOn`.
pub fn derive_platforms(g: &GameSummary) -> Vec<String> {
    let mut out = Vec::new();
    for raw in &g.supported_operating_systems {
        push_platform(&mut out, raw);
    }
    if out.is_empty() {
        if let Some(w) = &g.works_on {
            if w.windows {
                push_platform(&mut out, "windows");
            }
            if w.mac {
                push_platform(&mut out, "mac");
            }
            if w.linux {
                push_platform(&mut out, "linux");
            }
        }
    }
    out
}

/// Dodaje znormalizowaną nazwę platformy (bez duplikatów); ignoruje nieznane wartości.
fn push_platform(out: &mut Vec<String>, raw: &str) {
    let norm = match raw.trim().to_ascii_lowercase().as_str() {
        "windows" | "win" => "windows",
        "linux" => "linux",
        "mac" | "macos" | "osx" | "darwin" => "macos",
        _ => return,
    };
    if !out.iter().any(|p| p == norm) {
        out.push(norm.to_string());
    }
}

/// Wyciąga datę premiery jako tekst („RRRR-MM-DD”) z surowej wartości API.
/// Odporne na różne formaty (obiekt `{ date }` albo zwykły łańcuch).
pub fn release_date_text(value: Option<serde_json::Value>) -> Option<String> {
    let text = match value? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Object(m) => {
            m.get("date").and_then(|d| d.as_str()).unwrap_or("").to_string()
        }
        _ => String::new(),
    };
    let text = text.trim();
    // Bierzemy wiodące „RRRR-MM-DD” (obcinamy godzinę, strefę, część ułamkową).
    let head = text.get(..10).unwrap_or("");
    let result = if is_iso_date(head) {
        head.to_string()
    } else {
        text.split_whitespace().next().unwrap_or("").to_string()
    };
    if result.is_empty() { None } else { Some(result) }
}

/// Sprawdza, czy tekst ma format `RRRR-MM-DD`.
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// Odpowiedź `GET https://embed.gog.com/account/getFilteredProducts`.
/// Lista produktów jest w `products`, a liczba stron w `totalPages` (na najwyższym poziomie).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductsPage {
    #[serde(default)]
    products: Vec<GameSummary>,
    #[serde(default = "one")]
    total_pages: u32,
}

fn one() -> u32 { 1 }

/// Wyciąga nazwę pliku z nagłówka `Content-Disposition`.
fn content_disposition_filename(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let value = headers
        .get(reqwest::header::CONTENT_DISPOSITION)?
        .to_str()
        .ok()?;
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            // Format RFC 5987: charset'lang'nazwa
            let rest = rest.trim().trim_matches('"');
            let name = rest.rsplit("''").next().unwrap_or(rest);
            if !name.is_empty() {
                return Some(name.to_string());
            }
        } else if let Some(rest) = part.strip_prefix("filename=") {
            let name = rest.trim().trim_matches('"');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Wyciąga nazwę pliku z adresu URL: ostatni niepusty segment ścieżki
/// (z pominięciem `query`/`fragmentu`), zdekodowany z percent-encoding
/// (`%2864bit%29` → `(64bit)`). Gdy nie da się nic wywnioskować, zwraca `installer.exe`.
fn filename_from_url(url: &str) -> String {
    // `url::Url` poprawnie oddziela host od ścieżki i samo usuwa `query`/`fragment`.
    let raw = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|segs| segs.filter(|s| !s.is_empty()).last())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let decoded = percent_decode(&raw);
    if decoded.trim().is_empty() {
        "installer.exe".to_string()
    } else {
        decoded
    }
}

/// Minimalny dekoder percent-encoding (`%XX`) — wystarczający dla nazw plików GOG.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Wyciąga slug (ostatni segment ścieżki) z adresu strony produktu.
fn slug_from_url(url: &str) -> Option<String> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Normalizuje adres okładki z `getFilteredProducts`.
/// Pole `image` to bazowy hash (np. `//images-2.gog-statics.com/<hash>`),
/// do którego GOG dokłada format pliku — my robimy to samo co minigalaxy:
/// prefiks `https:` oraz sufiks `_196.jpg`.
fn normalize_image(image: Option<String>) -> Option<String> {
    let mut u = image?;
    if u.trim().is_empty() {
        return None;
    }
    if let Some(rest) = u.strip_prefix("//") {
        u = format!("https://{rest}");
    } else if let Some(rest) = u.strip_prefix("http://") {
        u = format!("https://{rest}");
    }
    let has_ext = [".jpg", ".jpeg", ".png", ".webp"]
        .iter()
        .any(|ext| u.ends_with(ext));
    if !has_ext {
        u.push_str("_196.jpg");
    }
    Some(u)
}

/// Buduje zrzut ekranu (miniatura + pełny rozmiar) z pojedynczego wpisu API.
/// Zwraca `None`, gdy wpis nie ma adresu bazowego.
fn screenshot_from_entry(s: V2Screenshot) -> Option<GameScreenshot> {
    let href = s._links.this.href;
    if href.trim().is_empty() {
        return None;
    }
    Some(GameScreenshot {
        thumb: screenshot_url(&href, "product_card_screenshot_748"),
        full: screenshot_url(&href, "1600"),
    })
}

/// Wstawia `formatter` w miejsce znacznika `{formatter}` w adresie zrzutu ekranu
/// (np. `…abc_{formatter}.jpg` → `…abc_1600.jpg`). Gdy znacznik jest nieobecny,
/// zwraca adres bez zmian.
fn screenshot_url(href: &str, formatter: &str) -> String {
    if href.contains("{formatter}") {
        href.replace("{formatter}", formatter)
    } else {
        href.to_string()
    }
}

/// Zamienia opis HTML z GOG na czysty tekst (bezpieczny do wyświetlenia w UI).
/// Tagi blokowe (`<br>`, `<p>`, `<li>`, …) stają się separatorami linii,
/// a podstawowe encje (`&amp;`, `&lt;`, `&#8230;`) są dekodowane.
fn html_to_text(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '<' => {
                // Zbieramy nazwę tagu aż do `>`.
                let mut name = String::new();
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '>' {
                    name.push(chars[j]);
                    j += 1;
                }
                i = if j < chars.len() { j + 1 } else { j };
                let lower = name.trim().trim_start_matches('/').to_ascii_lowercase();
                let tag = lower.split_whitespace().next().unwrap_or("");
                if matches!(
                    tag,
                    "br" | "p" | "li" | "ul" | "ol" | "div" | "tr" | "h1" | "h2" | "h3"
                ) && !out.ends_with('\n')
                {
                    out.push('\n');
                }
            }
            '&' => {
                // Zbieramy encję aż do `;` (max 8 znaków).
                let mut ent = String::new();
                let mut j = i + 1;
                while j < chars.len() && chars[j] != ';' && ent.len() < 8 {
                    ent.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() && chars[j] == ';' {
                    out.push_str(&decode_entity(&ent));
                    i = j + 1;
                } else {
                    out.push('&');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    // Zostawiamy po jednej, przyciętej linii na akapit — czytelnie i bezpiecznie.
    out.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Dekoduje podstawowe encje HTML (nazwane i numeryczne).
fn decode_entity(ent: &str) -> String {
    match ent {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" | "#39" => "'".into(),
        "nbsp" => " ".into(),
        "hellip" => "…".into(),
        "mdash" => "—".into(),
        "ndash" => "–".into(),
        _ => {
            if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default()
            } else if let Some(dec) = ent.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

/// Odpowiedź z `GET /v2/games/{id}?expand=downloads`.
#[derive(Deserialize)]
struct GameDetails {
    #[serde(default)]
    downloads: Option<DownloadGroups>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DownloadGroups {
    /// `"downloads": [ { "os": "windows", "installers": [...] } ]`
    ByOs(Vec<OsGroup>),
    /// `"downloads": { "installers": [...] }`
    Flat { installers: Vec<Installer> },
}

impl DownloadGroups {
    fn into_installers(self) -> Vec<Installer> {
        match self {
            Self::Flat { installers } => installers,
            Self::ByOs(groups) => groups.into_iter().flat_map(|g| g.installers).collect(),
        }
    }
}

#[derive(Deserialize)]
struct OsGroup {
    #[serde(default)]
    installers: Vec<Installer>,
}

/// Instalator offline dla konkretnej gry/systemu.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installer {
    #[serde(rename = "id")]
    pub download_id: String,
    pub name: String,
    pub os: String,
    #[serde(default)]
    pub version: String,
    /// GOG zwraca rozmiar instalatora w polu `total_size` (w bajtach).
    #[serde(default, alias = "total_size")]
    pub size: u64,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub files: Vec<InstallerFile>,
}

/// Pojedynczy plik instalatora z pre‑sygnowanym linkiem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerFile {
    pub id: String,
    pub size: u64,
    /// Pre‑sygnowany URL do CDN GOG (lub ścieżka do dalszej resolucji).
    pub downlink: String,
}
/// Odpowiedź endpointu `api.gog.com/products/{id}/downlink/...`.
/// Sam w sobie **nie jest** plikiem — wskazuje prawdziwy plik na CDN GOG.
#[derive(Debug, Deserialize)]
struct DownlinkResponse {
    /// Bezpośredni URL pliku na CDN GOG.
    downlink: String,
    /// URL do pliku `.xml` z sumą kontrolną (opcjonalny).
    #[serde(default)]
    checksum: Option<String>,
}

/// Rozwiązany link instalatora: bezpośredni URL pliku, sugerowana nazwa i suma kontrolna.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFile {
    pub url: String,
    pub checksum_url: Option<String>,
    pub filename: String,
}

/// Spłaszczone metadane gry pobrane z `GET https://api.gog.com/v2/games/{id}`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMeta {
    /// Opis gry (HTML → czysty tekst).
    pub description: Option<String>,
    /// Producenci.
    pub developers: Vec<String>,
    /// Wydawcy.
    pub publishers: Vec<String>,
    /// Gatunki/tagi (poziom 1 = gatunek główny).
    pub genres: Vec<String>,
    /// Zrzuty ekranu (miniatura + wersja pełnowymiarowa).
    pub screenshots: Vec<GameScreenshot>,
}

/// Pojedynczy zrzut ekranu gry: miniatura (748 px) i wersja pełnowymiarowa (1600 px).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameScreenshot {
    pub thumb: String,
    pub full: String,
}

/// Surowa odpowiedź `GET https://api.gog.com/v2/games/{id}` (HAL+JSON) —
/// tylko pola, których potrzebujemy. Opis jest na najwyższym poziomie, natomiast
/// producenci, wydawcy, tagi i zrzuty ekranu siedzą w bloku `_embedded`.
#[derive(Deserialize)]
struct V2Game {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    _embedded: V2Embedded,
}

/// Blok `_embedded` w odpowiedzi v2 (HAL+JSON).
#[derive(Deserialize, Default)]
struct V2Embedded {
    #[serde(default)]
    developers: Vec<NamedEntity>,
    #[serde(default)]
    publishers: Vec<NamedEntity>,
    #[serde(default)]
    tags: Vec<NamedEntity>,
    #[serde(default)]
    screenshots: Vec<V2Screenshot>,
}

/// Pojedynczy zrzut ekranu z `_embedded.screenshots` — niesie szablon adresu.
#[derive(Deserialize)]
struct V2Screenshot {
    #[serde(default)]
    _links: V2ScreenshotLinks,
}

#[derive(Deserialize, Default)]
struct V2ScreenshotLinks {
    #[serde(default, rename = "self")]
    this: V2ScreenshotRef,
}

#[derive(Deserialize, Default)]
struct V2ScreenshotRef {
    #[serde(default)]
    href: String,
}

/// Encja z nazwą (producent, wydawca, tag).
#[derive(Deserialize)]
struct NamedEntity {
    #[serde(default)]
    name: String,
}

// ========================================================================
// KLIENT API
// ========================================================================

#[derive(Clone)]
pub struct GogClient {
    http: reqwest::Client,
    token: String,
    app: AppHandle,
}

impl GogClient {
    pub fn new(token: String, http: reqwest::Client, app: AppHandle) -> Self {
        Self { http, token, app }
    }

    pub async fn fetch_library(&self) -> Result<Vec<GameSummary>> {
        let mut all = Vec::new();
        let mut page = 1u32;
        let mut total_pages = 1u32;
        while page <= total_pages {
            let url = format!(
                "https://embed.gog.com/account/getFilteredProducts?mediaType=1&page={page}"
            );
            let resp: ProductsPage = self.get_json(&url).await?;
            total_pages = resp.total_pages.max(1);
            for mut g in resp.products {
                if g.slug.is_none() {
                    g.slug = g.url.as_deref().and_then(slug_from_url);
                }
                g.image = normalize_image(g.image);
                g.platforms = derive_platforms(&g);
                all.push(g);
            }
            let _ = self.app.emit("library:page", serde_json::json!({
                "page": page, "totalPages": total_pages,
                "progress": (page as f32 / total_pages as f32) * 100.0,
            }));
            page += 1;
        }
        Ok(all)
    }

    pub async fn installers(&self, game_id: i64) -> Result<Vec<Installer>> {
        let url = format!("https://api.gog.com/products/{game_id}?expand=downloads");
        let details: GameDetails = self.get_json(&url).await?;
        Ok(details.downloads.map(|d| d.into_installers()).unwrap_or_default())
    }

    /// Pobiera metadane gry (opis, producenci, wydawcy, gatunki) z **publicznego**
    /// endpointu `GET https://api.gog.com/v2/games/{id}`. Nie wymaga tokena.
    pub async fn game_meta(&self, game_id: i64, locale: &str) -> Result<GameMeta> {
        let url = format!("https://api.gog.com/v2/games/{game_id}?locale={locale}");
        let v2: V2Game = self.get_json_public(&url).await?;
        let description = v2
            .description
            .as_deref()
            .map(html_to_text)
            .filter(|s| !s.is_empty());
        let emb = v2._embedded;
        Ok(GameMeta {
            description,
            developers: emb.developers.into_iter().map(|d| d.name).filter(|s| !s.is_empty()).collect(),
            publishers: emb.publishers.into_iter().map(|p| p.name).filter(|s| !s.is_empty()).collect(),
            genres: emb.tags.into_iter().map(|t| t.name).filter(|s| !s.is_empty()).collect(),
            screenshots: emb.screenshots.into_iter().filter_map(screenshot_from_entry).collect(),
        })
    }

    /// Zamienia `files[].downlink` (endpoint metadanych GOG) na **bezpośredni** URL pliku.
    ///
    /// GOG w `?expand=downloads` podaje link
    /// `https://api.gog.com/products/{id}/downlink/installer/{file_id}`, który pod
    /// `GET` zwraca JSON `{"downlink":"<cdn>","checksum":"<cdn.xml>"}`. Trzeba go
    /// najpierw „rozwiązać”, inaczej pobierany jest sam JSON, a nie instalator.
    /// Gdy podany URL jest już bezpośrednim plikiem (np. `application/octet-stream`),
    /// jest zwracany bez zmian.
    pub async fn resolve_download(&self, file_downlink: &str) -> Result<ResolvedFile> {
        let resp = self
            .http
            .get(file_downlink)
            .bearer_auth(&self.token)
            .header("User-Agent", crate::USER_AGENT)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::AuthRequired);
        }
        if !status.is_success() && !status.is_redirection() {
            return Err(AppError::GogStatus(status));
        }

        // Nazwa z nagłówka jest preferowana, gdy serwer ją podaje (`Content-Disposition`).
        let cd = content_disposition_filename(resp.headers());
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Wszystko, co nie jest wyraźnie JSON-em/HTML-em, traktujemy jako plik binarny.
        let is_binary = !content_type.contains("json") && !content_type.contains("html");

        let (url, checksum_url) = if is_binary {
            (resp.url().to_string(), None)
        } else {
            let body: DownlinkResponse = resp
                .json()
                .await
                .map_err(|e| AppError::GogPayload(e.to_string()))?;
            (body.downlink, body.checksum)
        };

        if url.trim().is_empty() {
            return Err(AppError::GogPayload("Brak pola `downlink` w odpowiedzi GOG".into()));
        }

        let filename = cd.unwrap_or_else(|| filename_from_url(&url));
        Ok(ResolvedFile { url, checksum_url, filename })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self.http.get(url)
            .bearer_auth(&self.token)
            .header("User-Agent", crate::USER_AGENT)
            .header("X-Client-Build-Version", "2.0.0")
            .send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::AuthRequired);
        }
        if !status.is_success() { return Err(AppError::GogStatus(status)); }
        let json: serde_json::Value = resp.json().await?;
        serde_json::from_value(json).map_err(|e| AppError::GogPayload(e.to_string()))
    }

    /// Jak `get_json`, ale **bez** nagłówka autoryzacji — dla publicznych endpointów
    /// (np. `api.gog.com/v2/games/{id}`), które działają nawet po wygaśnięciu tokena.
    async fn get_json_public<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self.http.get(url)
            .header("User-Agent", crate::USER_AGENT)
            .header("X-Client-Build-Version", "2.0.0")
            .send().await?;
        let status = resp.status();
        if !status.is_success() { return Err(AppError::GogStatus(status)); }
        let json: serde_json::Value = resp.json().await?;
        serde_json::from_value(json).map_err(|e| AppError::GogPayload(e.to_string()))
    }
}
// ========================================================================
// TESTY
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "products": [
            {
                "id": 1097893768,
                "title": "Neverwinter Nights: Enhanced Edition",
                "image": "//images-2.gog-statics.com/8706f7fb87a4a41bc34254f3b49f59f96cf13d067b2c8bbfd8d41c327392052a",
                "url": "/game/neverwinter_nights_enhanced_edition_pack",
                "category": "Role-playing"
            },
            {
                "id": 1207666633,
                "title": "Toonstruck",
                "image": "",
                "url": "/game/toonstruck"
            }
        ],
        "totalPages": 4,
        "page": 1,
        "totalProducts": 63
    }"#;

    const PRODUCT: &str = r#"{
        "id": 1207666633,
        "title": "Toonstruck",
        "downloads": {
            "installers": [
                {
                    "id": "installer_windows_en",
                    "name": "Toonstruck",
                    "os": "windows",
                    "language": "en",
                    "language_full": "English",
                    "version": "1.0",
                    "total_size": 939524096,
                    "files": [
                        {"id": "en1installer0", "size": 1048576, "downlink": "https://api.gog.com/products/1207666633/downlink/installer/en1installer0"}
                    ]
                }
            ],
            "patches": [],
            "language_packs": [],
            "bonus_content": []
        }
    }"#;

    #[test]
    fn parsuje_odpowiedz_get_filtered_products() {
        let page: ProductsPage = serde_json::from_str(SAMPLE).expect("deserializacja");
        assert_eq!(page.total_pages, 4);
        assert_eq!(page.products.len(), 2);
        assert_eq!(page.products[0].id, 1097893768);
        assert_eq!(page.products[0].title, "Neverwinter Nights: Enhanced Edition");
        // Gatunek (`category`) jest dostępny już przy synchronizacji biblioteki.
        assert_eq!(page.products[0].category.as_deref(), Some("Role-playing"));
        assert_eq!(page.products[1].category, None);
    }

    #[test]
    fn wyznacza_platformy_z_workson_i_supported_os() {
        let json = r#"{
            "id": 1, "title": "X",
            "worksOn": {"Windows": true, "Mac": true, "Linux": false}
        }"#;
        let g: GameSummary = serde_json::from_str(json).unwrap();
        assert_eq!(derive_platforms(&g), vec!["windows", "macos"]);

        let json = r#"{ "id": 2, "title": "Y", "supportedOperatingSystems": ["windows","linux"] }"#;
        let g: GameSummary = serde_json::from_str(json).unwrap();
        assert_eq!(derive_platforms(&g), vec!["windows", "linux"]);
    }

    const V2_SAMPLE: &str = r#"{
        "description": "Opis gry<br>z linią &amp; encją",
        "_embedded": {
            "developers": [{"name": "Burst", "searchIndex": "burst"}],
            "publishers": [{"name": "Interplay", "searchIndex": "interplay"}],
            "tags": [
                {"id": 87, "name": "Adventure", "level": 1, "slug": "adventure"},
                {"id": 88, "name": "Point-and-click", "level": 2, "slug": "pointandclick"}
            ],
            "screenshots": [
                {"_links": {"self": {
                    "href": "https://images.gog-statics.com/abc123_{formatter}.jpg",
                    "templated": true,
                    "formatters": ["product_card_screenshot_748", "1600"]
                }}}
            ]
        }
    }"#;

    #[test]
    fn parsuje_metadane_v2_i_konwertuje_opis() {
        let v2: V2Game = serde_json::from_str(V2_SAMPLE).expect("deserializacja");
        let description = v2.description.as_deref().map(html_to_text).filter(|s| !s.is_empty());
        let emb = v2._embedded;
        let meta = GameMeta {
            description,
            developers: emb.developers.into_iter().map(|d| d.name).collect(),
            publishers: emb.publishers.into_iter().map(|p| p.name).collect(),
            genres: emb.tags.into_iter().map(|t| t.name).collect(),
            screenshots: emb.screenshots.into_iter().filter_map(screenshot_from_entry).collect(),
        };
        assert_eq!(meta.developers, vec!["Burst"]);
        assert_eq!(meta.publishers, vec!["Interplay"]);
        assert_eq!(meta.genres, vec!["Adventure", "Point-and-click"]);
        assert_eq!(meta.description.as_deref(), Some("Opis gry\nz linią & encją"));
        assert_eq!(meta.screenshots.len(), 1);
        assert_eq!(
            meta.screenshots[0].thumb,
            "https://images.gog-statics.com/abc123_product_card_screenshot_748.jpg"
        );
        assert_eq!(
            meta.screenshots[0].full,
            "https://images.gog-statics.com/abc123_1600.jpg"
        );
    }

    #[test]
    fn buduje_adres_zrzutu_ekranu() {
        // Znacznik `{formatter}` jest zastępowany wybranym formatem.
        assert_eq!(
            screenshot_url("https://images.gog-statics.com/abc_{formatter}.jpg", "1600"),
            "https://images.gog-statics.com/abc_1600.jpg"
        );
        // Brak znacznika → adres zwracany bez zmian.
        assert_eq!(
            screenshot_url("https://images.gog-statics.com/abc.jpg", "1600"),
            "https://images.gog-statics.com/abc.jpg"
        );
    }

    #[test]
    fn wyciaga_date_premiery_z_roznych_formatow() {
        let obj = serde_json::json!({"date": "2013-07-25 00:00:00.000000", "timezone_type": 3});
        assert_eq!(release_date_text(Some(obj)).as_deref(), Some("2013-07-25"));
        assert_eq!(
            release_date_text(Some(serde_json::json!("2015-02-10"))).as_deref(),
            Some("2015-02-10")
        );
        assert_eq!(release_date_text(None), None);
        assert_eq!(release_date_text(Some(serde_json::json!(12345))), None);
        // Format ISO z separatorem „T” (np. z api.gog.com/products/{id}).
        assert_eq!(
            release_date_text(Some(serde_json::json!("2015-02-10T08:00:00+0100"))).as_deref(),
            Some("2015-02-10")
        );
    }

    #[test]
    fn konwertuje_html_opisu_do_tekstu() {
        assert_eq!(
            html_to_text("Linia 1<br><br>A &amp; B<ul><li>Punkt</li></ul>"),
            "Linia 1\nA & B\nPunkt"
        );
        // Bez treści → pusty string (nie zostają puste linie).
        assert_eq!(html_to_text("<p></p>"), "");
        // Encje numeryczne (dziesiętne) są dekodowane.
        assert_eq!(html_to_text("A&#66;C"), "ABC");
        // Samotny ampersand bez encji zostaje bez zmian.
        assert_eq!(html_to_text("A & B"), "A & B");
    }

    #[test]
    fn parsuje_instalatory_z_products() {
        let details: GameDetails = serde_json::from_str(PRODUCT).expect("deserializacja");
        let installers = details
            .downloads
            .map(|d| d.into_installers())
            .unwrap_or_default();
        assert_eq!(installers.len(), 1);
        let i = &installers[0];
        assert_eq!(i.download_id, "installer_windows_en");
        assert_eq!(i.os, "windows");
        assert_eq!(i.version, "1.0");
        // Rozmiar pochodzi z pola `total_size`.
        assert_eq!(i.size, 939524096);
        assert_eq!(i.files.len(), 1);
        assert_eq!(
            i.files[0].downlink,
            "https://api.gog.com/products/1207666633/downlink/installer/en1installer0"
        );
    }

    #[test]
    fn wyciaga_slug_z_adresu_produktu() {
        assert_eq!(
            slug_from_url("https://www.gog.com/game/the_witcher_3_wild_hunt").as_deref(),
            Some("the_witcher_3_wild_hunt")
        );
        assert_eq!(slug_from_url("https://www.gog.com/game/xyz/").as_deref(), Some("xyz"));
        assert_eq!(slug_from_url("/game/toonstruck").as_deref(), Some("toonstruck"));
    }

    #[test]
    fn normalizuje_adres_okladki() {
        // Bazowy hash z getFilteredProducts → dokładamy `_196.jpg` i schemat https.
        assert_eq!(
            normalize_image(Some(
                "//images-2.gog-statics.com/8706f7fb87a4a41bc34254f3b49f59f96cf13d067b2c8bbfd8d41c327392052a".into()
            ))
            .as_deref(),
            Some(
                "https://images-2.gog-statics.com/8706f7fb87a4a41bc34254f3b49f59f96cf13d067b2c8bbfd8d41c327392052a_196.jpg"
            )
        );
        // Pełny adres z rozszerzeniem zostaje bez zmian (tylko schemat na https).
        assert_eq!(
            normalize_image(Some("http://images-2.gog.com/x.jpg".into())).as_deref(),
            Some("https://images-2.gog.com/x.jpg")
        );
        assert_eq!(normalize_image(None), None);
        assert_eq!(normalize_image(Some("".into())), None);
    }

    #[test]
    fn wyciaga_nazwe_z_content_disposition() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::CONTENT_DISPOSITION,
            reqwest::header::HeaderValue::from_static(
                "attachment; filename=\"setup_toonstruck_1.0_(12345).exe\"",
            ),
        );
        assert_eq!(
            content_disposition_filename(&h).as_deref(),
            Some("setup_toonstruck_1.0_(12345).exe")
        );

        let mut h2 = reqwest::header::HeaderMap::new();
        h2.insert(
            reqwest::header::CONTENT_DISPOSITION,
            reqwest::header::HeaderValue::from_static("attachment; filename*=UTF-8''setup_game.exe"),
        );
        assert_eq!(
            content_disposition_filename(&h2).as_deref(),
            Some("setup_game.exe")
        );

        assert_eq!(content_disposition_filename(&reqwest::header::HeaderMap::new()), None);
    }

    #[test]
    fn serializuje_id_instalatora_dla_frontendu() {
        let i = Installer {
            download_id: "installer_windows_en".into(),
            name: "Game".into(),
            os: "windows".into(),
            version: "1.0".into(),
            size: 10,
            language: Some("en".into()),
            files: vec![],
        };
        let v = serde_json::to_value(&i).unwrap();
        // Frontend odczytuje `i.id` — pole musi być serializowane jako "id".
        assert_eq!(v.get("id").and_then(|x| x.as_str()), Some("installer_windows_en"));
        assert_eq!(v.get("size").and_then(|x| x.as_u64()), Some(10));
        assert_eq!(v.get("os").and_then(|x| x.as_str()), Some("windows"));
    }

    #[test]
    fn wyciaga_nazwe_pliku_z_url_cdn_i_dekoduje() {
        // Przykład z realnego CDN GOG (percent-encoding w nawiasach).
        let u = "https://gog-cdn-fastly.gog.com/token=nva=1~dirs=6/secure/offline/1136126792/0/9734/setup_absolute_drift_5f6049d_%2864bit%29_%2865598%29.exe";
        assert_eq!(
            filename_from_url(u),
            "setup_absolute_drift_5f6049d_(64bit)_(65598).exe"
        );
    }

    #[test]
    fn wyciaga_nazwe_pliku_ignorujac_query_fragment_i_ukosnik() {
        assert_eq!(filename_from_url("https://x/a/setup.exe?token=abc#frag"), "setup.exe");
        assert_eq!(filename_from_url("https://x/a/"), "a");
        assert_eq!(filename_from_url("https://x/"), "installer.exe");
    }

    /// `files[].downlink` wskazuje endpoint metadanych; prawdziwy plik jest w polu `downlink`.
    #[test]
    fn parsuje_odpowiedz_downlink_na_url_pliku_i_nazwe() {
        let json = r#"{
            "downlink": "https://gog-cdn-fastly.gog.com/x/setup_game_%2864bit%29.exe",
            "checksum": "https://gog-cdn-fastly.gog.com/x/setup_game_%2864bit%29.exe.xml"
        }"#;
        let d: DownlinkResponse = serde_json::from_str(json).expect("deserializacja");
        assert_eq!(
            d.downlink,
            "https://gog-cdn-fastly.gog.com/x/setup_game_%2864bit%29.exe"
        );
        assert_eq!(
            d.checksum.as_deref(),
            Some("https://gog-cdn-fastly.gog.com/x/setup_game_%2864bit%29.exe.xml")
        );
        assert_eq!(filename_from_url(&d.downlink), "setup_game_(64bit).exe");
    }
}