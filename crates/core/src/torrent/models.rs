use serde::{Deserialize, Serialize};

/// Parsed torrent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentMeta {
    pub info_hash: String,
    pub pieces_hash: String,
    pub name: String,
    pub total_size: u64,
    pub files: Vec<TorrentFile>,
    pub announce: Option<String>,
    pub announce_list: Vec<Vec<String>>,
    pub piece_length: u64,
    pub pieces_count: u64,
}

/// Individual file within a torrent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: u64,
}
