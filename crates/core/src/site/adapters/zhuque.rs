use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::{CoreError, SiteError};
use crate::site::models::*;
use crate::site::traits::*;

#[derive(Clone)]
pub struct ZhuqueAdapter {
    name: String,
    base_url: String,
    api_token: Option<String>,
    passkey: Option<String>,
    cookie: Option<String>,
    client: Client,
    batch_size: usize,
}

// ---------------------------------------------------------------------------
// Zhuque REST API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ZhuqueApiResponse<T> {
    #[serde(default)]
    code: Option<i32>,
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct ZhuqueSearchResponse {
    #[serde(default)]
    torrents: Vec<ZhuqueTorrentEntry>,
}

#[derive(Deserialize)]
struct ZhuqueTorrentEntry {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    seeders: u32,
    #[serde(default)]
    leechers: u32,
    #[serde(default)]
    info_hash: Option<String>,
}

#[derive(Deserialize)]
struct ZhuqueTorrentDetail {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    small_descr: Option<String>,
    #[serde(default)]
    descr: Option<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    info_hash: Option<String>,
    #[serde(default)]
    imdb_url: Option<String>,
    #[serde(default)]
    douban_url: Option<String>,
    #[serde(default)]
    mediainfo: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
    #[serde(default)]
    video_codec: Option<String>,
    #[serde(default)]
    audio_codec: Option<String>,
    #[serde(default)]
    medium: Option<String>,
}

#[derive(Deserialize)]
struct ZhuqueUserProfile {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    uploaded: Option<i64>,
    #[serde(default)]
    downloaded: Option<i64>,
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default)]
    bonus: Option<f64>,
    #[serde(default, rename = "class")]
    user_class: Option<String>,
    #[serde(default)]
    seeding_count: Option<i64>,
    #[serde(default)]
    leeching_count: Option<i64>,
    #[serde(default)]
    seeding_size: Option<i64>,
    #[serde(default)]
    passkey: Option<String>,
}

#[derive(Deserialize)]
struct ZhuquePiecesHashMatch {
    #[serde(default)]
    pieces_hash: String,
    #[serde(default)]
    torrent_id: i64,
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

impl ZhuqueAdapter {
    pub fn new(
        name: String,
        base_url: String,
        api_token: Option<String>,
        passkey: Option<String>,
        cookie: Option<String>,
        batch_size: usize,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("PT-Reseeder/0.1"));

        // Prefer Bearer token auth; fall back to cookie
        if let Some(ref token) = api_token {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(AUTHORIZATION, val);
            }
        } else if let Some(ref c) = cookie {
            if let Ok(val) = HeaderValue::from_str(c) {
                headers.insert(COOKIE, val);
            }
        }

        let client = Client::builder()
            .use_rustls_tls()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");

        Self {
            name,
            base_url,
            api_token,
            passkey,
            cookie,
            client,
            batch_size,
        }
    }

    /// Returns the configured batch size for hash queries.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Make an authenticated GET request to the Zhuque REST API.
    async fn api_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, CoreError> {
        let url = format!("{}{}", self.base_url, path);

        debug!(site = %self.name, path, "zhuque API GET request");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SiteError::RateLimited.into());
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SiteError::AuthFailed(format!("HTTP {status}")).into());
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SiteError::NotFound(format!("GET {path}")).into());
        }
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from {path}")).into());
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let api_resp: ZhuqueApiResponse<T> = serde_json::from_str(&body).map_err(|e| {
            SiteError::ParseError(format!("failed to parse zhuque response: {e}"))
        })?;

        // code == 0 or code absent means success
        if let Some(code) = api_resp.code {
            if code != 0 {
                let msg = api_resp.message.unwrap_or_default();
                return Err(SiteError::HttpError(format!("API error code={code}: {msg}")).into());
            }
        }

        api_resp.data.ok_or_else(|| {
            SiteError::ParseError("API data field is null".into()).into()
        })
    }

    /// Make an authenticated POST request with a JSON body to the Zhuque REST API.
    async fn api_post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, CoreError> {
        let url = format!("{}{}", self.base_url, path);

        debug!(site = %self.name, path, "zhuque API POST request");

        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SiteError::RateLimited.into());
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SiteError::AuthFailed(format!("HTTP {status}")).into());
        }
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from POST {path}")).into());
        }

        let body_text = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let api_resp: ZhuqueApiResponse<T> =
            serde_json::from_str(&body_text).map_err(|e| {
                SiteError::ParseError(format!("failed to parse zhuque response: {e}"))
            })?;

        if let Some(code) = api_resp.code {
            if code != 0 {
                let msg = api_resp.message.unwrap_or_default();
                return Err(SiteError::HttpError(format!("API error code={code}: {msg}")).into());
            }
        }

        api_resp.data.ok_or_else(|| {
            SiteError::ParseError("API data field is null".into()).into()
        })
    }

    /// Check whether we have valid auth credentials configured.
    fn require_auth(&self) -> Result<(), CoreError> {
        if self.api_token.is_none() && self.cookie.is_none() {
            return Err(
                SiteError::AuthFailed("no api_token or cookie configured".into()).into(),
            );
        }
        Ok(())
    }

    /// Extract image URLs from BBCode description text.
    fn extract_images_from_descr(descr: &str) -> Vec<String> {
        let mut images = Vec::new();
        let mut search = descr;
        while let Some(start) = search.find("[img]") {
            let after = &search[start + 5..];
            if let Some(end) = after.find("[/img]") {
                let url = after[..end].trim();
                if !url.is_empty() {
                    images.push(url.to_string());
                }
                search = &after[end + 6..];
            } else {
                break;
            }
        }
        images
    }
}

// ---------------------------------------------------------------------------
// SiteCore
// ---------------------------------------------------------------------------

impl SiteCore for ZhuqueAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn capabilities(&self) -> HashSet<SiteCapability> {
        let mut caps = HashSet::new();
        caps.insert(SiteCapability::Reseed);
        caps.insert(SiteCapability::Repost);
        caps.insert(SiteCapability::UserInfo);
        caps.insert(SiteCapability::Search);
        caps
    }
}

// ---------------------------------------------------------------------------
// ReseedCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl ReseedCapable for ZhuqueAdapter {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        self.require_auth()?;

        debug!(
            site = %self.name,
            hash_count = hashes.len(),
            "querying pieces hashes via /api/torrent/queryByHash"
        );

        let body = serde_json::json!({
            "pieces_hash": hashes,
        });

        let matches: Vec<ZhuquePiecesHashMatch> = self
            .api_post_json("/api/torrent/queryByHash", &body)
            .await?;

        debug!(
            site = %self.name,
            matches = matches.len(),
            "pieces hash query completed"
        );

        Ok(matches
            .into_iter()
            .map(|m| (m.pieces_hash, m.torrent_id))
            .collect())
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        let mut url = format!(
            "{}/api/torrent/{}/download",
            self.base_url, torrent_id
        );
        if let Some(ref pk) = self.passkey {
            url.push_str(&format!("?passkey={pk}"));
        }
        url
    }

    fn batch_size(&self) -> usize {
        self.batch_size.max(1)
    }
}

// ---------------------------------------------------------------------------
// UserInfoCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl UserInfoCapable for ZhuqueAdapter {
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError> {
        self.require_auth()?;

        debug!(site = %self.name, "fetching user info via /api/user/me");

        let profile: ZhuqueUserProfile = self.api_get("/api/user/me").await?;

        debug!(
            site = %self.name,
            user_id = profile.id,
            username = ?profile.username,
            "user info fetched successfully"
        );

        Ok(UserStats {
            site_id: SiteId(profile.id),
            uploaded: profile.uploaded,
            downloaded: profile.downloaded,
            ratio: profile.ratio,
            bonus: profile.bonus,
            user_class: profile.user_class,
            seeding_count: profile.seeding_count,
            leeching_count: profile.leeching_count,
            seeding_size: profile.seeding_size,
            upload_time_seconds: None,
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
        })
    }

    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError> {
        // Return configured passkey if available
        if self.passkey.is_some() {
            return Ok(self.passkey.clone());
        }

        self.require_auth()?;

        // Try fetching from user profile API
        debug!(site = %self.name, "attempting to fetch passkey from /api/user/me");

        let profile: ZhuqueUserProfile = self.api_get("/api/user/me").await?;

        if let Some(ref pk) = profile.passkey {
            if !pk.is_empty() {
                debug!(site = %self.name, "passkey found in user profile");
                return Ok(Some(pk.clone()));
            }
        }

        warn!(site = %self.name, "passkey not found in user profile");
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// RepostCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl RepostCapable for ZhuqueAdapter {
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        self.require_auth()?;

        debug!(site = %self.name, torrent_id, "extracting torrent detail via /api/torrent/");

        let path = format!("/api/torrent/{}", torrent_id);
        let detail: ZhuqueTorrentDetail = self.api_get(&path).await?;

        let name = detail.name.clone();
        let small_descr = detail.small_descr.clone().unwrap_or_default();
        let descr = detail.descr.clone().unwrap_or_default();
        let images = Self::extract_images_from_descr(&descr);

        debug!(site = %self.name, torrent_id, name = %name, "torrent detail extracted");

        // Download the torrent file
        let torrent_file_data = match torrent_id.parse::<i64>() {
            Ok(id) => {
                let download_url = self.build_download_url(id);
                match self.client.get(&download_url).send().await {
                    Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                        Ok(bytes) => Some(bytes.to_vec()),
                        Err(e) => {
                            warn!(site = %self.name, torrent_id, "failed to read torrent file bytes: {e}");
                            None
                        }
                    },
                    Ok(resp) => {
                        warn!(site = %self.name, torrent_id, status = %resp.status(), "failed to download torrent file");
                        None
                    }
                    Err(e) => {
                        warn!(site = %self.name, torrent_id, "failed to download torrent file: {e}");
                        None
                    }
                }
            }
            Err(_) => None,
        };

        Ok(RawTorrentInfo {
            name,
            small_descr,
            descr,
            imdb_url: detail.imdb_url,
            douban_url: detail.douban_url,
            mediainfo: detail.mediainfo,
            images,
            torrent_type: detail.category.unwrap_or_default(),
            region: detail.source.unwrap_or_default(),
            resolution: detail.resolution.unwrap_or_default(),
            video_codec: detail.video_codec.unwrap_or_default(),
            audio_codec: detail.audio_codec.unwrap_or_default(),
            medium: detail.medium.unwrap_or_default(),
            source_site: self.name.clone(),
            source_url: format!("{}/api/torrent/{}", self.base_url, torrent_id),
            torrent_file_data,
        })
    }

    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        self.require_auth()?;

        let url = format!("{}/api/torrent/upload", self.base_url);
        debug!(
            site = %self.name,
            name = %info.name,
            "submitting torrent (auth=[REDACTED])"
        );

        let mut form = reqwest::multipart::Form::new()
            .text("name", info.name.clone())
            .text("small_descr", info.small_descr.clone())
            .text("descr", info.descr.clone());

        if let Some(ref imdb) = info.imdb_url {
            form = form.text("imdb_url", imdb.clone());
        }
        if let Some(ref douban) = info.douban_url {
            form = form.text("douban_url", douban.clone());
        }
        if let Some(ref mi) = info.mediainfo {
            form = form.text("mediainfo", mi.clone());
        }
        if let Some(cat_id) = info.category_id {
            form = form.text("category_id", cat_id.to_string());
        }
        if let Some(source_id) = info.source_id {
            form = form.text("source_id", source_id.to_string());
        }
        if let Some(codec_id) = info.codec_id {
            form = form.text("codec_id", codec_id.to_string());
        }
        if let Some(res_id) = info.resolution_id {
            form = form.text("resolution_id", res_id.to_string());
        }

        if let Some(ref data) = info.torrent_file_data {
            let part = reqwest::multipart::Part::bytes(data.clone())
                .file_name("torrent.torrent")
                .mime_str("application/x-bittorrent")
                .map_err(|e| SiteError::HttpError(e.to_string()))?;
            form = form.part("torrent_file", part);
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Try to parse response for torrent id
        if let Ok(api_resp) = serde_json::from_str::<ZhuqueApiResponse<serde_json::Value>>(&body) {
            let success = api_resp.code.map_or(true, |c| c == 0);
            if success {
                // Extract torrent URL from response data if possible
                let torrent_url = api_resp
                    .data
                    .as_ref()
                    .and_then(|d| d.get("id"))
                    .and_then(|id| id.as_i64())
                    .map(|id| format!("{}/api/torrent/{}", self.base_url, id))
                    .unwrap_or_else(|| format!("{}/api/torrent/upload", self.base_url));

                debug!(site = %self.name, url = %torrent_url, "torrent submitted successfully");
                return Ok(torrent_url);
            }

            let msg = api_resp.message.unwrap_or_default();
            return Err(SiteError::HttpError(format!(
                "upload failed: code={}, msg={msg}",
                api_resp.code.unwrap_or(-1)
            ))
            .into());
        }

        Err(SiteError::HttpError(format!(
            "upload may have failed: HTTP {status}"
        ))
        .into())
    }
}

// ---------------------------------------------------------------------------
// SearchCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl SearchCapable for ZhuqueAdapter {
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError> {
        self.require_auth()?;

        let path = format!(
            "/api/torrent/search?keyword={}",
            urlencoding::encode(query)
        );
        debug!(site = %self.name, query, "searching torrents via /api/torrent/search");

        let search_resp: ZhuqueSearchResponse = self.api_get(&path).await?;

        let mut results: Vec<TorrentSearchResult> = search_resp
            .torrents
            .into_iter()
            .map(|t| TorrentSearchResult {
                id: t.id,
                name: t.name,
                size: t.size,
                seeders: t.seeders,
                leechers: t.leechers,
                info_hash: t.info_hash,
            })
            .collect();

        // Filter by size hint with +-1% tolerance
        if let Some(hint) = size_hint {
            let lower = (hint as f64 * 0.99) as u64;
            let upper = (hint as f64 * 1.01) as u64;
            results.retain(|r| r.size >= lower && r.size <= upper);
        }

        debug!(
            site = %self.name,
            count = results.len(),
            "search completed"
        );

        Ok(results)
    }
}
