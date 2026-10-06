// goglite — moduł bazy danych SQLite
// Operacje na tabelach `games` i `downloads` z użyciem prepared statements.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::Serialize;

use crate::error::Result;

// ====================== STRUKTURY ======================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub image_url: Option<String>,
    pub icon_url: Option<String>,
    /// Gatunek z GOG (pole `category`, np. „Role-playing”).
    pub genre: Option<String>,
    /// Data premiery (tekst ISO, np. „2015-02-10”).
    pub release_date: Option<String>,
    /// Producenci (cache z v2 API).
    pub developers: Vec<String>,
    /// Wydawcy (cache z v2 API).
    pub publishers: Vec<String>,
    /// Platformy dostępne dla gry: `windows` / `linux` / `macos`.
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRow {
    pub id: i64,
    pub game_id: i64,
    pub game_title: String,
    pub installer_id: String,
    pub filename: String,
    pub status: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub progress: f32,
    pub error: Option<String>,
}

// ====================== POMOCNICZE ======================

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn sanitize_filename(name: &str) -> String {
    PathBuf::from(name).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string())
}

/// Parsuje zapisany w bazie JSON `["a","b"]` do `Vec<String>` (odporny na braki/błędy).
fn parse_names(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

/// Lekkie migracje: dodaje brakujące kolumny do istniejącej tabeli `games`
/// (SQLite nie obsługuje `ADD COLUMN IF NOT EXISTS`).
fn migrate(conn: &rusqlite::Connection) -> Result<()> {
    let existing: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(games)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for (name, ty) in [
        ("genre", "TEXT"),
        ("release_date", "TEXT"),
        ("developers", "TEXT"),
        ("publishers", "TEXT"),
        ("platforms", "TEXT"),
    ] {
        if !existing.contains(name) {
            conn.execute_batch(&format!("ALTER TABLE games ADD COLUMN {name} {ty}"))?;
        }
    }
    Ok(())
}

// ====================== BAZA ======================

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).ok(); }
        let conn = rusqlite::Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(include_str!("../sql/schema.sql"))?;
        migrate(&conn)?;
        log::info!("Baza otwarta: {}", path.display());
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Tworzy bazę w pamięci (tylko do testów).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(include_str!("../sql/schema.sql"))?;
        migrate(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    // —————————————— GAMES ——————————————

    pub fn upsert_games(&self, games: &[GameRow]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "INSERT INTO games (id, title, slug, image_url, icon_url, genre, release_date, platforms, last_synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title, slug = excluded.slug,
               image_url = excluded.image_url, icon_url = excluded.icon_url,
               genre = excluded.genre, release_date = excluded.release_date,
               platforms = excluded.platforms,
               last_synced_at = excluded.last_synced_at",
        )?;
        let now = now_ms();
        let mut n = 0usize;
        for g in games {
            n += stmt.execute(params![
                g.id, g.title, g.slug, g.image_url, g.icon_url,
                g.genre, g.release_date,
                serde_json::to_string(&g.platforms).unwrap_or_else(|_| "[]".into()),
                now
            ])?;
        }
        Ok(n)
    }

    pub fn list_games(&self) -> Result<Vec<GameRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, slug, image_url, icon_url, genre, release_date, developers, publishers, platforms
             FROM games ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(GameRow {
                id: r.get(0)?, title: r.get(1)?, slug: r.get(2)?,
                image_url: r.get(3)?, icon_url: r.get(4)?,
                genre: r.get(5)?, release_date: r.get(6)?,
                developers: parse_names(r.get(7)?),
                publishers: parse_names(r.get(8)?),
                platforms: parse_names(r.get(9)?),
            })
        })?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    /// Dopisuje producentów/wydawców (poznanych dopiero po zapytaniu do v2 API)
    /// do wiersza gry — zachowuje je przy kolejnych synchronizacjach.
    pub fn set_game_meta(&self, game_id: i64, developers: &[String], publishers: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE games SET developers = ?2, publishers = ?3 WHERE id = ?1",
            params![
                game_id,
                serde_json::to_string(developers).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(publishers).unwrap_or_else(|_| "[]".into()),
            ],
        )?;
        Ok(())
    }

    // —————————————— DOWNLOADS ——————————————

    pub fn upsert_download(
        &self, game_id: i64, installer_id: &str, filename: &str,
        url: &str, target: &Path, total_bytes: u64,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let safe_name = sanitize_filename(filename);
        let target_str = target.to_string_lossy().to_string();

        let (was_active, existing_bytes): (bool, u64) = conn
            .query_row(
                "SELECT status, downloaded_bytes FROM downloads
                 WHERE game_id = ?1 AND installer_id = ?2 AND filename = ?3",
                params![game_id, installer_id, safe_name],
                |r| {
                    let s: String = r.get(0)?;
                    let b: i64 = r.get(1)?;
                    Ok((s == "paused" || s == "downloading" || s == "pending", b.max(0) as u64))
                },
            )
            .ok()
            .unwrap_or((false, 0));

        let offset = if was_active && target.exists() { existing_bytes } else { 0 };
        let now = now_ms();

        // UWAGA: `last_insert_rowid()` NIE jest aktualizowane przy trafieniu w konflikt
        // (gałąź ON CONFLICT DO UPDATE nic nie wstawia), więc po restarcie aplikacji
        // zwracałoby 0 i pobieranie „nie ruszało” (aktualizowany był wiersz o id 0).
        // `RETURNING id` zawsze zwraca właściwy identyfikator wiersza.
        let id = conn.query_row(
            "INSERT INTO downloads (game_id, installer_id, filename, url, target_path,
                    total_bytes, downloaded_bytes, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'pending',?8,?8)
             ON CONFLICT(game_id, installer_id, filename) DO UPDATE SET
               url=excluded.url, target_path=excluded.target_path,
               total_bytes=excluded.total_bytes,
               downloaded_bytes=excluded.downloaded_bytes,
               status='pending', error=NULL, updated_at=excluded.updated_at
             RETURNING id",
            params![game_id, installer_id, safe_name, url, target_str,
                    total_bytes, offset, now],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Aktualizuje postęp pobierania. `total` (jeśli > 0) nadpisuje rozmiar docelowy
    /// ustalony wcześniej z API GOG — realny `Content-Length` z CDN bywa dokładniejszy.
    pub fn set_download_progress(
        &self, id: i64, downloaded: u64, total: u64, status: &str, error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_bytes=?1,
                total_bytes = CASE WHEN ?2 > 0 THEN ?2 ELSE total_bytes END,
                status=?3, error=?4, updated_at=?5 WHERE id=?6",
            params![downloaded, total, status, error, now_ms(), id],
        )?;
        Ok(())
    }

    pub fn set_download_status(&self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status=?1, error=?2, updated_at=?3 WHERE id=?4",
            params![status, error, now_ms(), id],
        )?;
        Ok(())
    }

    /// Zwraca bieżący status pobierania (lub `None`, gdy wiersz nie istnieje).
    pub fn get_download_status(&self, id: i64) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT status FROM downloads WHERE id = ?1", params![id], |r| r.get(0))
            .ok()
    }

    pub fn mark_incomplete_paused(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE downloads SET status='paused', updated_at=?1 WHERE status IN ('downloading','pending')",
            params![now_ms()],
        )?;
        if n > 0 { log::info!("Oznaczono {n} pobierań jako wstrzymane"); }
        Ok(n as u64)
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT d.id, d.game_id, COALESCE(g.title,''), d.installer_id,
                    d.filename, d.status, d.total_bytes, d.downloaded_bytes, d.error
             FROM downloads d LEFT JOIN games g ON g.id = d.game_id
             ORDER BY d.updated_at DESC, d.id DESC LIMIT 500",
        )?;
        let rows = stmt.query_map([], |r| {
            let total = r.get::<_, i64>(6)?.max(0) as u64;
            let done = r.get::<_, i64>(7)?.max(0) as u64;
            let status: String = r.get(5)?;
            Ok(DownloadRow {
                id: r.get(0)?, game_id: r.get(1)?, game_title: r.get(2)?,
                installer_id: r.get(3)?, filename: r.get(4)?, status,
                total_bytes: total, downloaded_bytes: done,
                progress: if total > 0 { (done as f32 / total as f32) * 100.0 } else { -1.0 },
                error: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
}

// ====================== TESTY ======================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_game(id: i64) -> GameRow {
        GameRow {
            id,
            title: "Gra".into(),
            slug: String::new(),
            image_url: None,
            icon_url: None,
            genre: None,
            release_date: None,
            developers: Vec::new(),
            publishers: Vec::new(),
            platforms: Vec::new(),
        }
    }

    /// Regresja: `upsert_download` musi zwracać właściwe `id` także wtedy, gdy wiersz
    /// już istnieje (gałąź ON CONFLICT DO UPDATE). Wcześniej używano `last_insert_rowid()`,
    /// które po restarcie aplikacji zwracało `0` i pobieranie „nie ruszało”.
    #[test]
    fn upsert_download_zwraca_id_takze_przy_konflikcie() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_games(&[sample_game(5)]).unwrap();

        let target = PathBuf::from("/tmp/goglite-test-nieistnieje/a.exe");
        let first = db
            .upsert_download(5, "installer_windows_en", "a.exe", "https://x/a.exe", &target, 1024)
            .unwrap();
        // Ponowny „start” tego samego pliku (np. po zmianie folderu) trafia w konflikt.
        let second = db
            .upsert_download(5, "installer_windows_en", "a.exe", "https://x/a.exe", &target, 1024)
            .unwrap();

        assert!(first > 0, "id nowego wiersza musi być dodatnie");
        assert_eq!(first, second, "id istniejącego wiersza musi być stabilne");

        let rows = db.list_downloads().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, first);
    }

    #[test]
    fn mark_incomplete_paused_oznacza_aktywne_pobrania() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_games(&[sample_game(7)]).unwrap();

        let target = PathBuf::from("/tmp/goglite-test-nieistnieje/b.exe");
        let id = db
            .upsert_download(7, "i", "b.exe", "https://x/b.exe", &target, 10)
            .unwrap();
        db.set_download_status(id, "downloading", None).unwrap();

        assert_eq!(db.mark_incomplete_paused().unwrap(), 1);
        assert_eq!(db.list_downloads().unwrap()[0].status, "paused");
    }

    #[test]
    fn migracja_dodaje_nowe_kolumny_do_starej_bazy() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // „Stara” tabela bez kolumn genre/release_date/developers/publishers.
        conn.execute_batch(
            "CREATE TABLE games (
                id INTEGER PRIMARY KEY, title TEXT NOT NULL, slug TEXT DEFAULT '',
                image_url TEXT, icon_url TEXT, last_synced_at INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('games')
                 WHERE name IN ('genre','release_date','developers','publishers')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 4, "wszystkie nowe kolumny muszą istnieć po migracji");
    }

    #[test]
    fn set_game_meta_zapisuje_metadane_i_przetrwa_resync() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_games(&[sample_game(9)]).unwrap();
        db.set_game_meta(9, &["Burst".into()], &["Interplay".into()]).unwrap();

        let g = &db.list_games().unwrap()[0];
        assert_eq!(g.developers, vec!["Burst"]);
        assert_eq!(g.publishers, vec!["Interplay"]);

        // Ponowna synchronizacja nie kasuje cache'owanych producentów/wydawców.
        db.upsert_games(&[sample_game(9)]).unwrap();
        let g = &db.list_games().unwrap()[0];
        assert_eq!(g.developers, vec!["Burst"]);
        assert_eq!(g.publishers, vec!["Interplay"]);
    }
}

