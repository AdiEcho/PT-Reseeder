use serde::{Deserialize, Serialize};
use tracing;

use crate::error::{CoreError, EngineError};

/// Jackett search integration for pack detection and Jackett-mode matching.
///
/// Jackett provides a unified API across many indexers, used as a fallback
/// or supplement to direct site API queries.

/// Jackett API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JackettConfig {
    pub url: String,
    pub api_key: String,
}

/// A search result from Jackett.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JackettResult {
    pub title: String,
    pub size: u64,
    pub tracker: String,
    pub download_url: String,
    pub info_hash: Option<String>,
    pub seeders: u32,
    pub category: Vec<String>,
}

/// Search Jackett for torrents matching a query with optional size constraints.
pub async fn search(
    config: &JackettConfig,
    http_client: &reqwest::Client,
    query: &str,
    size_hint: Option<u64>,
) -> Result<Vec<JackettResult>, CoreError> {
    let url = format!(
        "{}/api/v2.0/indexers/all/results",
        config.url.trim_end_matches('/')
    );

    let params = vec![
        ("apikey", config.api_key.clone()),
        ("Query", query.to_string()),
    ];

    // Jackett doesn't have native size filter, but we can filter results
    let resp = http_client
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| EngineError::MatchFailed(format!("jackett request: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(EngineError::MatchFailed(format!("jackett HTTP {}: {}", status, url)).into());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| EngineError::MatchFailed(format!("jackett parse: {}", e)))?;

    let results = body
        .get("Results")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| parse_jackett_item(item))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Apply size filter if hint provided (±0.01 GB tolerance = ±10_737_418 bytes)
    let filtered = if let Some(hint) = size_hint {
        let tolerance = 10_737_418u64; // ~0.01 GB
        results
            .into_iter()
            .filter(|r| {
                let diff = if r.size > hint {
                    r.size - hint
                } else {
                    hint - r.size
                };
                diff <= tolerance
            })
            .collect()
    } else {
        results
    };

    tracing::debug!(
        query = %query,
        results = filtered.len(),
        "jackett search complete"
    );

    Ok(filtered)
}

/// Parse a single Jackett result item from JSON.
fn parse_jackett_item(item: &serde_json::Value) -> Option<JackettResult> {
    Some(JackettResult {
        title: item.get("Title")?.as_str()?.to_string(),
        size: item.get("Size")?.as_u64()?,
        tracker: item
            .get("Tracker")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        download_url: item.get("Link")?.as_str()?.to_string(),
        info_hash: item
            .get("InfoHash")
            .and_then(|v| v.as_str())
            .map(String::from),
        seeders: item.get("Seeders").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        category: item
            .get("Category")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_u64().map(|n| n.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}
