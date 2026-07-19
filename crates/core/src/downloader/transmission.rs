use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::downloader::models::*;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, DownloaderError};

/// Base64 encoding (standard alphabet, with padding).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Internal request body for Transmission RPC calls.
#[derive(Serialize)]
struct RpcRequest {
    method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    arguments: serde_json::Value,
}

/// Internal response body from Transmission RPC.
#[derive(Deserialize)]
struct RpcResponse {
    result: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

/// Internal deserialization struct for Transmission torrent info.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrTorrentInfo {
    hash_string: String,
    name: String,
    #[serde(default)]
    status: i64,
    #[serde(default)]
    total_size: i64,
    #[serde(default)]
    percent_done: f64,
    #[serde(default)]
    download_dir: String,
    /// Absolute path to the `.torrent` file on the Transmission host.
    #[serde(default)]
    torrent_file: Option<String>,
}

impl TrTorrentInfo {
    fn status_string(&self) -> &'static str {
        match self.status {
            0 => "stopped",
            1 => "check_pending",
            2 => "checking",
            3 => "download_pending",
            4 => "downloading",
            5 => "seed_pending",
            6 => "seeding",
            _ => "unknown",
        }
    }
}

impl From<TrTorrentInfo> for TorrentInfo {
    fn from(tr: TrTorrentInfo) -> Self {
        let state = tr.status_string().to_string();
        let torrent_file = tr
            .torrent_file
            .filter(|p| !p.trim().is_empty());
        TorrentInfo {
            info_hash: tr.hash_string,
            name: tr.name,
            save_path: tr.download_dir,
            progress: tr.percent_done,
            state,
            total_size: tr.total_size.max(0) as u64,
            added_on: None,
            torrent_file,
        }
    }
}

pub struct TransmissionClient {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    client: reqwest::Client,
    session_id: RwLock<Option<String>>,
    connected: bool,
}

impl TransmissionClient {
    pub fn new(host: &str, port: u16, username: Option<&str>, password: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .expect("failed to build reqwest client");

        Self {
            host: host.to_string(),
            port,
            username: username.map(String::from),
            password: password.map(String::from),
            client,
            session_id: RwLock::new(None),
            connected: false,
        }
    }

    fn rpc_url(&self) -> String {
        if self.host.starts_with("http://") || self.host.starts_with("https://") {
            format!("{}:{}/transmission/rpc", self.host, self.port)
        } else {
            format!("http://{}:{}/transmission/rpc", self.host, self.port)
        }
    }

    /// Execute an RPC call against the Transmission daemon.
    /// Handles the 409 Conflict / X-Transmission-Session-Id handshake automatically.
    async fn rpc_call(
        &self,
        method: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, CoreError> {
        let body = RpcRequest {
            method: method.to_string(),
            arguments,
        };

        let resp = self.send_rpc_request(&body).await?;

        // If 409, extract session id and retry once
        if resp.status() == reqwest::StatusCode::CONFLICT {
            if let Some(sid) = resp.headers().get("X-Transmission-Session-Id") {
                let sid_str = sid.to_str().unwrap_or_default().to_string();
                debug!(session_id = %sid_str, "received new Transmission session ID");
                {
                    let mut lock = self.session_id.write().await;
                    *lock = Some(sid_str);
                }
                // Retry with updated session id
                let resp = self.send_rpc_request(&body).await?;
                return self.parse_rpc_response(resp).await;
            } else {
                return Err(CoreError::Downloader(DownloaderError::ConnectionFailed(
                    "409 Conflict without X-Transmission-Session-Id header".to_string(),
                )));
            }
        }

        self.parse_rpc_response(resp).await
    }

    async fn send_rpc_request(&self, body: &RpcRequest) -> Result<reqwest::Response, CoreError> {
        let url = self.rpc_url();
        let mut req = self.client.post(&url).json(body);

        // Add Basic Auth if credentials are provided
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user, Some(pass));
        }

        // Add session id header if we have one
        {
            let lock = self.session_id.read().await;
            if let Some(sid) = lock.as_ref() {
                req = req.header("X-Transmission-Session-Id", sid.as_str());
            }
        }

        req.send().await.map_err(|e| {
            error!(host = %self.host, port = %self.port, "failed to send RPC request to Transmission: {}", e);
            CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string()))
        })
    }

    async fn parse_rpc_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<serde_json::Value, CoreError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CoreError::Downloader(DownloaderError::AuthFailed(format!(
                "authentication failed for {}:{}",
                self.host, self.port
            ))));
        }
        if !status.is_success() {
            return Err(CoreError::Downloader(DownloaderError::ConnectionFailed(
                format!("HTTP {} from {}:{}", status, self.host, self.port),
            )));
        }

        let rpc_resp: RpcResponse = resp
            .json()
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        if rpc_resp.result != "success" {
            return Err(CoreError::Downloader(DownloaderError::ConnectionFailed(
                format!("RPC error: {}", rpc_resp.result),
            )));
        }

        Ok(rpc_resp.arguments)
    }

    /// Get Transmission version string via session-get.
    pub async fn get_version(&self) -> Result<String, CoreError> {
        let args = self
            .rpc_call("session-get", serde_json::Value::Null)
            .await?;
        let version = args
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        Ok(version)
    }
}

#[async_trait]
impl Downloader for TransmissionClient {
    async fn connect(&mut self) -> Result<(), CoreError> {
        // Transmission uses a session-id handshake rather than explicit login.
        // Trigger the handshake by calling session-get.
        let _ = self
            .rpc_call("session-get", serde_json::Value::Null)
            .await
            .map_err(|e| {
                error!(host = %self.host, port = %self.port, "failed to connect to Transmission: {}", e);
                e
            })?;

        self.connected = true;
        debug!(host = %self.host, port = %self.port, "connected to Transmission");
        Ok(())
    }

    async fn test_connection(&self) -> Result<bool, CoreError> {
        match self.get_version().await {
            Ok(version) => {
                debug!(version = %version, "Transmission connection test successful");
                Ok(true)
            }
            Err(e) => {
                warn!("Transmission connection test failed: {}", e);
                Ok(false)
            }
        }
    }

    async fn get_torrent_info(&self, info_hash: &str) -> Result<Option<TorrentInfo>, CoreError> {
        let args = serde_json::json!({
            "ids": [info_hash],
            "fields": ["hashString", "name", "status", "totalSize", "percentDone", "downloadDir", "torrentFile"]
        });
        let result = self.rpc_call("torrent-get", args).await?;

        let torrents: Vec<TrTorrentInfo> = serde_json::from_value(
            result
                .get("torrents")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
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
        let args = serde_json::json!({
            "fields": ["hashString", "name", "status", "totalSize", "percentDone", "downloadDir", "torrentFile"]
        });
        let result = self.rpc_call("torrent-get", args).await?;

        let torrents: Vec<TrTorrentInfo> = serde_json::from_value(
            result
                .get("torrents")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        Ok(torrents.into_iter().map(TorrentInfo::from).collect())
    }

    async fn add_torrent(&self, opts: AddTorrentOpts) -> Result<bool, CoreError> {
        let metainfo = base64_encode(&opts.torrent_data);
        let mut args = serde_json::json!({
            "metainfo": metainfo,
            "download-dir": opts.save_path,
            "paused": opts.paused,
        });

        if let Some(obj) = args.as_object_mut() {
            if let Some(tag) = &opts.tag {
                obj.insert("labels".to_string(), serde_json::json!([tag]));
            }
        }

        let result = self.rpc_call("torrent-add", args).await;

        match result {
            Ok(resp) => {
                if resp.get("torrent-added").is_some() || resp.get("torrent-duplicate").is_some() {
                    debug!("torrent added to Transmission successfully");
                    Ok(true)
                } else {
                    warn!(response = %resp, "unexpected torrent-add response");
                    Err(CoreError::Downloader(DownloaderError::AddFailed(format!(
                        "unexpected response: {}",
                        resp
                    ))))
                }
            }
            Err(e) => {
                error!("failed to add torrent to Transmission: {}", e);
                Err(e)
            }
        }
    }

    async fn resume_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let args = serde_json::json!({
            "ids": [info_hash]
        });

        self.rpc_call("torrent-start", args)
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        debug!(info_hash = %info_hash, "torrent resumed in Transmission");
        Ok(true)
    }

    async fn pause_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        let args = serde_json::json!({
            "ids": [info_hash]
        });

        self.rpc_call("torrent-stop", args)
            .await
            .map_err(|e| CoreError::Downloader(DownloaderError::ConnectionFailed(e.to_string())))?;

        debug!(info_hash = %info_hash, "torrent paused in Transmission");
        Ok(true)
    }

    async fn export_torrent(&self, info_hash: &str) -> Result<Option<Vec<u8>>, CoreError> {
        // Transmission has no binary export RPC. When PT-Reseeder shares a filesystem
        // with the client (same host / bind-mount), read the path from `torrentFile`.
        let info = self.get_torrent_info(info_hash).await?;
        let Some(path) = info.and_then(|t| t.torrent_file) else {
            debug!(
                info_hash = %info_hash,
                "Transmission torrentFile unavailable; configure torrent_dir or share the torrents directory"
            );
            return Ok(None);
        };

        match std::fs::read(&path) {
            Ok(bytes) if !bytes.is_empty() => {
                debug!(info_hash = %info_hash, path = %path, "read Transmission torrentFile");
                Ok(Some(bytes))
            }
            Ok(_) => {
                warn!(info_hash = %info_hash, path = %path, "Transmission torrentFile is empty");
                Ok(None)
            }
            Err(e) => {
                // Path may be absolute on another host; treat as unavailable rather than hard-fail.
                debug!(
                    info_hash = %info_hash,
                    path = %path,
                    error = %e,
                    "cannot read Transmission torrentFile"
                );
                Ok(None)
            }
        }
    }

    async fn close(&mut self) -> Result<(), CoreError> {
        // Transmission doesn't have an explicit logout; just clear state.
        {
            let mut lock = self.session_id.write().await;
            *lock = None;
        }
        self.connected = false;
        debug!(host = %self.host, port = %self.port, "disconnected from Transmission");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_url_prepends_http_when_no_scheme() {
        let client = TransmissionClient::new("192.168.1.1", 9091, None, None);
        assert_eq!(client.rpc_url(), "http://192.168.1.1:9091/transmission/rpc");
    }

    #[test]
    fn rpc_url_preserves_http_scheme() {
        let client = TransmissionClient::new("http://myhost", 9091, None, None);
        assert_eq!(client.rpc_url(), "http://myhost:9091/transmission/rpc");
    }

    #[test]
    fn rpc_url_preserves_https_scheme() {
        let client = TransmissionClient::new("https://secure.host", 443, None, None);
        assert_eq!(client.rpc_url(), "https://secure.host:443/transmission/rpc");
    }

    #[test]
    fn new_client_starts_disconnected() {
        let client = TransmissionClient::new("host", 9091, Some("user"), Some("pass"));
        assert!(!client.connected);
    }

    #[test]
    fn new_client_stores_credentials() {
        let client = TransmissionClient::new("h", 9091, Some("u"), Some("p"));
        assert_eq!(client.username, Some("u".to_string()));
        assert_eq!(client.password, Some("p".to_string()));
    }

    #[test]
    fn new_client_without_credentials() {
        let client = TransmissionClient::new("h", 9091, None, None);
        assert!(client.username.is_none());
        assert!(client.password.is_none());
    }

    #[test]
    fn tr_torrent_info_status_string_maps_correctly() {
        let cases = vec![
            (0, "stopped"),
            (1, "check_pending"),
            (2, "checking"),
            (3, "download_pending"),
            (4, "downloading"),
            (5, "seed_pending"),
            (6, "seeding"),
            (7, "unknown"),
            (99, "unknown"),
        ];
        for (status, expected) in cases {
            let info = TrTorrentInfo {
                hash_string: String::new(),
                name: String::new(),
                status,
                total_size: 0,
                percent_done: 0.0,
                download_dir: String::new(),
                torrent_file: None,
            };
            assert_eq!(
                info.status_string(),
                expected,
                "status {} should map to {}",
                status,
                expected
            );
        }
    }

    #[test]
    fn tr_torrent_info_converts_to_torrent_info() {
        let tr = TrTorrentInfo {
            hash_string: "abc123".to_string(),
            name: "test.mkv".to_string(),
            status: 6,
            total_size: 2048,
            percent_done: 1.0,
            download_dir: "/downloads".to_string(),
            torrent_file: Some("/var/lib/transmission/torrents/abc.torrent".to_string()),
        };
        let info: TorrentInfo = tr.into();
        assert_eq!(info.info_hash, "abc123");
        assert_eq!(info.name, "test.mkv");
        assert_eq!(info.state, "seeding");
        assert_eq!(info.total_size, 2048);
        assert_eq!(info.progress, 1.0);
        assert_eq!(info.save_path, "/downloads");
        assert!(info.added_on.is_none()); // Transmission doesn't provide this
        assert_eq!(
            info.torrent_file.as_deref(),
            Some("/var/lib/transmission/torrents/abc.torrent")
        );
    }

    #[test]
    fn tr_torrent_info_clamps_negative_size_to_zero() {
        let tr = TrTorrentInfo {
            hash_string: "h".to_string(),
            name: "n".to_string(),
            status: 0,
            total_size: -500,
            percent_done: 0.0,
            download_dir: "/d".to_string(),
            torrent_file: Some("".to_string()),
        };
        let info: TorrentInfo = tr.into();
        assert_eq!(info.total_size, 0);
        assert!(info.torrent_file.is_none());
    }

    #[test]
    fn base64_encode_empty_input() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn base64_encode_standard_vectors() {
        // Standard base64 test vectors
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
