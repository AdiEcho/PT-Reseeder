use async_trait::async_trait;
use std::collections::HashSet;
use crate::error::CoreError;
use super::models::{AddTorrentOpts, TorrentInfo};

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn connect(&mut self) -> Result<(), CoreError>;
    async fn test_connection(&self) -> Result<bool, CoreError>;
    async fn get_torrent_info(&self, info_hash: &str) -> Result<Option<TorrentInfo>, CoreError>;
    async fn get_all_info_hashes(&self) -> Result<HashSet<String>, CoreError>;
    async fn add_torrent(&self, opts: AddTorrentOpts) -> Result<bool, CoreError>;
    async fn resume_torrent(&self, info_hash: &str) -> Result<bool, CoreError>;
    async fn pause_torrent(&self, info_hash: &str) -> Result<bool, CoreError>;
    async fn close(&mut self) -> Result<(), CoreError>;
}
