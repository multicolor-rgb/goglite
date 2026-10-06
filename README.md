# goglite — pobierarka instalatorów offline GOG

Lekka, wieloplatformowa aplikacja desktopowa do logowania do **GOG.com**, przeglądania biblioteki i pobierania instalatorów offline.

**Stos:** Tauri 2 · Rust · SQLite · Vanilla JS · Vite

---

## 🚀 Szybki start

### 1. Wymagania systemowe

| Składnik | Linux (Ubuntu/Debian) | Windows |
|----------|-----------------------|---------|
| **Node.js** | ≥ 20 | ≥ 20 |
| **Rust** | ≥ 1.75 ([rustup](https://rustup.rs)) | ≥ 1.75 |
| **GTK/Tauri** | `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev` | — |
| **C++** | `sudo apt install build-essential` | MSVC Build Tools |

### 2. Instalacja

```bash
git clone https://github.com/twoje/goglite.git
cd goglite
npm install
```

### 3. Uruchomienie (tryb deweloperski)

```bash
npm run tauri dev
```

### 4. Budowanie pliku binarnego

```bash
npm run tauri build
```

Pliki wynikowe: `src-tauri/target/release/bundle/`

---

## 🗂 Struktura projektu

```
goglite/
├── package.json                 # Zależności frontendowe
├── vite.config.js               # Konfiguracja Vite
├── index.html                   # Główny plik HTML
├── src/                         # Frontend (Vanilla JS + CSS)
│   ├── main.js                  # Logika UI (invoke, listen, render)
│   ├── i18n.js                  # Słowniki PL/EN + mechanizm tłumaczeń
│   └── styles.css               # Motywy jasny/ciemny (zmienne CSS), sidebar, siatka, panel pobierań
├── src-tauri/                   # Backend (Rust)
│   ├── Cargo.toml               # Zależności Rust
│   ├── tauri.conf.json          # Konfiguracja okna, bundle
---

## 🔐 Uwierzytelnianie

OAuth 2.0 Implicit Grant z `client_id` GOG Galaxy (`46899917096242505`).

1. Klik "Zaloguj" → okno z `auth.gog.com/auth?...`
2. Po zalogowaniu GOG przekierowuje → `embed.gog.com/on_login_success#access_token=...`
3. Backend przechwytuje nawigację, wyciąga token, zamyka okno.
4. Token zapisany w `~/.local/share/goglite/auth.json` (`chmod 600`).

## 📡 API GOG

| Endpoint | Cel |
|----------|-----|
| `GET embed.gog.com/account/getFilteredProducts` | Lista gier (tytuł, okładka, **gatunek `category`**, data premiery, **platformy `worksOn`**) |
| `GET api.gog.com/products/{id}?expand=downloads` | Instalatory + pliki |
| `GET files[].downlink` (`api.gog.com/products/{id}/downlink/installer/…`) | JSON `{ downlink, checksum }` z linkiem do CDN |
| `GET <downlink>` z tego JSON | Plik instalatora (CDN, obsługuje `Accept-Ranges`) |
| `GET api.gog.com/v2/games/{id}` | **Opis, producenci, wydawcy, gatunki, zrzuty ekranu** (publiczny — bez tokena) |

Wymaga `Authorization: Bearer <token>` (poza `api.gog.com/v2/games/{id}`, które jest publiczne).

## 🗄 Schemat SQLite

```sql
games (id INTEGER PK, title, slug, image_url, icon_url, genre, release_date,
       developers, publishers, platforms, last_synced_at)
downloads (id INTEGER PK, game_id FK, installer_id, filename, url,
           target_path, total_bytes, downloaded_bytes, status,
           error, created_at, updated_at)
UNIQUE (game_id, installer_id, filename)
```

## ⚙️ Komendy IPC (Rust ↔ JS)

| Komenda | Parametry | Zwraca |
|---------|-----------|--------|
| `get_auth_status` | — | `{ loggedIn }` |
| `login` | — | — (okno OAuth) |
| `logout` | — | — |
| `sync_library` | — | `GameRow[]` |
| `get_games` | — | `GameRow[]` |
| `get_installers` | `{ gameId }` | `Installer[]` |
| `get_game_details` | `{ gameId, language? }` | `{ description, developers, publishers, genres, screenshots }` |
| `start_download` | `{ gameId, installerId }` | `DownloadRow[]` |
| `pause_download` | `{ downloadId }` | — |
| `cancel_download` | `{ downloadId }` | — |
| `get_downloads` | — | `DownloadRow[]` |
| `get_download_dir` | — | `string` |
| `pick_download_dir` | — | `string \| null` |
| `get_settings` | — | `{ downloadDir, language, theme }` |
| `set_language` | `{ language }` | — |
| `set_theme` | `{ theme }` | — |
| `detect_os` | — | `{ os, arch }` |

Zdarzenia: `auth:changed`, `library:page`, `library:synced`, `download:progress`, `download:state`.

## 🖥 Filtr platformy (Windows / Linux / macOS)

Aplikacja pozwala szybko zobaczyć, **co można zainstalować na jakim systemie**:

- **Biblioteka** — pasek filtrów `Wszystkie / Windows / Linux / macOS`. Kafle gier pokazują znaczniki
  platform (`Win` / `Lin` / `Mac`) na podstawie danych z API GOG (`worksOn` / `supportedOperatingSystems`).
  Filtr łączy się z wyszukiwarką. Przy starcie domyślnie zaznaczana jest platforma bieżącego systemu.
- **Modal gry → zakładka „Instalatory”** — pasek filtrów filtruje i sortuje listę instalatorów wg systemu
  (Windows → Linux → macOS), na podstawie pola `os` każdego instalatora.

Jeśli API biblioteki nie zwróci informacji o platformach, filtr platformy w siatce jest automatycznie
pomijany (pokazywane są wszystkie gry), a filtrowanie pozostaje dostępne w modalu gry.

## 🌍 Wielojęzyczność (PL / EN)

Interfejs obsługuje polski i angielski. Tłumaczenia znajdują się w `src/i18n.js` (słowniki `pl`/`en`),
a statyczne teksty w `index.html` są oznaczone atrybutami `data-i18n` / `data-i18n-placeholder` / `data-i18n-title`.
Wybór języka zapisywany jest w `settings.json` (komenda `set_language`); przy pierwszym uruchomieniu
wykrywany jest język systemu. Zmiana następuje natychmiast, bez restartu.

## 🎨 Motywy (jasny / ciemny)

Interfejs obsługuje dwa motywy przełączane w ekranie **Ustawienia → Motyw**. Motyw zmieniany jest
atrybutem `data-theme` (`dark` / `light`) na elemencie `<html>`, a palety zdefiniowane są jako zmienne
CSS (`:root` = motyw ciemny, `html[data-theme="light"]` = motyw jasny).

- **Ciemny (domyślny)** — bazuje na fioletowym `#800080` w wielu odcieniach (tła, akcenty, obramowania).
- **Jasny** — jasne tła z fioletowym akcentem `#800080`.

Wybór zapisywany jest w `settings.json` (komenda `set_theme`) i stosowany natychmiast, bez restartu.

## 🖥 Interfejs

Layout wzorowany na **GOG Galaxy 2.0**: pionowy **sidebar** (Biblioteka / Pobierania / Ustawienia +
sekcja konta), **topbar** z wyszukiwarką biblioteki, siatka okładek gier oraz modal szczegółów gry
z dużym banerem (hero). Ekran **Ustawienia** zawiera przełącznik języka, wybór motywu
(jasny/ciemny) oraz wybór folderu pobrań.

## 🐧 Wieloplatformowość

- **Ścieżki:** `dirs::data_dir()` / `dirs::download_dir()`
- **HTTP:** `reqwest` + `rustls` (bez OpenSSL)
- **Baza:** SQLite `bundled` (WAL, bezpieczne prepared statements)
- **Token:** chmod 600 na Unix
- **Testowane:** Ubuntu 24.04, Windows 10/11

## ⚠️ Ograniczenia

- Token GOG wygasa ~1h → 401 → ponowne logowanie
- Każdy plik instalatora enqueuowany osobno
- Ikona zastępcza — zastąp przed bundlowaniem

---

*Projekt edukacyjny, niepowiązany z GOG.com / CD Projekt.*
│   ├── capabilities/default.json # Uprawnienia IPC
│   ├── sql/schema.sql           # Schemat SQLite (games, downloads)
│   ├── icons/icon.png           # Ikona (256×256)
│   └── src/
│       ├── main.rs              # Punkt wejścia
│       ├── lib.rs               # init Tauri, stan
│       ├── error.rs             # Typy błędów
│       ├── db.rs                # SQLite ops
│       ├── auth.rs              # OAuth, token, okno logowania
│       ├── gog.rs               # Klient API GOG
│       ├── downloads.rs         # Menedżer pobierania
│       ├── platform.rs          # Wykrywanie systemu (Windows/Linux/macOS)
│       └── commands.rs          # Komendy IPC
└── scripts/gen_icon.py          # Generator ikony PNG
```