use serde::{Deserialize, Serialize};

/// Site identifier (database primary key)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SiteId(pub i64);

impl From<i64> for SiteId {
    fn from(id: i64) -> Self {
        SiteId(id)
    }
}

/// User statistics from a PT site
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    pub site_id: SiteId,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub ratio: Option<f64>,
    pub bonus: Option<f64>,
    pub user_class: Option<String>,
    pub seeding_count: Option<i64>,
    pub leeching_count: Option<i64>,
    pub seeding_size: Option<i64>,
    pub upload_time_seconds: Option<i64>,
    pub fetched_at: Option<String>,
}

/// Torrent search result from a site
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSearchResult {
    pub id: i64,
    pub name: String,
    pub size: u64,
    pub seeders: u32,
    pub leechers: u32,
    pub info_hash: Option<String>,
}

/// Raw torrent info extracted from a source site (for repost)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTorrentInfo {
    pub name: String,
    pub small_descr: String,
    pub descr: String,
    pub imdb_url: Option<String>,
    pub douban_url: Option<String>,
    pub mediainfo: Option<String>,
    pub images: Vec<String>,
    pub torrent_type: String,
    pub region: String,
    pub resolution: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub medium: String,
    pub source_site: String,
    pub source_url: String,
    pub torrent_file_data: Option<Vec<u8>>,
}

/// Adapted torrent info for target site submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptedTorrentInfo {
    pub name: String,
    pub small_descr: String,
    pub descr: String,
    pub imdb_url: Option<String>,
    pub douban_url: Option<String>,
    pub mediainfo: Option<String>,
    pub images: Vec<String>,
    pub category_id: Option<i64>,
    pub source_id: Option<i64>,
    pub codec_id: Option<i64>,
    pub resolution_id: Option<i64>,
    pub torrent_file_data: Option<Vec<u8>>,
    pub target_site: String,
}

/// TOML-based site definition (loaded from files)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDefinition {
    pub site: SiteDefinitionCore,
    pub user_info: Option<UserInfoSelectors>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDefinitionCore {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter: String,
    pub rate_limit_interval_ms: Option<u64>,
    pub rate_limit_burst: Option<u32>,
    pub batch_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoSelectors {
    pub profile_url_template: Option<String>,
    pub uid_selector: Option<String>,
    pub uploaded_selector: Option<String>,
    pub downloaded_selector: Option<String>,
    pub ratio_selector: Option<String>,
    pub bonus_selector: Option<String>,
    pub user_class_selector: Option<String>,
    pub seeding_count_selector: Option<String>,
    pub leeching_count_selector: Option<String>,
    pub seeding_size_selector: Option<String>,
    pub upload_time_selector: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_id_from_i64() {
        let id: SiteId = 42.into();
        assert_eq!(id.0, 42);
    }

    #[test]
    fn site_id_equality() {
        let a = SiteId(1);
        let b = SiteId(1);
        let c = SiteId(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn site_id_can_be_used_as_hashmap_key() {
        let mut map = std::collections::HashMap::new();
        map.insert(SiteId(1), "site1");
        map.insert(SiteId(2), "site2");
        assert_eq!(map.get(&SiteId(1)), Some(&"site1"));
        assert_eq!(map.get(&SiteId(3)), None);
    }

    #[test]
    fn site_id_serializes_to_json_and_back() {
        let id = SiteId(99);
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: SiteId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, id);
    }

    #[test]
    fn user_stats_serializes_with_all_none_fields() {
        let stats = UserStats {
            site_id: SiteId(1),
            uploaded: None,
            downloaded: None,
            ratio: None,
            bonus: None,
            user_class: None,
            seeding_count: None,
            leeching_count: None,
            seeding_size: None,
            upload_time_seconds: None,
            fetched_at: None,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: UserStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.site_id, SiteId(1));
        assert!(deserialized.uploaded.is_none());
    }

    #[test]
    fn user_stats_serializes_with_populated_fields() {
        let stats = UserStats {
            site_id: SiteId(5),
            uploaded: Some(1_000_000),
            downloaded: Some(500_000),
            ratio: Some(2.0),
            bonus: Some(1234.5),
            user_class: Some("VIP".to_string()),
            seeding_count: Some(50),
            leeching_count: Some(2),
            seeding_size: Some(500_000_000),
            upload_time_seconds: Some(86400),
            fetched_at: Some("2024-01-01 00:00:00".to_string()),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: UserStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ratio, Some(2.0));
        assert_eq!(deserialized.user_class, Some("VIP".to_string()));
    }

    #[test]
    fn torrent_search_result_serializes() {
        let result = TorrentSearchResult {
            id: 12345,
            name: "Movie.2024.BluRay.mkv".to_string(),
            size: 4_000_000_000,
            seeders: 100,
            leechers: 5,
            info_hash: Some("abcdef1234567890".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TorrentSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 12345);
        assert_eq!(deserialized.seeders, 100);
    }

    #[test]
    fn site_definition_deserializes_from_toml() {
        let toml_str = r#"
[site]
id = "test"
name = "TestSite"
url = "https://test.example.com"
adapter = "nexusphp"
rate_limit_interval_ms = 5000
rate_limit_burst = 1
batch_size = 500
"#;
        let def: SiteDefinition = toml_lib::from_str(toml_str).unwrap();
        assert_eq!(def.site.id, "test");
        assert_eq!(def.site.name, "TestSite");
        assert_eq!(def.site.url, "https://test.example.com");
        assert_eq!(def.site.adapter, "nexusphp");
        assert_eq!(def.site.rate_limit_interval_ms, Some(5000));
        assert_eq!(def.site.rate_limit_burst, Some(1));
        assert_eq!(def.site.batch_size, Some(500));
        assert!(def.user_info.is_none());
    }

    #[test]
    fn site_definition_deserializes_with_user_info() {
        let toml_str = r#"
[site]
id = "full"
name = "FullSite"
url = "https://full.example.com"
adapter = "nexusphp"

[user_info]
profile_url_template = "/user/{uid}"
uid_selector = "a.username"
uploaded_selector = ".uploaded"
downloaded_selector = ".downloaded"
"#;
        let def: SiteDefinition = toml_lib::from_str(toml_str).unwrap();
        assert!(def.user_info.is_some());
        let ui = def.user_info.unwrap();
        assert_eq!(ui.profile_url_template, Some("/user/{uid}".to_string()));
        assert_eq!(ui.uid_selector, Some("a.username".to_string()));
    }

    #[test]
    fn raw_torrent_info_serializes() {
        let info = RawTorrentInfo {
            name: "Movie".to_string(),
            small_descr: "desc".to_string(),
            descr: "full desc".to_string(),
            imdb_url: Some("https://imdb.com/tt123".to_string()),
            douban_url: None,
            mediainfo: None,
            images: vec!["https://img.com/1.jpg".to_string()],
            torrent_type: "movie".to_string(),
            region: "CN".to_string(),
            resolution: "1080p".to_string(),
            video_codec: "H.264".to_string(),
            audio_codec: "AAC".to_string(),
            medium: "BluRay".to_string(),
            source_site: "hdsky".to_string(),
            source_url: "https://hdsky.me/details.php?id=1".to_string(),
            torrent_file_data: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: RawTorrentInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "Movie");
        assert_eq!(deserialized.images.len(), 1);
    }
}
