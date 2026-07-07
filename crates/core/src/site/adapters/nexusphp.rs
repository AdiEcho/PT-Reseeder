use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::{CoreError, SiteError};
use crate::site::models::*;
use crate::site::traits::*;

#[derive(Clone)]
pub struct NexusPhpAdapter {
    name: String,
    base_url: String,
    api_url: Option<String>,
    cookie: Option<String>,
    passkey: Option<String>,
    user_id: Option<String>,
    selectors: UserInfoSelectors,
    client: Client,
    batch_size: usize,
}

#[derive(Deserialize)]
struct PiecesHashResponse {
    #[serde(default)]
    data: Vec<PiecesHashMatch>,
}

#[derive(Deserialize)]
struct PiecesHashMatch {
    pieces_hash: String,
    torrent_id: i64,
}

impl NexusPhpAdapter {
    pub fn new(
        name: String,
        base_url: String,
        api_url: Option<String>,
        cookie: Option<String>,
        passkey: Option<String>,
        user_id: Option<String>,
        selectors: UserInfoSelectors,
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
            api_url,
            cookie,
            passkey,
            user_id,
            selectors,
            client,
            batch_size,
        }
    }

    /// Returns the configured batch size for hash queries.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn resolve_user_id(&self) -> Result<String, CoreError> {
        if let Some(user_id) = self.user_id.as_ref().filter(|id| !id.trim().is_empty()) {
            return Ok(user_id.clone());
        }

        let resp = self
            .client
            .get(&self.base_url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SiteError::HttpError(format!(
                "HTTP {} resolving user id",
                resp.status()
            ))
            .into());
        }
        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        let html = Html::parse_document(&body);
        extract_user_id(&html, &self.selectors)
            .ok_or_else(|| SiteError::AuthFailed("failed to resolve user_id from cookie session".into()).into())
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn extract_text(html: &Html, selector_str: &Option<String>) -> Option<String> {
    let sel_str = selector_str.as_deref()?;
    let selector = Selector::parse(sel_str).ok()?;
    let element = html.select(&selector).next()?;
    let text: String = element
        .text()
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_user_id_from_href(value: &str) -> Option<String> {
    let marker = "userdetails.php?id=";
    let start = value.find(marker)? + marker.len();
    let id: String = value[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    (!id.is_empty()).then_some(id)
}

fn extract_user_id(html: &Html, selectors: &UserInfoSelectors) -> Option<String> {
    if let Some(selector_str) = selectors.uid_selector.as_deref() {
        if let Ok(selector) = Selector::parse(selector_str) {
            for element in html.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    if let Some(id) = extract_user_id_from_href(href) {
                        return Some(id);
                    }
                }
                let text = element.text().collect::<Vec<_>>().join("");
                let id: String = text.chars().filter(|ch| ch.is_ascii_digit()).collect();
                if !id.is_empty() {
                    return Some(id);
                }
            }
        }
    }

    if let Ok(selector) = Selector::parse("a[href*='userdetails.php?id=']") {
        for element in html.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(id) = extract_user_id_from_href(href) {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn parse_size_to_bytes(text: &str) -> Option<i64> {
    let text = text.trim();
    // Try to split into numeric part and unit
    let mut num_end = 0;
    for (i, ch) in text.char_indices() {
        if ch.is_ascii_digit() || ch == '.' {
            num_end = i + ch.len_utf8();
        } else if !ch.is_ascii_whitespace() {
            break;
        }
    }

    let num_str = text[..num_end].trim();
    let unit_str = text[num_end..].trim().to_uppercase();

    let value: f64 = num_str.parse().ok()?;

    let multiplier: f64 = match unit_str.as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1024.0,
        "MB" | "MIB" => 1024.0 * 1024.0,
        "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "PB" | "PIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };

    Some((value * multiplier) as i64)
}

fn parse_ratio(text: &str) -> Option<f64> {
    let text = text.trim();
    if text == "∞" || text.eq_ignore_ascii_case("inf") || text.eq_ignore_ascii_case("infinite") {
        return Some(f64::INFINITY);
    }
    // Strip commas, e.g. "1,234.56"
    let cleaned: String = text.chars().filter(|c| *c != ',').collect();
    cleaned.parse::<f64>().ok()
}

fn parse_time_to_seconds(text: &str) -> Option<i64> {
    let text = text.trim();
    let mut total_seconds: i64 = 0;
    let mut current_num = String::new();

    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else if !current_num.is_empty() {
            let num: i64 = current_num.parse().ok()?;
            current_num.clear();

            match ch {
                '年' => total_seconds += num * 365 * 24 * 3600,
                '月' => total_seconds += num * 30 * 24 * 3600,
                '周' => total_seconds += num * 7 * 24 * 3600,
                '天' | '日' => total_seconds += num * 24 * 3600,
                '时' | '時' => total_seconds += num * 3600,
                '分' => total_seconds += num * 60,
                '秒' => total_seconds += num,
                _ => {
                    // Handle English units by collecting the rest of the word
                    let mut unit = String::new();
                    unit.push(ch);
                    // We'll handle English below after collecting the full word
                    // For now, store num back for the English path
                    current_num = num.to_string();
                    current_num.push('\0'); // sentinel
                    unit.clear();
                }
            }
        }
    }

    // Fallback: try English pattern "X days Y hours Z minutes"
    if total_seconds == 0 {
        let lower = text.to_lowercase();
        let tokens: Vec<&str> = lower.split_whitespace().collect();
        let mut i = 0;
        while i + 1 < tokens.len() {
            if let Ok(num) = tokens[i].parse::<i64>() {
                let unit = tokens[i + 1];
                if unit.starts_with("year") {
                    total_seconds += num * 365 * 24 * 3600;
                } else if unit.starts_with("month") {
                    total_seconds += num * 30 * 24 * 3600;
                } else if unit.starts_with("week") {
                    total_seconds += num * 7 * 24 * 3600;
                } else if unit.starts_with("day") {
                    total_seconds += num * 24 * 3600;
                } else if unit.starts_with("hour") {
                    total_seconds += num * 3600;
                } else if unit.starts_with("min") {
                    total_seconds += num * 60;
                } else if unit.starts_with("sec") {
                    total_seconds += num;
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    if total_seconds > 0 {
        Some(total_seconds)
    } else {
        None
    }
}

fn parse_number_from_text(text: &str) -> Option<i64> {
    let cleaned: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned.parse::<i64>().ok()
}

fn parse_f64_from_text(text: &str) -> Option<f64> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// SiteCore
// ---------------------------------------------------------------------------

impl SiteCore for NexusPhpAdapter {
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
impl ReseedCapable for NexusPhpAdapter {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        let api_url = self
            .api_url
            .as_deref()
            .ok_or_else(|| SiteError::AuthFailed("no API URL configured".into()))?;
        let passkey = self
            .passkey
            .as_deref()
            .ok_or_else(|| SiteError::AuthFailed("no passkey configured".into()))?;

        debug!(
            site = %self.name,
            hash_count = hashes.len(),
            "querying pieces hashes against API (passkey=[REDACTED])"
        );

        let body = serde_json::json!({
            "passkey": passkey,
            "pieces_hash": hashes,
        });

        let resp = self
            .client
            .post(api_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(SiteError::HttpError(format!("HTTP {status} from pieces_hash API")).into());
        }

        let parsed: PiecesHashResponse = resp.json().await.map_err(|e| {
            SiteError::ParseError(format!("failed to parse pieces_hash response: {e}"))
        })?;

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
        let mut url = format!("{}/download.php?id={}", self.base_url, torrent_id);
        if let Some(ref pk) = self.passkey {
            url.push_str(&format!("&passkey={pk}"));
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
impl UserInfoCapable for NexusPhpAdapter {
    async fn fetch_user_info(&self) -> Result<UserStats, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }
        let user_id = self.resolve_user_id().await?;

        let url = format!("{}/userdetails.php?id={}", self.base_url, user_id);
        debug!(site = %self.name, "fetching user info (cookie=[REDACTED])");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} fetching user info", resp.status())).into(),
            );
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        let html = Html::parse_document(&body);

        let uploaded = extract_text(&html, &self.selectors.uploaded_selector)
            .and_then(|t| parse_size_to_bytes(&t));
        let downloaded = extract_text(&html, &self.selectors.downloaded_selector)
            .and_then(|t| parse_size_to_bytes(&t));
        let ratio =
            extract_text(&html, &self.selectors.ratio_selector).and_then(|t| parse_ratio(&t));
        let bonus = extract_text(&html, &self.selectors.bonus_selector)
            .and_then(|t| parse_f64_from_text(&t));
        let user_class = extract_text(&html, &self.selectors.user_class_selector);
        let seeding_count = extract_text(&html, &self.selectors.seeding_count_selector)
            .and_then(|t| parse_number_from_text(&t));
        let leeching_count = extract_text(&html, &self.selectors.leeching_count_selector)
            .and_then(|t| parse_number_from_text(&t));
        let seeding_size = extract_text(&html, &self.selectors.seeding_size_selector)
            .and_then(|t| parse_size_to_bytes(&t));
        let upload_time_seconds = extract_text(&html, &self.selectors.upload_time_selector)
            .and_then(|t| parse_time_to_seconds(&t));

        debug!(site = %self.name, "user info fetched successfully");

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
            upload_time_seconds,
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
        })
    }

    async fn fetch_passkey(&self) -> Result<Option<String>, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }
        let user_id = self.resolve_user_id().await?;

        let url = format!("{}/userdetails.php?id={}", self.base_url, user_id);
        debug!(site = %self.name, "fetching passkey (cookie=[REDACTED])");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} fetching passkey", resp.status())).into(),
            );
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        let html = Html::parse_document(&body);

        // Try input[name='passkey'] first
        if let Ok(sel) = Selector::parse("input[name='passkey']") {
            if let Some(element) = html.select(&sel).next() {
                if let Some(val) = element.value().attr("value") {
                    let val = val.trim();
                    if !val.is_empty() {
                        debug!(site = %self.name, "passkey found via input selector");
                        return Ok(Some(val.to_string()));
                    }
                }
            }
        }

        // Fallback: regex pattern in page body
        let re = regex_lite::Regex::new(r"passkey=([a-f0-9]{32,64})").ok();
        if let Some(re) = re {
            if let Some(caps) = re.captures(&body) {
                if let Some(m) = caps.get(1) {
                    debug!(site = %self.name, "passkey found via regex");
                    return Ok(Some(m.as_str().to_string()));
                }
            }
        }

        warn!(site = %self.name, "passkey not found on user page");
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// RepostCapable
// ---------------------------------------------------------------------------

#[async_trait]
impl RepostCapable for NexusPhpAdapter {
    async fn extract_torrent_detail(&self, torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        let url = format!("{}/details.php?id={}", self.base_url, torrent_id);
        debug!(site = %self.name, torrent_id, "extracting torrent detail (cookie=[REDACTED])");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(SiteError::HttpError(format!(
                "HTTP {} fetching torrent detail",
                resp.status()
            ))
            .into());
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        let (
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
        ) = {
            let html = Html::parse_document(&body);

            // Name: try h1, then title tag
            let name = extract_by_selector(&html, "h1")
                .or_else(|| extract_by_selector(&html, "title"))
                .unwrap_or_default();

            // Subtitle
            let small_descr = extract_by_selector(&html, "span.subtitle")
                .or_else(|| extract_by_selector(&html, "td.rowfollow:nth-child(2)"))
                .unwrap_or_default();

            // Description (BBCode): try textarea#descr, then div#kdescr
            let descr = extract_by_selector(&html, "textarea#descr")
                .or_else(|| extract_by_selector(&html, "#kdescr"))
                .unwrap_or_default();

            // IMDb URL
            let imdb_url = extract_link_href(&html, r"imdb.com/title/");

            // Douban URL
            let douban_url = extract_link_href(&html, r"douban.com/subject/");

            // Mediainfo: from pre or code block
            let mediainfo = extract_by_selector(&html, "pre#mediainfo")
                .or_else(|| extract_by_selector(&html, "div.mediainfo pre"))
                .or_else(|| extract_by_selector(&html, "pre"));

            // Images from description area
            let images = extract_images(&html, "#kdescr img, .bbcodeimage");

            // Category fields - extract from select elements or text cells
            let torrent_type =
                extract_selected_option(&html, "select[name='type'] option[selected]")
                    .or_else(|| extract_by_selector(&html, "span#type"))
                    .unwrap_or_default();
            let region =
                extract_selected_option(&html, "select[name='source_sel'] option[selected]")
                    .unwrap_or_default();
            let resolution =
                extract_selected_option(&html, "select[name='standard_sel'] option[selected]")
                    .unwrap_or_default();
            let video_codec =
                extract_selected_option(&html, "select[name='codec_sel'] option[selected]")
                    .unwrap_or_default();
            let audio_codec =
                extract_selected_option(&html, "select[name='audiocodec_sel'] option[selected]")
                    .unwrap_or_default();
            let medium =
                extract_selected_option(&html, "select[name='medium_sel'] option[selected]")
                    .unwrap_or_default();

            (
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
            )
        };

        debug!(site = %self.name, torrent_id, name = %name, "torrent detail extracted");

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
            source_url: url,
            torrent_file_data,
        })
    }

    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        let url = format!("{}/takeupload.php", self.base_url);
        debug!(
            site = %self.name,
            name = %info.name,
            "submitting torrent (cookie=[REDACTED])"
        );

        let mut form = reqwest::multipart::Form::new()
            .text("name", info.name.clone())
            .text("small_descr", info.small_descr.clone())
            .text("descr", info.descr.clone());

        if let Some(ref imdb) = info.imdb_url {
            form = form.text("url", imdb.clone());
        }
        if let Some(ref douban) = info.douban_url {
            form = form.text("doubanurl", douban.clone());
        }
        if let Some(ref mi) = info.mediainfo {
            form = form.text("mediainfo", mi.clone());
        }
        if let Some(cat_id) = info.category_id {
            form = form.text("type", cat_id.to_string());
        }
        if let Some(source_id) = info.source_id {
            form = form.text("source_sel", source_id.to_string());
        }
        if let Some(codec_id) = info.codec_id {
            form = form.text("codec_sel", codec_id.to_string());
        }
        if let Some(res_id) = info.resolution_id {
            form = form.text("standard_sel", res_id.to_string());
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

        let final_url = resp.url().to_string();
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        // Success: redirected to details page or body contains success message
        if final_url.contains("details.php") {
            debug!(site = %self.name, url = %final_url, "torrent submitted successfully (redirect)");
            return Ok(final_url);
        }
        if body.contains("种子已成功上传") || body.contains("Torrent uploaded successfully")
        {
            debug!(site = %self.name, "torrent submitted successfully (message)");
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
impl SearchCapable for NexusPhpAdapter {
    async fn search_torrents(
        &self,
        query: &str,
        size_hint: Option<u64>,
    ) -> Result<Vec<TorrentSearchResult>, CoreError> {
        if self.cookie.is_none() {
            return Err(SiteError::AuthFailed("no cookie configured".into()).into());
        }

        let url = format!(
            "{}/torrents.php?search={}&notnewword=1",
            self.base_url,
            urlencoding::encode(query)
        );
        debug!(site = %self.name, query, "searching torrents (cookie=[REDACTED])");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} searching torrents", resp.status())).into(),
            );
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        let html = Html::parse_document(&body);

        let row_sel = Selector::parse("table.torrents tr:not(:first-child)").map_err(|_| {
            SiteError::ParseError("failed to parse torrent table row selector".into())
        })?;

        let link_sel = Selector::parse("a[href*='details.php']")
            .map_err(|_| SiteError::ParseError("failed to parse details link selector".into()))?;
        let td_sel = Selector::parse("td")
            .map_err(|_| SiteError::ParseError("failed to parse td selector".into()))?;

        let mut results = Vec::new();

        for row in html.select(&row_sel) {
            // Extract torrent ID from the details link
            let id = row
                .select(&link_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .and_then(|href| {
                    href.split("id=")
                        .nth(1)
                        .and_then(|s| s.split('&').next())
                        .and_then(|s| s.parse::<i64>().ok())
                });

            let id = match id {
                Some(id) => id,
                None => continue,
            };

            // Extract name from the details link
            let name = row
                .select(&link_sel)
                .next()
                .map(|a| a.text().collect::<Vec<_>>().join("").trim().to_string())
                .unwrap_or_default();

            if name.is_empty() {
                continue;
            }

            // Extract other fields from td cells
            let tds: Vec<_> = row.select(&td_sel).collect();

            let mut size: u64 = 0;
            let mut seeders: u32 = 0;
            let mut leechers: u32 = 0;

            // NexusPHP table layout varies but commonly:
            // size is often in td with class containing "rowfollow" with size text
            // seeders/leechers are in the last few columns
            for td in &tds {
                let text: String = td.text().collect::<Vec<_>>().join("").trim().to_string();

                // Try to identify size column (contains unit like GB, MB, TB)
                if size == 0 {
                    if let Some(bytes) = parse_size_to_bytes(&text) {
                        if bytes > 0 {
                            size = bytes as u64;
                        }
                    }
                }
            }

            // Seeders and leechers are typically in the last columns
            if tds.len() >= 2 {
                let seeders_text: String = tds[tds.len() - 2]
                    .text()
                    .collect::<Vec<_>>()
                    .join("")
                    .trim()
                    .to_string();
                let leechers_text: String = tds[tds.len() - 1]
                    .text()
                    .collect::<Vec<_>>()
                    .join("")
                    .trim()
                    .to_string();
                seeders = seeders_text.parse().unwrap_or(0);
                leechers = leechers_text.parse().unwrap_or(0);
            }

            results.push(TorrentSearchResult {
                id,
                name,
                size,
                seeders,
                leechers,
                info_hash: None,
            });
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

// ---------------------------------------------------------------------------
// HTML extraction helpers
// ---------------------------------------------------------------------------

fn extract_by_selector(html: &Html, selector_str: &str) -> Option<String> {
    let sel = Selector::parse(selector_str).ok()?;
    let el = html.select(&sel).next()?;
    let text: String = el.text().collect::<Vec<_>>().join("").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_selected_option(html: &Html, selector_str: &str) -> Option<String> {
    let sel = Selector::parse(selector_str).ok()?;
    let el = html.select(&sel).next()?;
    let text: String = el.text().collect::<Vec<_>>().join("").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn extract_link_href(html: &Html, pattern: &str) -> Option<String> {
    let sel = Selector::parse("a[href]").ok()?;
    for el in html.select(&sel) {
        if let Some(href) = el.value().attr("href") {
            if href.contains(pattern) {
                return Some(href.to_string());
            }
        }
    }
    None
}

fn extract_images(html: &Html, selector_str: &str) -> Vec<String> {
    let mut images = Vec::new();
    if let Ok(sel) = Selector::parse(selector_str) {
        for el in html.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                if !src.is_empty() {
                    images.push(src.to_string());
                }
            }
        }
    }
    images
}
