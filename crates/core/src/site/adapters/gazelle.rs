use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::{CoreError, SiteError};
use crate::site::models::*;
use crate::site::traits::*;

#[derive(Clone)]
pub struct GazelleAdapter {
    name: String,
    base_url: String,
    cookie: Option<String>,
    passkey: Option<String>,
    client: Client,
    batch_size: usize,
}

// ---------------------------------------------------------------------------
// Gazelle JSON API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GazelleApiResponse<T> {
    status: String,
    response: Option<T>,
}

#[derive(Deserialize)]
struct GazelleBrowseResponse {
    #[serde(default)]
    results: Vec<GazelleGroupResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GazelleGroupResult {
    group_id: i64,
    group_name: String,
    #[serde(default)]
    torrents: Vec<GazelleTorrentEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GazelleTorrentEntry {
    torrent_id: i64,
    size: u64,
    seeders: u32,
    leechers: u32,
    info_hash: Option<String>,
}

#[derive(Deserialize)]
struct GazelleTorrentDetailResponse {
    group: GazelleTorrentGroup,
    torrent: GazelleTorrentDetail,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GazelleTorrentGroup {
    name: String,
    #[serde(default)]
    wiki_body: String,
    #[serde(default)]
    music_info: Option<serde_json::Value>,
    #[serde(default)]
    category_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GazelleTorrentDetail {
    id: i64,
    size: u64,
    #[serde(default)]
    info_hash: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    media_info: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
}

#[derive(Deserialize)]
struct GazelleIndexResponse {
    id: i64,
    username: String,
    #[serde(default)]
    stats: Option<GazelleUserStats>,
    #[serde(default)]
    community: Option<GazelleCommunityStats>,
}

#[derive(Deserialize)]
struct GazelleUserStats {
    #[serde(default)]
    uploaded: Option<i64>,
    #[serde(default)]
    downloaded: Option<i64>,
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default)]
    bonus: Option<f64>,
}

#[derive(Deserialize)]
struct GazelleCommunityStats {
    #[serde(default)]
    seeding: Option<i64>,
    #[serde(default)]
    leeching: Option<i64>,
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

impl GazelleAdapter {
    pub fn new(
        name: String,
        base_url: String,
        cookie: Option<String>,
        passkey: Option<String>,
        batch_size: usize,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("PT-Reseeder/0.1"));
        if let Some(ref c) = cookie {
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
            cookie,
            passkey,
            client,
            batch_size,
        }
    }

    /// Returns the configured batch size for hash queries.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Make an authenticated GET request to the Gazelle JSON API.
    async fn api_get<T: serde::de::DeserializeOwned>(
        &self,
        action: &str,
        extra_params: &[(&str, &str)],
    ) -> Result<T, CoreError> {
        let mut url = format!("{}/ajax.php?action={}", self.base_url, action);
        for (k, v) in extra_params {
            url.push_str(&format!("&{}={}", k, urlencoding::encode(v)));
        }

        debug!(site = %self.name, action, "gazelle API request");

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
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from ajax.php")).into());
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let api_resp: GazelleApiResponse<T> = serde_json::from_str(&body)
            .map_err(|e| SiteError::ParseError(format!("failed to parse gazelle response: {e}")))?;

        if api_resp.status != "success" {
            return Err(
                SiteError::HttpError(format!("API returned status: {}", api_resp.status)).into(),
            );
        }

        api_resp
            .response
            .ok_or_else(|| SiteError::ParseError("API response field is null".into()).into())
    }

    /// Extract images from the wiki body HTML/BBCode description.
    fn extract_images_from_body(body: &str) -> Vec<String> {
        let mut images = Vec::new();
        // Match [img]url[/img] BBCode
        let mut search = body;
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
        // Also try <img src="..."> HTML
        let mut search = body;
        while let Some(start) = search.find("<img") {
            let after = &search[start..];
            if let Some(src_start) = after.find("src=\"") {
                let url_start = &after[src_start + 5..];
                if let Some(src_end) = url_start.find('"') {
                    let url = &url_start[..src_end];
                    if !url.is_empty() {
                        images.push(url.to_string());
                    }
                    search = &url_start[src_end..];
                } else {
                    break;
                }
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

impl SiteCore for GazelleAdapter {
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
impl ReseedCapable for GazelleAdapter {
    async fn query_pieces_hash(&self, _hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        // Gazelle framework does not provide a native pieces-hash query API.
        // Reseed matching must rely on search + size matching instead.
        debug!(
            site = %self.name,
            "gazelle has no pieces_hash API, returning empty"
        );
        Ok(Vec::new())
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        let mut url = format!(
            "{}/torrents.php?action=download&id={}",
            self.base_url, torrent_id
        );
        if let Some(ref pk) = self.passkey {
            url.push_str(&format!("&torrent_pass={pk}"));
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
impl UserInfoCapable for GazelleAdapter {
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        debug!(site = %self.name, "fetching user info via ajax.php?action=index");

        let info: GazelleIndexResponse = self.api_get("index", &[]).await?;

        let (uploaded, downloaded, ratio, bonus) = match info.stats {
            Some(s) => (s.uploaded, s.downloaded, s.ratio, s.bonus),
            None => (None, None, None, None),
        };

        let (seeding_count, leeching_count) = match info.community {
            Some(c) => (c.seeding, c.leeching),
            None => (None, None),
        };

        debug!(
            site = %self.name,
            user_id = info.id,
            username = %info.username,
            "user info fetched successfully"
        );

        Ok(UserStats {
            site_id: SiteId(info.id),
            uploaded,
            downloaded,
            ratio,
            bonus,
            user_class: None,
            seeding_count,
            leeching_count,
            seeding_size: None,
            upload_time_seconds: None,
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
        })
    }

    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError> {
        // Passkey is typically provided in config for Gazelle sites; if we have
        // one already, return it.  Otherwise there is no standard API to fetch it.
        if self.passkey.is_some() {
            return Ok(self.passkey.clone());
        }

        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        // Try to scrape from user settings page
        let url = format!("{}/user.php?action=edit", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} fetching settings", resp.status())).into(),
            );
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Look for passkey/torrent_pass in the page
        let re = regex_lite::Regex::new(r"torrent_pass=([a-f0-9]{32,64})").ok();
        if let Some(re) = re {
            if let Some(caps) = re.captures(&body) {
                if let Some(m) = caps.get(1) {
                    debug!(site = %self.name, "passkey found via regex");
                    return Ok(Some(m.as_str().to_string()));
                }
            }
        }

        warn!(site = %self.name, "passkey not found on user settings page");
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// RepostCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl RepostCapable for GazelleAdapter {
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        debug!(site = %self.name, torrent_id, "extracting torrent detail via ajax.php?action=torrent");

        let detail: GazelleTorrentDetailResponse =
            self.api_get("torrent", &[("id", torrent_id)]).await?;

        let name = detail.group.name.clone();
        let descr = detail.torrent.description.clone();
        let wiki_body = detail.group.wiki_body.clone();
        let mediainfo = detail.torrent.media_info.clone();
        let images = Self::extract_images_from_body(&wiki_body);

        // Build small description from file path or torrent info
        let small_descr = detail.torrent.file_path.clone().unwrap_or_default();

        // Determine category from music_info if present
        let torrent_type = detail
            .group
            .category_id
            .map(|c| c.to_string())
            .unwrap_or_default();

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
            imdb_url: None,
            douban_url: None,
            mediainfo,
            images,
            torrent_type,
            region: String::new(),
            resolution: String::new(),
            video_codec: String::new(),
            audio_codec: String::new(),
            medium: String::new(),
            source_site: self.name.clone(),
            source_url: format!("{}/torrents.php?torrentid={}", self.base_url, torrent_id),
            torrent_file_data,
        })
    }

    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        let url = format!("{}/upload.php", self.base_url);
        debug!(
            site = %self.name,
            name = %info.name,
            "submitting torrent (cookie=[REDACTED])"
        );

        let mut form = reqwest::multipart::Form::new()
            .text("title", info.name.clone())
            .text("desc", info.descr.clone());

        if let Some(ref imdb) = info.imdb_url {
            form = form.text("imdb", imdb.clone());
        }
        if let Some(ref mi) = info.mediainfo {
            form = form.text("mediainfo", mi.clone());
        }
        if let Some(cat_id) = info.category_id {
            form = form.text("type", cat_id.to_string());
        }

        if let Some(ref data) = info.torrent_file_data {
            let part = reqwest::multipart::Part::bytes(data.clone())
                .file_name("torrent.torrent")
                .mime_str("application/x-bittorrent")
                .map_err(|e| SiteError::HttpError(e.to_string()))?;
            form = form.part("file_input", part);
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let final_url = resp.url().to_string();
        let status = resp.status();
        let _body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Gazelle redirects to the torrent page on success
        if final_url.contains("torrents.php") && final_url.contains("torrentid=") {
            debug!(site = %self.name, url = %final_url, "torrent submitted successfully (redirect)");
            return Ok(final_url);
        }

        if status.is_success() || status.is_redirection() {
            debug!(site = %self.name, url = %final_url, "torrent submitted (status={status})");
            return Ok(final_url);
        }

        Err(SiteError::HttpError(format!(
            "upload may have failed: HTTP {status}, url={final_url}"
        ))
        .into())
    }
}

// ---------------------------------------------------------------------------
// SearchCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl SearchCapable for GazelleAdapter {
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        debug!(site = %self.name, query, "searching torrents via ajax.php?action=browse");

        let browse: GazelleBrowseResponse = self.api_get("browse", &[("searchstr", query)]).await?;

        let mut results = Vec::new();

        for group in browse.results {
            for torrent in group.torrents {
                results.push(TorrentSearchResult {
                    id: torrent.torrent_id,
                    name: group.group_name.clone(),
                    size: torrent.size,
                    seeders: torrent.seeders,
                    leechers: torrent.leechers,
                    info_hash: torrent.info_hash.clone(),
                });
            }
        }

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
