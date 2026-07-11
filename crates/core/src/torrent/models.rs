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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torrent_meta_serializes_to_json_and_back() {
        let meta = TorrentMeta {
            info_hash: "abc123".to_string(),
            pieces_hash: "def456".to_string(),
            name: "test.mkv".to_string(),
            total_size: 1024,
            files: vec![TorrentFile {
                path: vec!["test.mkv".to_string()],
                length: 1024,
            }],
            announce: Some("http://tracker/announce".to_string()),
            announce_list: vec![vec!["http://tracker/announce".to_string()]],
            piece_length: 256,
            pieces_count: 4,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: TorrentMeta = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.info_hash, "abc123");
        assert_eq!(deserialized.pieces_hash, "def456");
        assert_eq!(deserialized.name, "test.mkv");
        assert_eq!(deserialized.total_size, 1024);
        assert_eq!(deserialized.files.len(), 1);
        assert_eq!(deserialized.piece_length, 256);
        assert_eq!(deserialized.pieces_count, 4);
    }

    #[test]
    fn torrent_file_serializes_to_json_and_back() {
        let file = TorrentFile {
            path: vec!["dir".to_string(), "sub".to_string(), "file.txt".to_string()],
            length: 999,
        };

        let json = serde_json::to_string(&file).unwrap();
        let deserialized: TorrentFile = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.path, vec!["dir", "sub", "file.txt"]);
        assert_eq!(deserialized.length, 999);
    }

    #[test]
    fn torrent_meta_with_no_announce_serializes() {
        let meta = TorrentMeta {
            info_hash: "h".to_string(),
            pieces_hash: "p".to_string(),
            name: "n".to_string(),
            total_size: 0,
            files: vec![],
            announce: None,
            announce_list: vec![],
            piece_length: 0,
            pieces_count: 0,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: TorrentMeta = serde_json::from_str(&json).unwrap();
        assert!(deserialized.announce.is_none());
        assert!(deserialized.announce_list.is_empty());
        assert!(deserialized.files.is_empty());
    }
}
