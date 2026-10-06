-- goglite — schemat bazy danych SQLite
-- Używany przez Database::open() do inicjalizacji

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS games (
    id             INTEGER PRIMARY KEY,   -- GOG product id
    title          TEXT NOT NULL,
    slug           TEXT DEFAULT '',
    image_url      TEXT,
    icon_url       TEXT,
    genre          TEXT,                  -- gatunek (pole `category` z getFilteredProducts)
    release_date   TEXT,                  -- data premiery (tekst ISO)
    developers     TEXT,                  -- producenci (JSON array, uzupełniane z v2 API)
    publishers     TEXT,                  -- wydawcy (JSON array, uzupełniane z v2 API)
    platforms      TEXT,                  -- platformy: JSON array ["windows","linux","macos"]
    last_synced_at INTEGER NOT NULL DEFAULT 0  -- timestamp (ms od epoch)
);

CREATE TABLE IF NOT EXISTS downloads (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id          INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    installer_id     TEXT NOT NULL DEFAULT '',      -- GOG download_id
    filename         TEXT NOT NULL,                  -- oryginalna nazwa pliku
    url              TEXT NOT NULL,                  -- pre-sygnowany link CDN
    target_path      TEXT NOT NULL,                  -- docelowa ścieżka na dysku
    total_bytes      INTEGER NOT NULL DEFAULT 0,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending','downloading','paused','completed','failed','cancelled')),
    error            TEXT,
    created_at       INTEGER NOT NULL DEFAULT 0,
    updated_at       INTEGER NOT NULL DEFAULT 0,

    UNIQUE(game_id, installer_id, filename)
);

CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
CREATE INDEX IF NOT EXISTS idx_downloads_game   ON downloads(game_id);
CREATE INDEX IF NOT EXISTS idx_games_title      ON games(title COLLATE NOCASE);