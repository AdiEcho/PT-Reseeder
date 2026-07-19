use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;
use tracing::{debug, error, warn};

use crate::downloader::models::*;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, DownloaderError};

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
            torrent_file: None,
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
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
            })?;
        let version = resp
            .text()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;
        Ok(version)
    }

    /// Get total number of torrents.
    pub async fn get_torrent_count(&self) -> Result<u64, CoreError> {
        let url = format!("{}/api/v2/torrents/info", self.base_url());
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
            })?;
        let torrents: Vec<QBTorrentInfo> = resp
            .json()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;
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

        let body = resp
            .text()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        if body.trim() != "Ok." {
            warn!(host = %self.host, port = %self.port, "qBittorrent auth failed");
            return Err(CoreError::Downloader(DownloaderError::AuthFailed(format!(
                "login failed for {}:{}",
                self.host, self.port
            ))));
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
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
            })?;
        let torrents: Vec<QBTorrentInfo> = resp
            .json()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        Ok(torrents.into_iter().next().map(TorrentInfo::from))
    }

    async fn get_all_info_hashes(&self) -> Result<HashSet<String>, CoreError> {
        Ok(self
            .list_torrents()
            .await?
            .into_iter()
            .map(|t| t.info_hash)
            .collect())
    }

    async fn list_torrents(&self) -> Result<Vec<TorrentInfo>, CoreError> {
        let url = format!("{}/api/v2/torrents/info", self.base_url());
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
            })?;
        let torrents: Vec<QBTorrentInfo> = resp
            .json()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        Ok(torrents.into_iter().map(TorrentInfo::from).collect())
    }

    async fn add_torrent(&self, opts: AddTorrentOpts) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/add", self.base_url());

        let file_part = multipart::Part::bytes(opts.torrent_data)
            .file_name("torrent.torrent")
            .mime_str("application/x-bittorrent")
            .map_err(|e| CoreError::Downloader(DownloaderError::AddFailed(e.to_string())))?;

        let skip_checking = if opts.skip_hash_check {
            "true"
        } else {
            "false"
        };
        let paused = if opts.paused { "true" } else { "false" };

        let mut form = multipart::Form::new()
            .part("torrents", file_part)
            .text("savepath", opts.save_path)
            .text("skip_checking", skip_checking.to_string())
            .text("paused", paused.to_string());

        if let Some(tag) = opts.tag {
            form = form.text("tags", tag);
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                error!("failed to add torrent: {}", e);
                CoreError::Downloader(DownloaderError::AddFailed(e.to_string()))
            })?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::AddFailed(e.to_string())))?;

        if status.is_success() && body.trim() == "Ok." {
            debug!("torrent added successfully");
            Ok(true)
        } else {
            warn!(status = %status, body = %body, "add torrent returned unexpected response");
            Err(CoreError::Downloader(DownloaderError::AddFailed(format!(
                "status={}, body={}",
                status, body
            ))))
        }
    }

    async fn resume_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/resume", self.base_url());
        let params = [("hashes", info_hash)];

        self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        debug!(info_hash = %info_hash, "torrent resumed");
        Ok(true)
    }

    async fn pause_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let url = format!("{}/api/v2/torrents/pause", self.base_url());
        let params = [("hashes", info_hash)];

        self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        debug!(info_hash = %info_hash, "torrent paused");
        Ok(true)
    }

    async fn export_torrent(&self, info_hash: &str) -> Result<Option<Vec<u8>>, CoreError> {
        let url = format!(
            "{}/api/v2/torrents/export?hash={}",
            self.base_url(),
            info_hash
        );
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        let status = resp.status();
        if status.as_u16() == 404 {
            // Older qB builds may not expose export; fall back to torrent_dir scan.
            debug!(info_hash = %info_hash, "qBittorrent export endpoint not available");
            return Ok(None);
        }
        if !status.is_success() {
            warn!(
                info_hash = %info_hash,
                status = %status,
                "qBittorrent export failed"
            );
            return Ok(None);
        }

        let bytes = resp.bytes().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(bytes.to_vec()))
    }

    async fn get_pieces_hash(&self, info_hash: &str) -> Result<Option<String>, CoreError> {
        use sha1::{Digest as _, Sha1};

        // qBittorrent WebAPI: GET /api/v2/torrents/pieceHashes?hash=<hash>
        // Returns a JSON array of 40-char hex piece hashes. pieces_hash is
        // SHA1 of the raw concatenated 20-byte piece digests (same as info.pieces).
        let url = format!(
            "{}/api/v2/torrents/pieceHashes?hash={}",
            self.base_url(),
            info_hash
        );
        let resp = self.client.get(&url).send().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;

        let status = resp.status();
        if status.as_u16() == 404 || status.as_u16() == 400 {
            debug!(
                info_hash = %info_hash,
                status = %status,
                "qBittorrent pieceHashes unavailable"
            );
            return Ok(None);
        }
        if !status.is_success() {
            warn!(
                info_hash = %info_hash,
                status = %status,
                "qBittorrent pieceHashes failed"
            );
            return Ok(None);
        }

        let piece_hexes: Vec<String> = resp.json().await.map_err(|e| {
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })?;
        if piece_hexes.is_empty() {
            return Ok(None);
        }

        let mut pieces_bytes = Vec::with_capacity(piece_hexes.len() * 20);
        for hex in &piece_hexes {
            let decoded = decode_hex20(hex).ok_or_else(|| {
                CoreError::Downloader(DownloaderError::ConnectionFailed(format!(
                    "invalid piece hash hex from qBittorrent: {}",
                    hex
                )))
            })?;
            pieces_bytes.extend_from_slice(&decoded);
        }

        let digest = Sha1::digest(&pieces_bytes);
        let pieces_hash = digest.iter().map(|b| format!("{:02x}", b)).collect();
        Ok(Some(pieces_hash))
    }

    async fn close(&mut self) -> Result<(), CoreError> {
        let url = format!("{}/api/v2/auth/logout", self.base_url());
        let _ = self.client.post(&url).send().await;
        self.connected = false;
        debug!(host = %self.host, port = %self.port, "disconnected from qBittorrent");
        Ok(())
    }
}

/// Decode a 40-char hex string into exactly 20 bytes.
fn decode_hex20(hex: &str) -> Option<[u8; 20]> {
    if hex.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_prepends_http_when_no_scheme() {
        let client = QBittorrentClient::new("192.168.1.1", 8080, "admin", "pw");
        assert_eq!(client.base_url(), "http://192.168.1.1:8080");
    }

    #[test]
    fn base_url_preserves_http_scheme() {
        let client = QBittorrentClient::new("http://myhost", 8080, "admin", "pw");
        assert_eq!(client.base_url(), "http://myhost:8080");
    }

    #[test]
    fn base_url_preserves_https_scheme() {
        let client = QBittorrentClient::new("https://secure.host", 443, "admin", "pw");
        assert_eq!(client.base_url(), "https://secure.host:443");
    }

    #[test]
    fn new_client_starts_disconnected() {
        let client = QBittorrentClient::new("host", 8080, "u", "p");
        assert!(!client.connected);
    }

    #[test]
    fn qb_torrent_info_converts_to_torrent_info() {
        let qb = QBTorrentInfo {
            hash: "abc123".to_string(),
            name: "test.mkv".to_string(),
            save_path: "/downloads".to_string(),
            progress: 0.5,
            state: "downloading".to_string(),
            total_size: 1024,
            added_on: Some(1700000000),
        };
        let info: TorrentInfo = qb.into();
        assert_eq!(info.info_hash, "abc123");
        assert_eq!(info.name, "test.mkv");
        assert_eq!(info.save_path, "/downloads");
        assert_eq!(info.progress, 0.5);
        assert_eq!(info.total_size, 1024);
    }

    #[test]
    fn qb_torrent_info_clamps_negative_size_to_zero() {
        let qb = QBTorrentInfo {
            hash: "h".to_string(),
            name: "n".to_string(),
            save_path: "/p".to_string(),
            progress: 0.0,
            state: "s".to_string(),
            total_size: -100,
            added_on: None,
        };
        let info: TorrentInfo = qb.into();
        assert_eq!(info.total_size, 0);
    }

    #[test]
    fn decode_hex20_accepts_lowercase_and_uppercase() {
        let lower = decode_hex20("0123456789abcdef0123456789abcdef01234567").unwrap();
        let upper = decode_hex20("0123456789ABCDEF0123456789ABCDEF01234567").unwrap();
        assert_eq!(lower, upper);
        assert_eq!(lower[0], 0x01);
        assert_eq!(lower[1], 0x23);
        assert_eq!(lower[19], 0x67);
    }

    #[test]
    fn decode_hex20_rejects_bad_input() {
        assert!(decode_hex20("abc").is_none());
        assert!(decode_hex20("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }
}
