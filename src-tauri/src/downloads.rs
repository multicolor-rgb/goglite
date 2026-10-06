// goglite — menedżer pobierania
// Obsługuje pobieranie plików z możliwością wstrzymania i anulowania.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::RANGE;
use tauri::{AppHandle, Emitter};

use crate::db::Database;
use crate::error::{AppError, Result};

/// Flagi sterujące zadaniem pobierania.
#[derive(Clone, Default)]
struct TaskFlags {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

/// Menedżer uruchamiający zadania pobierania.
#[derive(Clone)]
pub struct DownloadManager {
    pub http: reqwest::Client,
    pub db: Database,
    pub app: AppHandle,
    pub dir: PathBuf,
    tasks: Arc<Mutex<HashMap<i64, TaskFlags>>>,
}

impl DownloadManager {
    pub fn new(http: reqwest::Client, db: Database, app: AppHandle, dir: PathBuf) -> Self {
        Self { http, db, app, dir, tasks: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn start(&self, id: i64, url: String, target: PathBuf, auth_token: String) -> Result<()> {
        let flags = TaskFlags::default();
        {
            let mut tasks = self.tasks.lock().unwrap();
            if let Some(existing) = tasks.get(&id) {
                let stopping = existing.paused.load(Ordering::SeqCst)
                    || existing.cancelled.load(Ordering::SeqCst);
                if !stopping {
                    log::info!("Pobieranie {} już trwa.", id);
                    return Ok(());
                }
                // Stare zadanie właśnie się zatrzymuje — nie blokujemy wznowienia.
            }
            tasks.insert(id, flags.clone());
        }
        let my_flags = flags.clone();
        let mgr = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = mgr.run(id, &url, &target, &auth_token, flags).await {
                log::error!("Pobieranie {} nieudane: {}", id, e);
                let _ = mgr.db.set_download_status(id, "failed", Some(&e.to_string()));
                let _ = mgr.emit_state(id, "failed");
            }
            // Usuwamy wpis tylko, gdy nadal należy do TEGO zadania — inaczej
            // skasowalibyśmy wpis nowszego wznowienia, które je zastąpiło.
            let mut tasks = mgr.tasks.lock().unwrap();
            let mine = tasks.get(&id).map(|f| Arc::ptr_eq(&f.paused, &my_flags.paused)).unwrap_or(false);
            if mine { tasks.remove(&id); }
        });
        Ok(())
    }

    pub fn pause(&self, id: i64) {
        if let Some(f) = self.tasks.lock().unwrap().get(&id) {
            f.paused.store(true, Ordering::SeqCst);
            return;
        }
        // Brak aktywnego zadania (np. wiersz „pending” lub już zatrzymany) —
        // oznaczamy wstrzymanie bezpośrednio w bazie, żeby UI się zaktualizowało.
        if let Some(status) = self.db.get_download_status(id) {
            if matches!(status.as_str(), "pending" | "downloading") {
                let _ = self.db.set_download_status(id, "paused", None);
                let _ = self.emit_state(id, "paused");
            }
        }
    }

    pub fn cancel(&self, id: i64) {
        if let Some(f) = self.tasks.lock().unwrap().get(&id) {
            f.cancelled.store(true, Ordering::SeqCst);
            return;
        }
        // Brak aktywnego zadania — anulujemy wprost w bazie. Dzięki temu działa też
        // dla pobrań wstrzymanych/nieudanych, które nie mają już wpisu w `tasks`.
        if let Some(status) = self.db.get_download_status(id) {
            if status != "completed" && status != "cancelled" {
                let _ = self.db.set_download_status(id, "cancelled", None);
                let _ = self.emit_state(id, "cancelled");
            }
        }
    }
// ---------- wewnętrzna pętla ----------

    async fn run(&self, id: i64, url: &str, target: &Path, auth_token: &str, flags: TaskFlags) -> Result<()> {
        self.db.set_download_status(id, "downloading", None)?;

        let offset = if target.exists() {
            let meta = target.metadata().map(|m| m.len()).unwrap_or(0);
            let from_db: u64 = self.db.list_downloads()
                .ok().and_then(|list| list.into_iter().find(|r| r.id == id))
                .map(|r| r.downloaded_bytes).unwrap_or(0);
            meta.min(from_db)
        } else { 0 };

        let mut file = std::fs::OpenOptions::new()
            .create(true).write(true).truncate(offset == 0).open(target)?;
        if offset > 0 {
            use std::io::Seek;
            file.seek(std::io::SeekFrom::Start(offset))?;
        }

        let mut req = self.http.get(url)
            .header("User-Agent", crate::USER_AGENT)
            .bearer_auth(auth_token);
        if offset > 0 { req = req.header(RANGE, format!("bytes={offset}-")); }
        let resp = req.send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED { return Err(AppError::AuthRequired); }
        if !status.is_success() && !status.is_redirection() { return Err(AppError::GogStatus(status)); }

        // Zabezpieczenie: JSON/HTML to nie plik instalatora (tak zachowuje się np.
        // nierozwiązany `downlink` API GOG). Bez tego zapisywalibyśmy ~459 B metadanych
        // i oznaczali pobranie jako udane.
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("json") || content_type.contains("text/html") {
            return Err(AppError::Msg(format!(
                "Serwer zwrócił '{content_type}' zamiast pliku instalatora"
            )));
        }

        let start = if status == reqwest::StatusCode::PARTIAL_CONTENT { offset } else { 0 };
        let total = resp.content_length().unwrap_or(0).saturating_add(start);
        let mut downloaded = start as u64;
        let mut last_emit = Instant::now();

        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if flags.cancelled.load(Ordering::SeqCst) { break; }
            if flags.paused.load(Ordering::SeqCst) { break; }
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(200) {
                let _ = self.db.set_download_progress(id, downloaded, total, "downloading", None);
                let _ = self.emit_progress(id, downloaded, total);
                last_emit = Instant::now();
            }
        }
        file.flush()?;

        let paused = flags.paused.load(Ordering::SeqCst);
        let cancelled = flags.cancelled.load(Ordering::SeqCst);
        // Sukces tylko wtedy, gdy pobrano całość (przy nieznanym rozmiarze — ufamy strumieniowi).
        let complete = total == 0 || downloaded >= total;

        if paused {
            let _ = self.db.set_download_progress(id, downloaded, total, "paused", None);
            let _ = self.emit_state(id, "paused");
        } else if cancelled {
            let _ = self.db.set_download_progress(id, downloaded, total, "cancelled", None);
            let _ = self.emit_state(id, "cancelled");
        } else if complete {
            let _ = self.db.set_download_progress(id, downloaded, total, "completed", None);
            let _ = self.emit_state(id, "completed");
        } else {
            // Strumień skończył się przed pobraniem całości — to błąd, nie sukces.
            let msg = format!("Pobrano {downloaded} z {total} bajtów");
            let _ = self.db.set_download_progress(id, downloaded, total, "failed", Some(&msg));
            let _ = self.emit_state(id, "failed");
        }
        Ok(())
    }

    fn emit_progress(&self, id: i64, downloaded: u64, total: u64) -> Result<()> {
        let progress = if total > 0 { (downloaded as f32 / total as f32) * 100.0 } else { -1.0 };
        let _ = self.app.emit("download:progress", serde_json::json!({
            "id": id, "downloadedBytes": downloaded, "totalBytes": total,
            "progress": progress, "status": "downloading"
        }));
        Ok(())
    }

    fn emit_state(&self, id: i64, status: &str) -> Result<()> {
        let _ = self.app.emit("download:state", serde_json::json!({"id": id, "status": status}));
        Ok(())
    }
}