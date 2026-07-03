CREATE TABLE pieces_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pieces_hash TEXT NOT NULL,
    info_hash TEXT NOT NULL,
    torrent_name TEXT,
    file_path TEXT,
    total_size INTEGER,
    announce_url TEXT,
    cached_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX idx_pieces_cache_info ON pieces_cache(info_hash);
CREATE INDEX idx_pieces_cache_hash ON pieces_cache(pieces_hash);

CREATE TABLE reseed_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pieces_hash TEXT NOT NULL,
    site_id INTEGER NOT NULL REFERENCES sites(id),
    torrent_id INTEGER,
    info_hash TEXT,
    status TEXT NOT NULL DEFAULT 'success',
    error_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_reseed_history_ph_site ON reseed_history(pieces_hash, site_id);
CREATE INDEX idx_reseed_history_time ON reseed_history(created_at);
