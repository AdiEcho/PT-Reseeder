use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloaderType {
    QBittorrent,
    Transmission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloaderRole {
    Source,
    Destination,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderConfig {
    pub name: String,
    pub dl_type: DownloaderType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: DownloaderRole,
    pub torrent_dir: Option<String>,
    pub default_save_path: Option<String>,
    pub skip_hash_check: bool,
    pub auto_start: bool,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderStatus {
    pub connected: bool,
    pub version: Option<String>,
    pub torrent_count: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AddTorrentOpts {
    pub torrent_data: Vec<u8>,
    pub save_path: String,
    pub skip_hash_check: bool,
    pub paused: bool,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub info_hash: String,
    pub name: String,
    pub save_path: String,
    pub progress: f64,
    pub state: String,
    pub total_size: u64,
    pub added_on: Option<i64>,
}
