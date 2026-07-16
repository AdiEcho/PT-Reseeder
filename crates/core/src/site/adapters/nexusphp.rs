use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, USER_AGENT};
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
    /// Shared runtime switch for the extra seeding-list request.
    fetch_seeding_size: Arc<AtomicBool>,
}

/// NexusPHP `/api/pieces-hash` 常见返回：
/// ```json
/// { "ret": 0, "msg": "...", "data": { "<pieces_hash>": <torrent_id>, ... } }
/// ```
/// 无命中时 `data` 是 `{}` 而不是 `[]`。部分站点也可能返回数组形式。
#[derive(Debug, Deserialize)]
struct PiecesHashResponse {
    #[serde(default)]
    ret: Option<i64>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct PiecesHashMatch {
    #[serde(default, alias = "piecesHash")]
    pieces_hash: String,
    #[serde(default, alias = "torrentId", deserialize_with = "deserialize_torrent_id")]
    torrent_id: i64,
}

fn deserialize_torrent_id<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Unexpected};

    struct TorrentIdVisitor;

    impl<'de> de::Visitor<'de> for TorrentIdVisitor {
        type Value = i64;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("torrent id as number or numeric string")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(|_| de::Error::invalid_value(Unexpected::Unsigned(v), &self))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.trim()
                .parse::<i64>()
                .map_err(|_| de::Error::invalid_value(Unexpected::Str(v), &self))
        }
    }

    deserializer.deserialize_any(TorrentIdVisitor)
}

/// 把 pieces-hash API 的 `data` 字段归一成 `(pieces_hash, torrent_id)` 列表。
///
/// 兼容：
/// - 对象 map：`{ "abc...": 123 }`（NexusPHP 主流）
/// - 空对象 / null：`{}` / `null`
/// - 数组：`[{ "pieces_hash": "...", "torrent_id": 123 }]`
fn normalize_pieces_hash_data(
    data: serde_json::Value,
) -> Result<Vec<(String, i64)>, String> {
    match data {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Object(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (pieces_hash, torrent_id) in map {
                // 个别站点会把请求字段原样回显到 data 里，直接跳过
                if pieces_hash == "passkey" || pieces_hash == "pieces_hash" {
                    continue;
                }
                let id = match torrent_id {
                    serde_json::Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_u64().and_then(|u| i64::try_from(u).ok()))
                        .ok_or_else(|| {
                            format!("invalid torrent_id number for pieces_hash {pieces_hash}: {n}")
                        })?,
                    serde_json::Value::String(s) => s.trim().parse::<i64>().map_err(|_| {
                        format!("invalid torrent_id string for pieces_hash {pieces_hash}: {s}")
                    })?,
                    other => {
                        return Err(format!(
                            "unsupported torrent_id type for pieces_hash {pieces_hash}: {other}"
                        ));
                    }
                };
                out.push((pieces_hash, id));
            }
            Ok(out)
        }
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().enumerate() {
                let m: PiecesHashMatch = serde_json::from_value(item).map_err(|e| {
                    format!("invalid pieces_hash array item at index {idx}: {e}")
                })?;
                if m.pieces_hash.is_empty() {
                    return Err(format!(
                        "pieces_hash missing in array item at index {idx}"
                    ));
                }
                out.push((m.pieces_hash, m.torrent_id));
            }
            Ok(out)
        }
        other => Err(format!(
            "unexpected pieces_hash data type, expected object/array/null, got {}",
            match other {
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                _ => "unknown",
            }
        )),
    }
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
            .cookie_store(false)
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
            fetch_seeding_size: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_fetch_seeding_size(mut self, enabled: bool) -> Self {
        self.fetch_seeding_size = Arc::new(AtomicBool::new(enabled));
        self
    }

    pub fn with_fetch_seeding_size_switch(mut self, enabled: Arc<AtomicBool>) -> Self {
        self.fetch_seeding_size = enabled;
        self
    }

    /// Returns the configured batch size for hash queries.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Fetch the index page, extract the user id, and return (user_id, index_html).
    /// The index HTML is kept so callers can extract info-bar fields (seeding count,
    /// leeching count, etc.) that only appear on the main page, not on userdetails.
    async fn resolve_user_id_with_index(&self) -> Result<(String, Html), CoreError> {
        let resp = self
            .client
            .get(&self.base_url)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(
                SiteError::HttpError(format!("HTTP {} resolving user id", resp.status())).into(),
            );
        }
        let body = resp
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        let html = Html::parse_document(&body);
        let uid = extract_user_id(&html, &self.selectors).ok_or_else(|| -> CoreError {
            SiteError::AuthFailed("failed to resolve user_id from cookie session".into()).into()
        })?;
        Ok((uid, html))
    }

    async fn resolve_user_id(&self) -> Result<String, CoreError> {
        if let Some(user_id) = self.user_id.as_ref().filter(|id| !id.trim().is_empty()) {
            return Ok(user_id.clone());
        }
        let (uid, _) = self.resolve_user_id_with_index().await?;
        Ok(uid)
    }

    async fn fetch_ajax_seeding_size(
        &self,
        user_id: &str,
        referer: &str,
    ) -> Result<Option<i64>, CoreError> {
        let url = format!(
            "{}/getusertorrentlistajax.php?userid={}&type=seeding",
            self.base_url.trim_end_matches('/'),
            user_id
        );
        let response = self
            .client
            .get(url)
            .header(REFERER, referer)
            .send()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SiteError::HttpError(format!(
                "HTTP {} fetching seeding list",
                response.status()
            ))
            .into());
        }

        let body = response
            .text()
            .await
            .map_err(|e| SiteError::HttpError(e.to_string()))?;
        Ok(parse_seeding_size_summary(&body))
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn extract_text(html: &Html, selector_str: &Option<String>) -> Option<String> {
    let sel_str = selector_str.as_deref()?;

    // Handle :contains('...') pseudo-selector which scraper does not support.
    // Patterns supported:
    //   "tag.class:contains('text') + sibling"
    //   "tag.class:contains('text') + sibling tag"
    //   "tag:contains('text')"
    if let Some(contains_start) = sel_str.find(":contains(") {
        let prefix_sel_str = &sel_str[..contains_start];
        let after_contains = &sel_str[contains_start + ":contains(".len()..];
        // Extract the text inside quotes: 'text' or "text"
        let (needle, rest) = if after_contains.starts_with('\'') {
            let end = after_contains[1..].find('\'')?;
            (&after_contains[1..1 + end], &after_contains[2 + end..])
        } else if after_contains.starts_with('"') {
            let end = after_contains[1..].find('"')?;
            (&after_contains[1..1 + end], &after_contains[2 + end..])
        } else {
            return None;
        };
        // rest should start with ')', strip it
        let rest = rest.strip_prefix(')')?;
        let suffix = rest.trim();

        // Parse the prefix selector (the part before :contains)
        let prefix_selector = Selector::parse(prefix_sel_str).ok()?;

        // Find the most specific (deepest) element whose text contains the needle.
        // A naive `.find()` would match a large ancestor whose text also contains
        // the needle. Prefer the element with the shortest text content — it is
        // the most specific match.
        let matched_el = html
            .select(&prefix_selector)
            .filter(|el| {
                let text: String = el.text().collect::<Vec<_>>().join("");
                text.contains(needle)
            })
            .min_by_key(|el| el.text().collect::<Vec<_>>().join("").len())?;

        // If there is a " + sibling" suffix, navigate to the next sibling element
        if let Some(sibling_part) = suffix.strip_prefix('+') {
            let sibling_part = sibling_part.trim();
            // Parse "td img" as sibling_tag="td", descendant_sel="img"
            let (sibling_tag, descendant_sel) = match sibling_part.split_once(char::is_whitespace) {
                Some((tag, desc)) => (tag.trim(), Some(desc.trim())),
                None => (sibling_part, None),
            };
            let expected_tag = sibling_tag
                .split(|c: char| c == '.' || c == '#' || c == '[' || c == ':')
                .next()
                .unwrap_or("");
            // Walk next element siblings via the tree
            let node_id = matched_el.id();
            let node_ref = html.tree.get(node_id)?;
            for sibling in node_ref.next_siblings() {
                if let Some(el) = scraper::ElementRef::wrap(sibling) {
                    if !expected_tag.is_empty()
                        && !el.value().name().eq_ignore_ascii_case(expected_tag)
                    {
                        continue;
                    }
                    // If there is a descendant selector (e.g. "img" in "+ td img"),
                    // find that element inside the matched sibling
                    if let Some(desc) = descendant_sel {
                        if let Ok(desc_sel) = Selector::parse(desc) {
                            if let Some(inner) = el.select(&desc_sel).next() {
                                // Use the same img-aware extraction logic
                                if inner.value().name().eq_ignore_ascii_case("img") {
                                    // Try next sibling text, then alt/title
                                    let inner_id = inner.id();
                                    if let Some(inner_ref) = html.tree.get(inner_id) {
                                        for s in inner_ref.next_siblings() {
                                            if let Some(t) = s.value().as_text() {
                                                let trimmed = t.trim();
                                                if !trimmed.is_empty() {
                                                    return Some(trimmed.to_string());
                                                }
                                            }
                                            if scraper::ElementRef::wrap(s).is_some() {
                                                break;
                                            }
                                        }
                                    }
                                    let title = inner.value().attr("title").unwrap_or("").trim();
                                    let alt = inner.value().attr("alt").unwrap_or("").trim();
                                    let val = if !title.is_empty() { title } else { alt };
                                    if !val.is_empty() {
                                        return Some(val.to_string());
                                    }
                                } else {
                                    let t: String = inner
                                        .text()
                                        .collect::<Vec<_>>()
                                        .join("")
                                        .trim()
                                        .to_string();
                                    if !t.is_empty() {
                                        return Some(t);
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    let text: String = el.text().collect::<Vec<_>>().join("").trim().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        } else {
            // No sibling suffix — return text of the matched element itself
            let text: String = matched_el
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
    } else {
        let selector = Selector::parse(sel_str).ok()?;
        let element = html.select(&selector).next()?;

        // For <img> elements: first try next sibling text node (e.g. seeding count
        // after <img class="arrowup"/>33), then fall back to alt/title attribute
        // (e.g. user class <img alt="Veteran User" title="Veteran User"/>).
        if element.value().name().eq_ignore_ascii_case("img") {
            let node_id = element.id();
            if let Some(node_ref) = html.tree.get(node_id) {
                for sibling in node_ref.next_siblings() {
                    if let Some(text_node) = sibling.value().as_text() {
                        let trimmed = text_node.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                    if scraper::ElementRef::wrap(sibling).is_some() {
                        break;
                    }
                }
            }
            // Fallback to alt/title attribute
            let title = element.value().attr("title").unwrap_or("").trim();
            let alt = element.value().attr("alt").unwrap_or("").trim();
            let val = if !title.is_empty() { title } else { alt };
            return if val.is_empty() {
                None
            } else {
                Some(val.to_string())
            };
        }

        let text: String = element
            .text()
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        // If the element text looks like a label only (ends with ':' or '：'),
        // the actual value is in the next sibling text node (common NexusPHP
        // info-bar pattern: <font class='color_uploaded'>上传量:</font> 4.425 TB).
        if !text.is_empty() && !text.ends_with(':') && !text.ends_with('：') {
            return Some(text);
        }

        // Fallback: collect the next sibling text node after this element
        let node_id = element.id();
        if let Some(node_ref) = html.tree.get(node_id) {
            for sibling in node_ref.next_siblings() {
                // Text nodes are not elements — they won't wrap into ElementRef.
                // Check if it's a text node by trying to get its value.
                if let Some(text_node) = sibling.value().as_text() {
                    let trimmed = text_node.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                // Stop at the next element (don't skip past it into unrelated content)
                if scraper::ElementRef::wrap(sibling).is_some() {
                    break;
                }
            }
        }

        if text.is_empty() {
            None
        } else {
            Some(text)
        }
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
    // Use regex to find a "number unit" pattern anywhere in the text.
    // This handles prefix labels like "上传量:  4.425 TB".
    let re = regex_lite::Regex::new(r"(\d[\d,]*\.?\d*)\s*(B|KB|KIB|MB|MIB|GB|GIB|TB|TIB|PB|PIB)\b")
        .ok()?;
    let upper = text.to_uppercase();
    let caps = re.captures(&upper)?;
    let num_str: String = caps[1].chars().filter(|c| *c != ',').collect();
    let unit_str = &caps[2];

    let value: f64 = num_str.parse().ok()?;

    let multiplier: f64 = match unit_str {
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

fn parse_seeding_size_summary(body: &str) -> Option<i64> {
    let text = Html::parse_fragment(body)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ");
    let summary = regex_lite::Regex::new(
        r"(?i)(?:总大小|total\s*size)\s*[：:]?\s*(\d[\d,]*\.?\d*\s*(?:B|KB|KIB|MB|MIB|GB|GIB|TB|TIB|PB|PIB))\b",
    )
    .ok()?;
    let value = summary.captures(&text)?.get(1)?.as_str();
    parse_size_to_bytes(value)
}

fn extract_seeding_ajax_user_id(body: &str) -> Option<String> {
    let patterns = [
        r#"getusertorrentlistajax\s*\(\s*['\"](\d+)['\"]\s*,\s*['\"]seeding['\"]"#,
        r#"getusertorrentlistajax\.php\?[^\"']*userid=(\d+)[^\"']*type=seeding"#,
    ];
    patterns.into_iter().find_map(|pattern| {
        regex_lite::Regex::new(pattern)
            .ok()?
            .captures(body)?
            .get(1)
            .map(|value| value.as_str().to_string())
    })
}

fn parse_ratio(text: &str) -> Option<f64> {
    let text = text.trim();
    if text == "∞" || text.eq_ignore_ascii_case("inf") || text.eq_ignore_ascii_case("infinite") {
        return Some(f64::INFINITY);
    }
    // Extract the first floating-point number from the text,
    // skipping prefix labels like "分享率:" or "Ratio:"
    let re = regex_lite::Regex::new(r"(\d[\d,]*\.?\d*)").ok()?;
    let caps = re.captures(text)?;
    let cleaned: String = caps[1].chars().filter(|c| *c != ',').collect();
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
                ':' => {
                    // Colon-separated time: could be HH:MM:SS or MM:SS
                    // Collect the rest as a time string and parse it
                    let rest: String =
                        text[text.find(&format!("{num}:")).unwrap_or(0)..].to_string();
                    let parts: Vec<&str> = rest.split(':').collect();
                    match parts.len() {
                        3 => {
                            // HH:MM:SS
                            let h: i64 = parts[0].trim().parse().unwrap_or(0);
                            let m: i64 = parts[1].trim().parse().unwrap_or(0);
                            let s: i64 = parts[2].trim().parse().unwrap_or(0);
                            total_seconds += h * 3600 + m * 60 + s;
                        }
                        2 => {
                            // MM:SS
                            let m: i64 = parts[0].trim().parse().unwrap_or(0);
                            let s: i64 = parts[1].trim().parse().unwrap_or(0);
                            total_seconds += m * 60 + s;
                        }
                        _ => {}
                    }
                    // We consumed the rest of the colon time, return immediately
                    return if total_seconds > 0 {
                        Some(total_seconds)
                    } else {
                        None
                    };
                }
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
        let body = resp.text().await.map_err(|e| {
            SiteError::HttpError(format!("failed to read pieces_hash response body: {e}"))
        })?;

        if !status.is_success() {
            let preview: String = body.chars().take(200).collect();
            return Err(SiteError::HttpError(format!(
                "HTTP {status} from pieces_hash API: {preview}"
            ))
            .into());
        }

        let parsed: PiecesHashResponse = serde_json::from_str(&body).map_err(|e| {
            let preview: String = body.chars().take(200).collect();
            SiteError::ParseError(format!(
                "failed to parse pieces_hash response: {e}; body={preview}"
            ))
        })?;

        // 部分站点带 ret 字段：0 表示成功，其它为业务错误
        if let Some(ret) = parsed.ret {
            if ret != 0 {
                let msg = parsed.msg.unwrap_or_else(|| "unknown error".into());
                return Err(SiteError::ParseError(format!(
                    "pieces_hash API returned ret={ret}: {msg}"
                ))
                .into());
            }
        }

        let matches = normalize_pieces_hash_data(parsed.data).map_err(|e| {
            SiteError::ParseError(format!("failed to parse pieces_hash response: {e}"))
        })?;

        debug!(
            site = %self.name,
            matches = matches.len(),
            "pieces hash query completed"
        );

        Ok(matches)
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

        // Fetch index page and extract info-bar fields before any further await.
        // Html contains Cell<usize> (not Send), so it must not live across awaits.
        let (
            user_id,
            idx_uploaded,
            idx_downloaded,
            idx_ratio,
            idx_bonus,
            idx_user_class,
            idx_seeding_count,
            idx_leeching_count,
            idx_seeding_size,
            idx_upload_time,
        ) = {
            let (uid, index_html) =
                if let Some(uid) = self.user_id.as_ref().filter(|id| !id.trim().is_empty()) {
                    let resp = self
                        .client
                        .get(&self.base_url)
                        .send()
                        .await
                        .map_err(|e| SiteError::HttpError(e.to_string()))?;
                    let body = resp
                        .text()
                        .await
                        .map_err(|e| SiteError::HttpError(e.to_string()))?;
                    (uid.clone(), Html::parse_document(&body))
                } else {
                    self.resolve_user_id_with_index().await?
                };
            let s = &self.selectors;
            (
                uid,
                extract_text(&index_html, &s.uploaded_selector)
                    .and_then(|t| parse_size_to_bytes(&t)),
                extract_text(&index_html, &s.downloaded_selector)
                    .and_then(|t| parse_size_to_bytes(&t)),
                extract_text(&index_html, &s.ratio_selector).and_then(|t| parse_ratio(&t)),
                extract_text(&index_html, &s.bonus_selector).and_then(|t| parse_f64_from_text(&t)),
                extract_text(&index_html, &s.user_class_selector),
                extract_text(&index_html, &s.seeding_count_selector)
                    .and_then(|t| parse_number_from_text(&t)),
                extract_text(&index_html, &s.leeching_count_selector)
                    .and_then(|t| parse_number_from_text(&t)),
                extract_text(&index_html, &s.seeding_size_selector)
                    .and_then(|t| parse_size_to_bytes(&t)),
                extract_text(&index_html, &s.upload_time_selector)
                    .and_then(|t| parse_time_to_seconds(&t)),
            )
            // index_html is dropped here
        };

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
        let ajax_user_id = extract_seeding_ajax_user_id(&body);
        let (
            uploaded,
            downloaded,
            ratio,
            bonus,
            user_class,
            seeding_count,
            leeching_count,
            seeding_size,
            upload_time_seconds,
        ) = {
            let html = Html::parse_document(&body);
            (
                extract_text(&html, &self.selectors.uploaded_selector)
                    .and_then(|t| parse_size_to_bytes(&t))
                    .or(idx_uploaded),
                extract_text(&html, &self.selectors.downloaded_selector)
                    .and_then(|t| parse_size_to_bytes(&t))
                    .or(idx_downloaded),
                extract_text(&html, &self.selectors.ratio_selector)
                    .and_then(|t| parse_ratio(&t))
                    .or(idx_ratio),
                extract_text(&html, &self.selectors.bonus_selector)
                    .and_then(|t| parse_f64_from_text(&t))
                    .or(idx_bonus),
                extract_text(&html, &self.selectors.user_class_selector).or(idx_user_class),
                extract_text(&html, &self.selectors.seeding_count_selector)
                    .and_then(|t| parse_number_from_text(&t))
                    .or(idx_seeding_count),
                extract_text(&html, &self.selectors.leeching_count_selector)
                    .and_then(|t| parse_number_from_text(&t))
                    .or(idx_leeching_count),
                extract_text(&html, &self.selectors.seeding_size_selector)
                    .and_then(|t| parse_size_to_bytes(&t))
                    .or(idx_seeding_size),
                extract_text(&html, &self.selectors.upload_time_selector)
                    .and_then(|t| parse_time_to_seconds(&t))
                    .or(idx_upload_time),
            )
        };

        let seeding_size = if self.fetch_seeding_size.load(Ordering::Relaxed) {
            let ajax_user_id = ajax_user_id.as_deref().unwrap_or(&user_id);
            match self.fetch_ajax_seeding_size(ajax_user_id, &url).await {
                Ok(Some(size)) => Some(size),
                Ok(None) => {
                    warn!(site = %self.name, "seeding list response did not contain total size");
                    seeding_size
                }
                Err(error) => {
                    warn!(site = %self.name, %error, "failed to fetch seeding list size");
                    seeding_size
                }
            }
        } else {
            seeding_size
        };

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

#[cfg(test)]
mod tests {
    use super::{
        extract_seeding_ajax_user_id, normalize_pieces_hash_data, parse_seeding_size_summary,
        PiecesHashResponse,
    };

    #[test]
    fn extracts_ptcafe_seeding_ajax_user_id() {
        let body = r#"
            <a href="javascript: getusertorrentlistajax('13154', 'seeding', 'ka1'); klappe_news('a1')">
                当前做种
            </a>
        "#;

        assert_eq!(extract_seeding_ajax_user_id(body).as_deref(), Some("13154"));
    }

    #[test]
    fn parses_ptcafe_seeding_size_summary() {
        let body = r#"
            <div style="display: flex;justify-content: space-between">
                <div><b>33</b> 条记录 | 总大小：5.659 TB</div>
                <div></div>
            </div>
        "#;

        assert_eq!(
            parse_seeding_size_summary(body),
            Some((5.659_f64 * 1024_f64.powi(4)) as i64)
        );
    }

    #[test]
    fn rejects_seeding_response_without_total_size() {
        assert_eq!(
            parse_seeding_size_summary("<div><b>33</b> 条记录</div>"),
            None
        );
    }

    #[test]
    fn normalizes_empty_object_data_as_no_matches() {
        // PTCafe 无命中时返回 data: {}
        let raw = r#"{"ret":0,"msg":"torrent.querybypieceshash","data":{},"time":0.2,"rid":"x"}"#;
        let parsed: PiecesHashResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.ret, Some(0));
        let matches = normalize_pieces_hash_data(parsed.data).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn normalizes_object_map_data() {
        let raw = r#"{
            "ret": 0,
            "msg": "ok",
            "data": {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": 1001,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": "2002",
                "passkey": "should-skip",
                "pieces_hash": "should-skip"
            }
        }"#;
        let parsed: PiecesHashResponse = serde_json::from_str(raw).unwrap();
        let mut matches = normalize_pieces_hash_data(parsed.data).unwrap();
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            matches,
            vec![
                (
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    1001
                ),
                (
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    2002
                ),
            ]
        );
    }

    #[test]
    fn normalizes_array_data() {
        let raw = r#"{
            "data": [
                {"pieces_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "torrent_id": 42},
                {"piecesHash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "torrentId": "43"}
            ]
        }"#;
        let parsed: PiecesHashResponse = serde_json::from_str(raw).unwrap();
        let matches = normalize_pieces_hash_data(parsed.data).unwrap();
        assert_eq!(
            matches,
            vec![
                (
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    42
                ),
                (
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    43
                ),
            ]
        );
    }

    #[test]
    fn normalizes_null_data_as_no_matches() {
        let matches = normalize_pieces_hash_data(serde_json::Value::Null).unwrap();
        assert!(matches.is_empty());
    }
}
