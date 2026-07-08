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
// Adapter struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Unit3dAdapter {
    name: String,
    base_url: String,
    api_token: Option<String>,
    passkey: Option<String>,
    client: Client,
    batch_size: usize,
}

// ---------------------------------------------------------------------------
// Unit3D API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Unit3dListResponse<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct Unit3dSingleResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct Unit3dTorrentResource {
    id: i64,
    attributes: Unit3dTorrentAttributes,
}

#[derive(Debug, Deserialize)]
struct Unit3dTorrentAttributes {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mediainfo: Option<String>,
    #[serde(default)]
    category_id: Option<i64>,
    #[serde(default)]
    type_id: Option<i64>,
    #[serde(default)]
    resolution_id: Option<i64>,
    #[serde(default)]
    tmdb_id: Option<i64>,
    #[serde(default)]
    imdb_id: Option<i64>,
    size: u64,
    #[serde(default)]
    files_count: Option<u64>,
    #[serde(default)]
    seeders: u32,
    #[serde(default)]
    leechers: u32,
    #[serde(default)]
    info_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Unit3dUserResource {
    #[allow(dead_code)]
    id: i64,
    attributes: Unit3dUserAttributes,
}

#[derive(Debug, Deserialize)]
struct Unit3dUserAttributes {
    username: String,
    #[serde(default)]
    uploaded: Option<i64>,
    #[serde(default)]
    downloaded: Option<i64>,
    #[serde(default)]
    ratio: Option<f64>,
    #[serde(default)]
    seedbonus: Option<f64>,
    #[serde(default)]
    group: Option<Unit3dGroup>,
    #[serde(default)]
    seeding_count: Option<i64>,
    #[serde(default)]
    leeching_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Unit3dGroup {
    #[serde(default)]
    name: Option<String>,
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

impl Unit3dAdapter {
    pub fn new(
        name: String,
        base_url: String,
        api_token: Option<String>,
        passkey: Option<String>,
        batch_size: usize,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("PT-Reseeder/0.1"));

        let client = Client::builder()
            .use_rustls_tls()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()
            .expect("failed to build reqwest client");

        Self {
            name,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_token,
            passkey,
            client,
            batch_size,
        }
    }

    /// Build a URL with the API token appended as a query parameter.
    fn api_url(&self, path: &str) -> Result<String, CoreError> {
        let token = self
            .api_token
            .as_deref()
            .ok_or_else(|| SiteError::AuthFailed("no API token configured".into()))?;
        let separator = if path.contains('?') { '&' } else { '?' };
        Ok(format!(
            "{}{}{separator}api_token={token}",
            self.base_url, path
        ))
    }

    /// Send a GET request to the Unit3D API and return the response body text.
    async fn api_get(&self, path: &str) -> Result<String, CoreError> {
        let url = self.api_url(path)?;
        debug!(site = %self.name, path, "Unit3D API GET");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(SiteError::AuthFailed(format!("HTTP {status}")).into());
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SiteError::NotFound(format!("HTTP {status} for {path}")).into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SiteError::RateLimited.into());
        }
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status}")).into());
        }

        resp.text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()).into())
    }
}

// ---------------------------------------------------------------------------
// SiteCore
// ---------------------------------------------------------------------------

impl SiteCore for Unit3dAdapter {
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
impl ReseedCapable for Unit3dAdapter {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        // Unit3D does not expose a native pieces-hash query API.
        // Matching is handled externally via Jackett / search fallback.
        debug!(
            site = %self.name,
            hash_count = hashes.len(),
            "Unit3D has no native pieces-hash API; returning empty result set"
        );
        Ok(Vec::new())
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        // Prefer passkey-based URL when available; fall back to API-token URL.
        if let Some(ref pk) = self.passkey {
            format!("{}/torrent/download/{}.{}", self.base_url, torrent_id, pk)
        } else if let Some(ref token) = self.api_token {
            format!(
                "{}/api/torrents/{}/download?api_token={}",
                self.base_url, torrent_id, token
            )
        } else {
            // Best-effort URL without auth (will likely fail at download time).
            format!("{}/torrent/download/{}", self.base_url, torrent_id)
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
impl UserInfoCapable for Unit3dAdapter {
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError> {
        let body = self.api_get("/api/user").await?;

        let parsed: Unit3dSingleResponse<Unit3dUserResource> =
            serde_json::from_str(&body).map_err(|e| {
                SiteError::ParseError(format!("failed to parse user info response: {e}"))
            })?;

        let attrs = &parsed.data.attributes;

        let ratio = attrs.ratio.or_else(|| {
            match (attrs.uploaded, attrs.downloaded) {
                (Some(up), Some(down)) if down > 0 => Some(up as f64 / down as f64),
                (Some(_), Some(0)) => Some(f64::INFINITY),
                _ => None,
            }
        });

        debug!(
            site = %self.name,
            username = %attrs.username,
            "user info fetched successfully"
        );

        Ok(UserStats {
            site_id: SiteId(0),
            uploaded: attrs.uploaded,
            downloaded: attrs.downloaded,
            ratio,
            bonus: attrs.seedbonus,
            user_class: attrs
                .group
                .as_ref()
                .and_then(|g| g.name.clone()),
            seeding_count: attrs.seeding_count,
            leeching_count: attrs.leeching_count,
            seeding_size: None,
            upload_time_seconds: None,
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
        })
    }

    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError> {
        // Unit3D does not expose a passkey via its REST API.
        // Return the locally configured passkey if any.
        if self.passkey.is_some() {
            debug!(site = %self.name, "returning locally configured passkey");
            return Ok(self.passkey.clone());
        }
        warn!(site = %self.name, "no passkey configured and Unit3D API does not expose one");
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// RepostCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl RepostCapable for Unit3dAdapter {
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        let path = format!("/api/torrents/{torrent_id}");
        let body = self.api_get(&path).await?;

        let parsed: Unit3dSingleResponse<Unit3dTorrentResource> =
            serde_json::from_str(&body).map_err(|e| {
                SiteError::ParseError(format!("failed to parse torrent detail response: {e}"))
            })?;

        let attrs = &parsed.data.attributes;

        // Build an IMDB URL from the numeric ID if present.
        let imdb_url = attrs
            .imdb_id
            .filter(|id| *id > 0)
            .map(|id| format!("https://www.imdb.com/title/tt{:07}", id));

        // Build a TMDB URL from the numeric ID if present (stored for reference in descr).
        let tmdb_url = attrs
            .tmdb_id
            .filter(|id| *id > 0)
            .map(|id| format!("https://www.themoviedb.org/movie/{}", id));

        // Compose a description from the API fields.
        let mut descr = attrs.description.clone().unwrap_or_default();
        if let Some(ref tmdb) = tmdb_url {
            if !descr.contains(tmdb) {
                descr.push_str(&format!("\n\nTMDB: {tmdb}"));
            }
        }

        let torrent_type = attrs
            .type_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        let resolution = attrs
            .resolution_id
            .map(|id| id.to_string())
            .unwrap_or_default();

        let source_url = format!("{}/torrents/{}", self.base_url, torrent_id);

        debug!(
            site = %self.name,
            torrent_id,
            name = %attrs.name,
            "torrent detail extracted"
        );

        // Download the .torrent file.
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
            name: attrs.name.clone(),
            small_descr: String::new(),
            descr,
            imdb_url,
            douban_url: None,
            mediainfo: attrs.mediainfo.clone(),
            images: Vec::new(),
            torrent_type,
            region: String::new(),
            resolution,
            video_codec: String::new(),
            audio_codec: String::new(),
            medium: String::new(),
            source_site: self.name.clone(),
            source_url,
            torrent_file_data,
        })
    }

    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        let url = self.api_url("/api/torrents/upload")?;

        debug!(
            site = %self.name,
            name = %info.name,
            "submitting torrent via Unit3D API"
        );

        let mut form = reqwest::multipart::Form::new()
            .text("name", info.name.clone())
            .text("description", info.descr.clone());

        if let Some(ref imdb) = info.imdb_url {
            // Extract numeric IMDB ID (e.g. "tt1234567" -> "1234567").
            let imdb_num = imdb
                .rsplit('/')
                .find(|s| s.starts_with("tt"))
                .map(|s| s.trim_start_matches("tt").trim_end_matches('/'))
                .unwrap_or("");
            if !imdb_num.is_empty() {
                form = form.text("imdb", imdb_num.to_string());
            }
        }

        if let Some(ref mi) = info.mediainfo {
            form = form.text("mediainfo", mi.clone());
        }
        if let Some(cat_id) = info.category_id {
            form = form.text("category_id", cat_id.to_string());
        }
        if let Some(source_id) = info.source_id {
            form = form.text("type_id", source_id.to_string());
        }
        if let Some(res_id) = info.resolution_id {
            form = form.text("resolution_id", res_id.to_string());
        }

        if let Some(ref data) = info.torrent_file_data {
            let part = reqwest::multipart::Part::bytes(data.clone())
                .file_name("torrent.torrent")
                .mime_str("application/x-bittorrent")
                .map_err(|e| SiteError::HttpError(e.to_string()))?;
            form = form.part("torrent", part);
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(SiteError::AuthFailed(format!("HTTP {status} on upload")).into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SiteError::RateLimited.into());
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Unit3D returns the created torrent resource on success (HTTP 200/201).
        if status.is_success() {
            // Try to extract the new torrent ID from the response.
            if let Ok(parsed) =
                serde_json::from_str::<Unit3dSingleResponse<Unit3dTorrentResource>>(&body)
            {
                let new_url = format!("{}/torrents/{}", self.base_url, parsed.data.id);
                debug!(site = %self.name, url = %new_url, "torrent submitted successfully");
                return Ok(new_url);
            }
            // Fallback: return the raw body as confirmation.
            debug!(site = %self.name, "torrent submitted (could not parse response for ID)");
            return Ok(format!("{}/torrents", self.base_url));
        }

        Err(
            SiteError::HttpError(format!("upload failed: HTTP {status}, body={body}")).into(),
        )
    }
}

// ---------------------------------------------------------------------------
// SearchCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl SearchCapable for Unit3dAdapter {
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError> {
        let encoded = urlencoding::encode(query);
        let path = format!("/api/torrents?name={encoded}");
        let body = self.api_get(&path).await?;

        let parsed: Unit3dListResponse<Unit3dTorrentResource> =
            serde_json::from_str(&body).map_err(|e| {
                SiteError::ParseError(format!("failed to parse search response: {e}"))
            })?;

        let mut results: Vec<TorrentSearchResult> = parsed
            .data
            .into_iter()
            .map(|t| TorrentSearchResult {
                id: t.id,
                name: t.attributes.name,
                size: t.attributes.size,
                seeders: t.attributes.seeders,
                leechers: t.attributes.leechers,
                info_hash: t.attributes.info_hash,
            })
            .collect();

        // Filter by size hint with +-1% tolerance.
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
