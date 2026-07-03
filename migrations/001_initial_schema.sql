-- 001_initial_schema.sql

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    kdf_salt BLOB NOT NULL,
    wrapped_dek BLOB NOT NULL,
    dek_nonce BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at TEXT
);

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_sessions_expiry ON sessions(expires_at);

CREATE TABLE app_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    api_url TEXT,
    adapter_type TEXT NOT NULL DEFAULT 'nexusphp',
    auth_type TEXT NOT NULL DEFAULT 'cookie',
    encrypted_cookie BLOB,
    cookie_nonce BLOB,
    encrypted_passkey BLOB,
    passkey_nonce BLOB,
    encrypted_token BLOB,
    token_nonce BLOB,
    rate_limit_interval_ms INTEGER DEFAULT 5000,
    rate_limit_burst INTEGER DEFAULT 1,
    download_interval_ms INTEGER DEFAULT 5000,
    probe_status TEXT NOT NULL DEFAULT 'unknown',
    probe_detail_json TEXT,
    probed_at TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE user_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    uploaded INTEGER,
    downloaded INTEGER,
    ratio REAL,
    bonus REAL,
    user_class TEXT,
    seeding_count INTEGER,
    leeching_count INTEGER,
    seeding_size INTEGER,
    upload_time_seconds INTEGER,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_user_stats_site_time ON user_stats(site_id, fetched_at);

CREATE TABLE downloaders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    dl_type TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    encrypted_username BLOB,
    username_nonce BLOB,
    encrypted_password BLOB,
    password_nonce BLOB,
    role TEXT NOT NULL DEFAULT 'both',
    torrent_dir TEXT,
    default_save_path TEXT,
    skip_hash_check INTEGER DEFAULT 1,
    auto_start INTEGER DEFAULT 1,
    tag TEXT DEFAULT 'PT-Reseeder',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE downloader_pairs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    source_id INTEGER NOT NULL REFERENCES downloaders(id),
    destination_id INTEGER NOT NULL REFERENCES downloaders(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
