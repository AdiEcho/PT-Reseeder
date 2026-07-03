CREATE TABLE folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    scan_mode TEXT NOT NULL DEFAULT 'local',
    downloader_id INTEGER REFERENCES downloaders(id),
    enabled INTEGER NOT NULL DEFAULT 1,
    last_scanned_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    task_type TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    cron_expression TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    downloader_pair_id INTEGER REFERENCES downloader_pairs(id),
    last_run_at TEXT,
    next_run_at TEXT,
    run_count INTEGER DEFAULT 0,
    config_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE task_folders (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, folder_id)
);

CREATE TABLE task_sites (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, site_id)
);

CREATE TABLE task_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    matched_count INTEGER DEFAULT 0,
    succeeded_count INTEGER DEFAULT 0,
    failed_count INTEGER DEFAULT 0,
    duration_ms INTEGER,
    log_text TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_task_logs_task ON task_logs(task_id, created_at);

CREATE TABLE repost_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_site_id INTEGER NOT NULL REFERENCES sites(id),
    source_torrent_id TEXT NOT NULL,
    target_site_id INTEGER NOT NULL REFERENCES sites(id),
    raw_info_json TEXT NOT NULL,
    adapted_info_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    review_notes TEXT,
    submitted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_repost_queue_status ON repost_queue(status);
