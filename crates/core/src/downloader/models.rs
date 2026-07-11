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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloader_type_serializes_to_json_and_back() {
        let qt = DownloaderType::QBittorrent;
        let json = serde_json::to_string(&qt).unwrap();
        let deserialized: DownloaderType = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, DownloaderType::QBittorrent));

        let tr = DownloaderType::Transmission;
        let json = serde_json::to_string(&tr).unwrap();
        let deserialized: DownloaderType = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, DownloaderType::Transmission));
    }

    #[test]
    fn downloader_role_serializes_to_json_and_back() {
        for role in [DownloaderRole::Source, DownloaderRole::Destination, DownloaderRole::Both] {
            let json = serde_json::to_string(&role).unwrap();
            let _deserialized: DownloaderRole = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn downloader_config_serializes_with_all_fields() {
        let config = DownloaderConfig {
            name: "my-qb".to_string(),
            dl_type: DownloaderType::QBittorrent,
            host: "192.168.1.100".to_string(),
            port: 8080,
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            role: DownloaderRole::Both,
            torrent_dir: Some("/torrents".to_string()),
            default_save_path: Some("/downloads".to_string()),
            skip_hash_check: true,
            auto_start: false,
            tag: Some("PT-Reseeder".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DownloaderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-qb");
        assert_eq!(deserialized.port, 8080);
        assert!(deserialized.skip_hash_check);
        assert!(!deserialized.auto_start);
        assert_eq!(deserialized.username, Some("admin".to_string()));
    }

    #[test]
    fn downloader_config_serializes_with_optional_none() {
        let config = DownloaderConfig {
            name: "bare".to_string(),
            dl_type: DownloaderType::Transmission,
            host: "localhost".to_string(),
            port: 9091,
            username: None,
            password: None,
            role: DownloaderRole::Source,
            torrent_dir: None,
            default_save_path: None,
            skip_hash_check: false,
            auto_start: true,
            tag: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DownloaderConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.username.is_none());
        assert!(deserialized.password.is_none());
        assert!(deserialized.tag.is_none());
    }

    #[test]
    fn downloader_status_serializes_to_json_and_back() {
        let status = DownloaderStatus {
            connected: true,
            version: Some("v4.6.2".to_string()),
            torrent_count: Some(150),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: DownloaderStatus = serde_json::from_str(&json).unwrap();
        assert!(deserialized.connected);
        assert_eq!(deserialized.version, Some("v4.6.2".to_string()));
        assert_eq!(deserialized.torrent_count, Some(150));
    }

    #[test]
    fn torrent_info_serializes_to_json_and_back() {
        let info = TorrentInfo {
            info_hash: "abcdef1234567890".to_string(),
            name: "test.mkv".to_string(),
            save_path: "/downloads".to_string(),
            progress: 0.75,
            state: "downloading".to_string(),
            total_size: 1073741824,
            added_on: Some(1700000000),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: TorrentInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.info_hash, "abcdef1234567890");
        assert_eq!(deserialized.progress, 0.75);
        assert_eq!(deserialized.total_size, 1073741824);
    }

    #[test]
    fn add_torrent_opts_can_be_constructed() {
        let opts = AddTorrentOpts {
            torrent_data: vec![0x01, 0x02, 0x03],
            save_path: "/downloads".to_string(),
            skip_hash_check: true,
            paused: false,
            tag: Some("test".to_string()),
        };
        assert_eq!(opts.torrent_data.len(), 3);
        assert!(opts.skip_hash_check);
        assert!(!opts.paused);
    }
}
