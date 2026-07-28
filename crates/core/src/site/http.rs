//! HTTP construction and response handling shared by the site adapters.
//!
//! Extracted from logic that was duplicated verbatim across the five adapters:
//! client construction, torrent-file download, and size-hint filtering.
//!
//! The downloaders deliberately do **not** use this module. Their clients need
//! `danger_accept_invalid_certs(true)` (self-signed certs are common on a
//! home-network qBittorrent/Transmission) and set no default headers, which is a
//! different security posture from a public tracker — unifying the two would mean
//! either weakening the site clients or breaking self-signed downloader setups.

use crate::site::models::TorrentSearchResult;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, USER_AGENT};
use reqwest::Client;
use std::time::Duration;
use tracing::warn;

pub const SITE_USER_AGENT: &str = "PT-Reseeder/0.1";
pub const SITE_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Lower/upper multipliers for the ±1% size-hint tolerance.
///
/// Written as literals rather than `1.0 ± 0.01` so the `f64` values are provably
/// bit-identical to the five call sites this replaced.
const SIZE_HINT_LOWER: f64 = 0.99;
const SIZE_HINT_UPPER: f64 = 1.01;

/// How a site authenticates — decides which request header gets injected.
pub enum SiteAuth<'a> {
    /// No auth header. unit3d passes its token in the query string instead.
    None,
    /// `Cookie: <value>` — nexusphp, gazelle.
    Cookie(&'a str),
    /// `Authorization: Bearer <token>` — zhuque, when a token is configured.
    Bearer(&'a str),
    /// `x-api-key: <token>` — mteam.
    ApiKey(&'a str),
}

/// Build the reqwest client used by a site adapter.
///
/// `cookie_store` is caller-supplied because nexusphp passes `false` (it manages
/// the `Cookie` header by hand) while the other four pass `true`. A header value
/// that fails to parse is skipped, matching the previous per-adapter behaviour.
pub fn build_site_client(auth: SiteAuth<'_>, cookie_store: bool) -> Client {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(SITE_USER_AGENT));
    match auth {
        SiteAuth::None => {}
        SiteAuth::Cookie(v) => {
            if let Ok(val) = HeaderValue::from_str(v) {
                headers.insert(COOKIE, val);
            }
        }
        SiteAuth::Bearer(t) => {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {t}")) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        SiteAuth::ApiKey(t) => {
            if let Ok(val) = HeaderValue::from_str(t) {
                headers.insert("x-api-key", val);
            }
        }
    }

    Client::builder()
        .use_rustls_tls()
        .cookie_store(cookie_store)
        .timeout(SITE_HTTP_TIMEOUT)
        .default_headers(headers)
        .build()
        // The single remaining panic point, previously one per adapter. Every
        // header value above went through `from_str` already, and `build()` has
        // no other failure path under this configuration.
        .expect("failed to build site reqwest client")
}

/// Download a torrent file's bytes.
///
/// Logs a warning and yields `None` on any failure — callers treat a missing
/// torrent file as non-fatal, as all four original copies did.
pub async fn fetch_torrent_bytes(
    client: &Client,
    site: &str,
    torrent_id: &str,
    download_url: &str,
) -> Option<Vec<u8>> {
    match client.get(download_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(bytes) => Some(bytes.to_vec()),
            Err(e) => {
                warn!(site = %site, torrent_id, "failed to read torrent file bytes: {e}");
                None
            }
        },
        Ok(resp) => {
            warn!(site = %site, torrent_id, status = %resp.status(), "failed to download torrent file");
            None
        }
        Err(e) => {
            warn!(site = %site, torrent_id, "failed to download torrent file: {e}");
            None
        }
    }
}

/// Drop search results whose size falls outside ±1% of `size_hint`.
///
/// A `None` hint filters nothing.
pub fn filter_by_size_hint(results: &mut Vec<TorrentSearchResult>, size_hint: Option<u64>) {
    let Some(hint) = size_hint else { return };
    let lower = (hint as f64 * SIZE_HINT_LOWER) as u64;
    let upper = (hint as f64 * SIZE_HINT_UPPER) as u64;
    results.retain(|r| r.size >= lower && r.size <= upper);
}
