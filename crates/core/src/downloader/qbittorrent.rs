use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;
use tracing::{debug, warn, error};

use crate::error::{CoreError, DownloaderError};
use crate::downloader::models::*;
use crate::downloader::traits::Downloader;

/// Internal deserialization struct for qBittorrent torrent info API responses.
#[derive(Deserialize)]
struct QBTorrentInfo {
    hash: String,
    name: String,
    save_path: String,
    progress: f64,
    state: String,
    total_size: i64,
    added_on: Option<i64>,
}

impl From<QBTorrentInfo> for TorrentInfo {
    fn from(qb: QBTorrentInfo) -> Self {
        TorrentInfo {
            info_hash: qb.hash,
            name: qb.name,
            save_path: qb.save_path,
            progress: qb.progress,
            state: qb.state,
            total_size: qb.total_size.max(0) as u64,
            added_on: qb.added_on,
        }
    }
}

pub struct QBittorrentClient {
    host: String,
    port: u16,
    username: String,
    password: String,
    client: reqwest::Client,
    connected: bool,
}

impl QBittorrentClient {
    pub fn new(host: &str, port: u16, username: &str, password: &str) -> Self {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar)
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .expect("failed to build reqwest client");

        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            password: password.to_string(),
            client,
            connected: false,
        }
    }

    fn base_url(&self) -> String {
        if self.host.starts_with("http://") || self.host.starts_with("https://") {
            format!("{}:{}", self.host, self.port)
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }

    /// Get qBittorrent version string.
    pub async fn get_version(&self) -> Result<String, CoreError> {
        let url = format!("{}/api/v2/app/version", self.base_url());
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        let version = resp.text().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        Ok(version)
    }

    /// Get total number of torrents.
    pub async fn get_torrent_count(&self) -> Result<u64, CoreError> {
        let url = format!("{}/api/v2/torrents/info", self.base_url());
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        let torrents: Vec<QBTorrentInfo> = resp.json().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        Ok(torrents.len() as u64)
    }

    /// Get full status for UI display.
    pub async fn get_status(&self) -> Result<DownloaderStatus, CoreError> {
        let version = self.get_version().await.ok();
        let torrent_count = self.get_torrent_count().await.ok();
        Ok(DownloaderStatus {
            connected: self.connected,
            version,
            torrent_count,
        })
    }
}

#[async_trait]
impl Downloader for QBittorrentClient {
    async fn connect(&mut self) -> Result<(), CoreError> {
        let url = format!("{}/api/v2/auth/login", self.base_url());
        let params = [
            ("username", self.username.as_str()),
            ("password", self.password.as_str()),
        ];

        let resp = self.client.post(&url).form(&params).send().await.map_err(|e| {
            error!(host = %self.host, port = %self.port, "failed to connect to qBittorrent: {}", e);
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        let body = resp.text().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        if body.trim() != "Ok." {
            warn!(host = %self.host, port = %self.port, "qBittorrent auth failed");
            return Err(CoreError::Downloader(DownloaderError::AuthFailed(
                format!("login failed for {}:{}", self.host, self.port),
            )));
        }

        self.connected = true;
        debug!(host = %self.host, port = %self.port, "connected to qBittorrent");
        Ok(())
    }

    async fn test_connection(&self) -> Result<bool, CoreError> {
        match self.get_version().await {
            Ok(version) => {
                debug!(version = %version, "qBittorrent connection test successful");
                Ok(true)
            }
            Err(e) => {
                warn!("qBittorrent connection test failed: {}", e);
                Ok(false)
            }
        }
    }

    async fn get_torrent_info(&self, info_hash: &str) -> Result<Option<TorrentInfo>, CoreError> {
        let url = format!(
            "{}/api/v2/torrents/info?hashes={}",
            self.base_url(),
            info_hash
        );
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        let torrents: Vec<QBTorrentInfo> = resp.json().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        Ok(torrents.into_iter().next().map(TorrentInfo::from))
    }

    async fn get_all_info_hashes(&self) -> Result<HashSet<String>, CoreError> {
        let url = format!("{}/api/v2/torrents/info", self.base_url());
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        let torrents: Vec<QBTorrentInfo> = resp.json().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        Ok(torrents.into_iter().map(|t| t.hash).collect())
    }

    async fn add_torrent(&self, opts: AddTorrentOpts) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/add", self.base_url());

        let file_part = multipart::Part::bytes(opts.torrent_data)
            .file_name("torrent.torrent")
            .mime_str("application/x-bittorrent")
            .map_err(|e| {
                CoreError::Downloader(DownloaderError::AddFailed(e.to_string()))
            })?;

        let skip_checking = if opts.skip_hash_check { "true" } else { "false" };
        let paused = if opts.paused { "true" } else { "false" };

        let mut form = multipart::Form::new()
            .part("torrents", file_part)
            .text("savepath", opts.save_path)
            .text("skip_checking", skip_checking.to_string())
            .text("paused", paused.to_string());

        if let Some(tag) = opts.tag {
            form = form.text("tags", tag);
        }

        let resp = self.client.post(&url).multipart(form).send().await.map_err(|e| {
            error!("failed to add torrent: {}", e);
            CoreError::Downloader(DownloaderError::AddFailed(e.to_string()))
        })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::AddFailed(e.to_string()))
        })?;

        if status.is_success() && body.trim() == "Ok." {
            debug!("torrent added successfully");
            Ok(true)
        } else {
            warn!(status = %status, body = %body, "add torrent returned unexpected response");
            Err(CoreError::Downloader(DownloaderError::AddFailed(
                format!("status={}, body={}", status, body),
            )))
        }
    }

    async fn resume_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/resume", self.base_url());
        let params = [("hashes", info_hash)];

        self.client.post(&url).form(&params).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        debug!(info_hash = %info_hash, "torrent resumed");
        Ok(true)
    }

    async fn pause_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/pause", self.base_url());
        let params = [("hashes", info_hash)];

        self.client.post(&url).form(&params).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        debug!(info_hash = %info_hash, "torrent paused");
        Ok(true)
    }

    async fn close(&mut self) -> Result<(), CoreError> {
        let url = format!("{}/api/v2/auth/logout", self.base_url());
        let _ = self.client.post(&url).send().await;
        self.connected = false;
        debug!(host = %self.host, port = %self.port, "disconnected from qBittorrent");
        Ok(())
    }
}
