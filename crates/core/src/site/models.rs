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
