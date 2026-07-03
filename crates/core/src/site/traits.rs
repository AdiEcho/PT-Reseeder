use std::collections::HashSet;
use async_trait::async_trait;
use crate::error::CoreError;
use super::models::{AdaptedTorrentInfo, RawTorrentInfo, TorrentSearchResult, UserStats};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SiteCapability {
    Reseed,
    Repost,
    UserInfo,
    Search,
}

pub trait SiteCore: Send + Sync {
    fn name(&self) -> &str;
    fn base_url(&self) -> &str;
    fn capabilities(&self) -> HashSet<SiteCapability>;
}

#[async_trait]
pub trait ReseedCapable: SiteCore {
    /// Query site API with pieces hashes, returns list of (pieces_hash, torrent_id) matches
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError>;
    /// Build download URL for a torrent by its site-specific ID
    fn build_download_url(&self, torrent_id: i64) -> String;
}

#[async_trait]
pub trait RepostCapable: SiteCore {
    /// Extract full torrent details from site
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError>;
    /// Submit an adapted torrent to the site
    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError>;
}

#[async_trait]
pub trait UserInfoCapable: SiteCore {
    /// Fetch current user stats from the site
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError>;
    /// Fetch user's passkey if available
    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError>;
}

#[async_trait]
pub trait SearchCapable: SiteCore {
    /// Search for torrents by name with optional size hint
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError>;
}
