use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub kdf_salt: Vec<u8>,
    pub wrapped_dek: Vec<u8>,
    pub dek_nonce: Vec<u8>,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token_hash: Vec<u8>,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppConfigEntry {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SiteRow {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: String,
    pub auth_type: String,
    pub encrypted_cookie: Option<Vec<u8>>,
    pub cookie_nonce: Option<Vec<u8>>,
    pub encrypted_passkey: Option<Vec<u8>>,
    pub passkey_nonce: Option<Vec<u8>>,
    pub encrypted_token: Option<Vec<u8>>,
    pub token_nonce: Option<Vec<u8>>,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
    pub download_interval_ms: Option<i64>,
    pub probe_status: String,
    pub probe_detail_json: Option<String>,
    pub probed_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserStatRecord {
    pub id: i64,
    pub site_id: i64,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub ratio: Option<f64>,
    pub bonus: Option<f64>,
    pub user_class: Option<String>,
    pub seeding_count: Option<i64>,
    pub leeching_count: Option<i64>,
    pub seeding_size: Option<i64>,
    pub upload_time_seconds: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DownloaderRow {
    pub id: i64,
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    pub encrypted_username: Option<Vec<u8>>,
    pub username_nonce: Option<Vec<u8>>,
    pub encrypted_password: Option<Vec<u8>>,
    pub password_nonce: Option<Vec<u8>>,
    pub role: String,
    pub torrent_dir: Option<String>,
    pub default_save_path: Option<String>,
    pub skip_hash_check: Option<bool>,
    pub auto_start: Option<bool>,
    pub tag: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PiecesCacheEntry {
    pub id: i64,
    pub pieces_hash: String,
    pub info_hash: String,
    pub torrent_name: Option<String>,
    pub file_path: Option<String>,
    pub total_size: Option<i64>,
    pub announce_url: Option<String>,
    pub cached_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReseedHistoryEntry {
    pub id: i64,
    pub pieces_hash: String,
    pub site_id: i64,
    pub torrent_id: Option<i64>,
    pub info_hash: Option<String>,
    pub status: String,
    pub error_reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FolderRow {
    pub id: i64,
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    pub enabled: bool,
    pub last_scanned_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: i64,
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub status: String,
    pub destination_downloader_id: Option<i64>,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: Option<i64>,
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskLog {
    pub id: i64,
    pub task_id: i64,
    pub status: String,
    pub matched_count: Option<i64>,
    pub succeeded_count: Option<i64>,
    pub failed_count: Option<i64>,
    pub duration_ms: Option<i64>,
    pub log_text: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RepostQueueEntry {
    pub id: i64,
    pub source_site_id: i64,
    pub source_torrent_id: String,
    pub target_site_id: i64,
    pub raw_info_json: String,
    pub adapted_info_json: Option<String>,
    pub status: String,
    pub review_notes: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
}
