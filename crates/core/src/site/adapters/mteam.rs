use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::{CoreError, SiteError};
use crate::site::models::*;
use crate::site::traits::*;

// ---------------------------------------------------------------------------
// API response models
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MTeamApiResponse<T> {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<T>,
}

#[derive(Deserialize, Default)]
struct PiecesHashData {
    #[serde(default)]
    data: Vec<PiecesHashMatch>,
}

#[derive(Deserialize)]
struct PiecesHashMatch {
    #[serde(default, alias = "piecesHash")]
    pieces_hash: String,
    #[serde(default, alias = "torrentId")]
    torrent_id: i64,
}

#[derive(Deserialize, Default)]
struct SearchData {
    #[serde(default)]
    data: Vec<SearchTorrent>,
}

#[derive(Deserialize)]
struct SearchTorrent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    status: Option<SearchTorrentStatus>,
    #[serde(default, alias = "infoHash")]
    info_hash: Option<String>,
}

#[derive(Deserialize)]
struct SearchTorrentStatus {
    #[serde(default)]
    seeders: Option<String>,
    #[serde(default)]
    leechers: Option<String>,
}

#[derive(Deserialize, Default)]
struct TorrentDetail {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "smallDescr")]
    small_descr: Option<String>,
    #[serde(default)]
    descr: Option<String>,
    #[serde(default)]
    imdb: Option<String>,
    #[serde(default)]
    douban: Option<String>,
    #[serde(default)]
    mediainfo: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    medium: Option<String>,
    #[serde(default, alias = "videoCodec")]
    video_codec: Option<String>,
    #[serde(default, alias = "audioCodec")]
    audio_codec: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
    #[serde(default)]
    images: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct ProfileData {
    #[serde(default, alias = "memberCount")]
    member_count: Option<MemberCount>,
    #[serde(default, alias = "memberClass")]
    member_class: Option<MemberClass>,
}

#[derive(Deserialize)]
struct MemberCount {
    #[serde(default)]
    uploaded: Option<String>,
    #[serde(default)]
    downloaded: Option<String>,
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default)]
    bonus: Option<f64>,
    #[serde(default, alias = "seedingCount")]
    seeding_count: Option<i64>,
    #[serde(default, alias = "leechingCount")]
    leeching_count: Option<i64>,
    #[serde(default, alias = "seedingSize")]
    seeding_size: Option<String>,
}

#[derive(Deserialize)]
struct MemberClass {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize, Default)]
struct DlTokenData {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "downloadUrl")]
    download_url: Option<String>,
}

#[derive(Deserialize, Default)]
struct UploadResult {
    #[serde(default)]
    id: Option<String>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MTeamAdapter {
    name: String,
    base_url: String,
    api_token: Option<String>,
    passkey: Option<String>,
    client: Client,
    batch_size: usize,
}

impl MTeamAdapter {
    pub fn new(
        name: String,
        base_url: String,
        api_token: Option<String>,
        passkey: Option<String>,
        batch_size: usize,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("PT-Reseeder/0.1"));
        if let Some(ref token) = api_token {
            if let Ok(val) = HeaderValue::from_str(token) {
                headers.insert("x-api-key", val);
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
            client,
            batch_size,
        }
    }

    /// Require a valid API token, returning an error if absent.
    fn require_api_token(&self) -> Result<&str, CoreError> {
        self.api_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| SiteError::AuthFailed("no API token configured".into()).into())
    }

    /// Send a POST request with a JSON body and parse the typed API response.
    async fn api_post<T: serde::de::DeserializeOwned + Default>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, CoreError> {
        let url = format!("{}{}", self.base_url, path);
        debug!(site = %self.name, path, "POST API request");

        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(format!("request to {path} failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            warn!(site = %self.name, path, "rate limited by API");
            return Err(SiteError::RateLimited.into());
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(SiteError::AuthFailed(format!(
                "HTTP {status} from {path} (token=[REDACTED])"
            ))
            .into());
        }
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from {path}")).into());
        }

        let api_resp: MTeamApiResponse<T> = resp.json().await.map_err(|e| {
            SiteError::ParseError(format!("failed to parse response from {path}: {e}"))
        })?;

        // MTeam uses code "0" or "SUCCESS" for success
        if let Some(ref code) = api_resp.code {
            if code != "0" && code.to_uppercase() != "SUCCESS" {
                let msg = api_resp.message.unwrap_or_else(|| code.clone());
                return Err(SiteError::HttpError(format!("API error from {path}: {msg}")).into());
            }
        }

        api_resp.data.ok_or_else(|| {
            SiteError::ParseError(format!("empty data in response from {path}")).into()
        })
    }

    /// Send a GET request and parse the typed API response.
    async fn api_get<T: serde::de::DeserializeOwned + Default>(
        &self,
        path: &str,
    ) -> Result<T, CoreError> {
        let url = format!("{}{}", self.base_url, path);
        debug!(site = %self.name, path, "GET API request");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(format!("request to {path} failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            warn!(site = %self.name, path, "rate limited by API");
            return Err(SiteError::RateLimited.into());
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(SiteError::AuthFailed(format!(
                "HTTP {status} from {path} (token=[REDACTED])"
            ))
            .into());
        }
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from {path}")).into());
        }

        let api_resp: MTeamApiResponse<T> = resp.json().await.map_err(|e| {
            SiteError::ParseError(format!("failed to parse response from {path}: {e}"))
        })?;

        if let Some(ref code) = api_resp.code {
            if code != "0" && code.to_uppercase() != "SUCCESS" {
                let msg = api_resp.message.unwrap_or_else(|| code.clone());
                return Err(SiteError::HttpError(format!("API error from {path}: {msg}")).into());
            }
        }

        api_resp.data.ok_or_else(|| {
            SiteError::ParseError(format!("empty data in response from {path}")).into()
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a string that may represent bytes as an integer (MTeam returns size as
/// a stringified integer in bytes).
fn parse_size_str(s: &str) -> u64 {
    s.trim().parse::<u64>().unwrap_or(0)
}

/// Parse a stringified integer, returning 0 on failure.
fn parse_u32_str(s: &str) -> u32 {
    s.trim().parse::<u32>().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// SiteCore
// ---------------------------------------------------------------------------

impl SiteCore for MTeamAdapter {
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
impl ReseedCapable for MTeamAdapter {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        self.require_api_token()?;

        debug!(
            site = %self.name,
            hash_count = hashes.len(),
            "querying pieces hashes via API (token=[REDACTED])"
        );

        let body = serde_json::json!({
            "piecesHashList": hashes,
        });

        let parsed: PiecesHashData = self
            .api_post("/api/torrent/queryByPiecesHash", &body)
            .await?;

        debug!(
            site = %self.name,
            matches = parsed.data.len(),
            "pieces hash query completed"
        );

        Ok(parsed
            .data
            .into_iter()
            .map(|m| (m.pieces_hash, m.torrent_id))
            .collect())
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        // MTeam uses token-based downloads; callers should use genDlToken first.
        // This returns the genDlToken endpoint which can be POSTed to obtain the
        // actual download URL. As a fallback, if a passkey is configured we can
        // build a direct link.
        if let Some(ref pk) = self.passkey {
            format!(
                "{}/download.php?id={}&passkey={}",
                self.base_url, torrent_id, pk
            )
        } else {
            format!("{}/api/torrent/genDlToken?id={}", self.base_url, torrent_id)
        }
    }

    fn batch_size(&self) -> usize {
        self.batch_size.max(1)
    }
}

// ---------------------------------------------------------------------------
// UserInfoCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl UserInfoCapable for MTeamAdapter {
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError> {
        self.require_api_token()?;

        debug!(site = %self.name, "fetching user profile via API (token=[REDACTED])");

        let profile: ProfileData = self
            .api_post("/api/member/profile", &serde_json::json!({}))
            .await?;

        let (uploaded, downloaded, ratio, bonus, seeding_count, leeching_count, seeding_size) =
            if let Some(ref mc) = profile.member_count {
                (
                    mc.uploaded.as_deref().and_then(|s| s.parse::<i64>().ok()),
                    mc.downloaded.as_deref().and_then(|s| s.parse::<i64>().ok()),
                    mc.ratio,
                    mc.bonus,
                    mc.seeding_count,
                    mc.leeching_count,
                    mc.seeding_size
                        .as_deref()
                        .and_then(|s| s.parse::<i64>().ok()),
                )
            } else {
                (None, None, None, None, None, None, None)
            };

        let user_class = profile.member_class.and_then(|mc| mc.name);

        debug!(site = %self.name, "user profile fetched successfully");

        Ok(UserStats {
            site_id: SiteId(0),
            uploaded,
            downloaded,
            ratio,
            bonus,
            user_class,
            seeding_count,
            leeching_count,
            seeding_size,
            upload_time_seconds: None, // MTeam API does not expose this field
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
        })
    }

    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError> {
        // If the adapter already has a passkey configured, return it directly.
        if let Some(ref pk) = self.passkey {
            if !pk.is_empty() {
                debug!(site = %self.name, "returning configured passkey");
                return Ok(Some(pk.clone()));
            }
        }

        self.require_api_token()?;

        debug!(site = %self.name, "fetching passkey from profile API (token=[REDACTED])");

        // The profile endpoint may contain a passkey or laboratory-style key.
        // We attempt to extract it from the full raw JSON response.
        let url = format!("{}/api/member/profile", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| SiteError::HttpError(format!("request to profile failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} fetching profile", resp.status())).into(),
            );
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Try to find passkey in the JSON response
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            // Check data.passkey
            if let Some(pk) = json
                .pointer("/data/passkey")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                debug!(site = %self.name, "passkey found in profile response");
                return Ok(Some(pk.to_string()));
            }

            // Check data.laborPasskey (some MTeam variants)
            if let Some(pk) = json
                .pointer("/data/laborPasskey")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                debug!(site = %self.name, "passkey found via laborPasskey field");
                return Ok(Some(pk.to_string()));
            }
        }

        warn!(site = %self.name, "passkey not found in profile API response");
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// RepostCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl RepostCapable for MTeamAdapter {
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        self.require_api_token()?;

        debug!(
            site = %self.name,
            torrent_id,
            "extracting torrent detail via API (token=[REDACTED])"
        );

        let path = format!("/api/torrent/detail?id={}", torrent_id);
        let detail: TorrentDetail = self.api_get(&path).await?;

        let name = detail.name.unwrap_or_default();
        let small_descr = detail.small_descr.unwrap_or_default();
        let descr = detail.descr.unwrap_or_default();
        let imdb_url = detail.imdb.filter(|s| !s.is_empty());
        let douban_url = detail.douban.filter(|s| !s.is_empty());
        let mediainfo = detail.mediainfo.filter(|s| !s.is_empty());
        let images = detail.images.unwrap_or_default();

        let torrent_type = detail.category.unwrap_or_default();
        let region = detail.source.unwrap_or_default();
        let resolution = detail.resolution.unwrap_or_default();
        let video_codec = detail.video_codec.unwrap_or_default();
        let audio_codec = detail.audio_codec.unwrap_or_default();
        let medium = detail.medium.unwrap_or_default();

        let source_url = format!("{}/api/torrent/detail?id={}", self.base_url, torrent_id);

        // Download the torrent file
        let torrent_file_data = match torrent_id.parse::<i64>() {
            Ok(id) => match self.download_torrent_file(id).await {
                Ok(data) => Some(data),
                Err(e) => {
                    warn!(
                        site = %self.name,
                        torrent_id,
                        "failed to download torrent file: {e}"
                    );
                    None
                }
            },
            Err(_) => {
                warn!(
                    site = %self.name,
                    torrent_id,
                    "invalid torrent id, skipping file download"
                );
                None
            }
        };

        debug!(
            site = %self.name,
            torrent_id,
            name = %name,
            "torrent detail extracted"
        );

        Ok(RawTorrentInfo {
            name,
            small_descr,
            descr,
            imdb_url,
            douban_url,
            mediainfo,
            images,
            torrent_type,
            region,
            resolution,
            video_codec,
            audio_codec,
            medium,
            source_site: self.name.clone(),
            source_url,
            torrent_file_data,
        })
    }

    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        self.require_api_token()?;

        let url = format!("{}/api/torrent/upload", self.base_url);
        debug!(
            site = %self.name,
            name = %info.name,
            "submitting torrent via API (token=[REDACTED])"
        );

        let mut form = reqwest::multipart::Form::new()
            .text("name", info.name.clone())
            .text("small_descr", info.small_descr.clone())
            .text("descr", info.descr.clone());

        if let Some(ref imdb) = info.imdb_url {
            form = form.text("imdb", imdb.clone());
        }
        if let Some(ref douban) = info.douban_url {
            form = form.text("douban", douban.clone());
        }
        if let Some(ref mi) = info.mediainfo {
            form = form.text("mediainfo", mi.clone());
        }
        if let Some(cat_id) = info.category_id {
            form = form.text("category", cat_id.to_string());
        }
        if let Some(source_id) = info.source_id {
            form = form.text("source", source_id.to_string());
        }
        if let Some(codec_id) = info.codec_id {
            form = form.text("videoCodec", codec_id.to_string());
        }
        if let Some(res_id) = info.resolution_id {
            form = form.text("resolution", res_id.to_string());
        }

        if let Some(ref data) = info.torrent_file_data {
            let part = reqwest::multipart::Part::bytes(data.clone())
                .file_name("torrent.torrent")
                .mime_str("application/x-bittorrent")
                .map_err(|e| SiteError::HttpError(e.to_string()))?;
            form = form.part("file", part);
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(SiteError::AuthFailed(format!(
                "HTTP {status} uploading torrent (token=[REDACTED])"
            ))
            .into());
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Parse to check for success
        let api_resp: MTeamApiResponse<UploadResult> = serde_json::from_str(&body)
            .map_err(|e| SiteError::ParseError(format!("failed to parse upload response: {e}")))?;

        let success = api_resp
            .code
            .as_deref()
            .map(|c| c == "0" || c.eq_ignore_ascii_case("SUCCESS"))
            .unwrap_or(false);

        if success {
            let result_id = api_resp.data.and_then(|d| d.id).unwrap_or_default();
            let result_url = if result_id.is_empty() {
                format!("{}/api/torrent/upload", self.base_url)
            } else {
                format!("{}/api/torrent/detail?id={}", self.base_url, result_id)
            };
            debug!(
                site = %self.name,
                url = %result_url,
                "torrent submitted successfully"
            );
            Ok(result_url)
        } else {
            let msg = api_resp.message.unwrap_or_else(|| format!("HTTP {status}"));
            Err(SiteError::HttpError(format!("upload failed: {msg}")).into())
        }
    }
}

// ---------------------------------------------------------------------------
// SearchCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl SearchCapable for MTeamAdapter {
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError> {
        self.require_api_token()?;

        debug!(
            site = %self.name,
            query,
            "searching torrents via API (token=[REDACTED])"
        );

        let body = serde_json::json!({
            "keyword": query,
            "pageNumber": 1,
            "pageSize": 100,
        });

        let parsed: SearchData = self.api_post("/api/torrent/search", &body).await?;

        let mut results: Vec<TorrentSearchResult> = parsed
            .data
            .into_iter()
            .filter_map(|t| {
                let id = t.id.parse::<i64>().ok()?;
                if t.name.is_empty() {
                    return None;
                }
                let size = t.size.as_deref().map(parse_size_str).unwrap_or(0);
                let (seeders, leechers) = t
                    .status
                    .map(|s| {
                        (
                            s.seeders.as_deref().map(parse_u32_str).unwrap_or(0),
                            s.leechers.as_deref().map(parse_u32_str).unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0));

                Some(TorrentSearchResult {
                    id,
                    name: t.name,
                    size,
                    seeders,
                    leechers,
                    info_hash: t.info_hash.filter(|s| !s.is_empty()),
                })
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

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl MTeamAdapter {
    /// Generate a download token and fetch the actual torrent file bytes.
    async fn download_torrent_file(&self, torrent_id: i64) -> Result<Vec<u8>, CoreError> {
        // Step 1: generate download token
        let token_body = serde_json::json!({});
        let path = format!("/api/torrent/genDlToken?id={}", torrent_id);

        let token_data: DlTokenData = self.api_post(&path, &token_body).await?;

        let download_url = token_data
            .download_url
            .or_else(|| {
                token_data.token.map(|tok| {
                    format!(
                        "{}/download.php?id={}&token={}",
                        self.base_url, torrent_id, tok
                    )
                })
            })
            .ok_or_else(|| {
                SiteError::ParseError("genDlToken returned neither downloadUrl nor token".into())
            })?;

        debug!(site = %self.name, torrent_id, "downloading torrent file");

        // Step 2: download the actual file
        let resp =
            self.client.get(&download_url).send().await.map_err(|e| {
                SiteError::HttpError(format!("failed to download torrent file: {e}"))
            })?;

        if !resp.status().is_success() {
            return Err(SiteError::HttpError(format!(
                "HTTP {} downloading torrent file",
                resp.status()
            ))
            .into());
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| SiteError::HttpError(format!("failed to read torrent file bytes: {e}")))?;

        Ok(bytes.to_vec())
    }
}
