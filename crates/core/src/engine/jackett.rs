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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jackett_config_serializes_to_json_and_back() {
        let config = JackettConfig {
            url: "http://localhost:9117".to_string(),
            api_key: "test-api-key".to_string(),
        };
        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized: JackettConfig = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.url, config.url);
        assert_eq!(deserialized.api_key, config.api_key);
    }

    #[test]
    fn jackett_result_serializes_to_json_and_back() {
        let result = JackettResult {
            title: "Test Torrent".to_string(),
            size: 1073741824,
            tracker: "TestTracker".to_string(),
            download_url: "http://example.com/download".to_string(),
            info_hash: Some("abc123def456".to_string()),
            seeders: 42,
            category: vec!["2000".to_string(), "2010".to_string()],
        };
        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: JackettResult = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.title, result.title);
        assert_eq!(deserialized.size, result.size);
        assert_eq!(deserialized.tracker, result.tracker);
        assert_eq!(deserialized.download_url, result.download_url);
        assert_eq!(deserialized.info_hash, result.info_hash);
        assert_eq!(deserialized.seeders, result.seeders);
        assert_eq!(deserialized.category, result.category);
    }

    #[test]
    fn parse_jackett_item_full_item() {
        let item = json!({
            "Title": "Ubuntu 22.04 LTS",
            "Size": 3_000_000_000u64,
            "Tracker": "LinuxTracker",
            "Link": "http://example.com/dl/ubuntu.torrent",
            "InfoHash": "deadbeef1234",
            "Seeders": 100,
            "Category": [2000, 2020]
        });
        let result = parse_jackett_item(&item).unwrap();
        assert_eq!(result.title, "Ubuntu 22.04 LTS");
        assert_eq!(result.size, 3_000_000_000);
        assert_eq!(result.tracker, "LinuxTracker");
        assert_eq!(result.download_url, "http://example.com/dl/ubuntu.torrent");
        assert_eq!(result.info_hash, Some("deadbeef1234".to_string()));
        assert_eq!(result.seeders, 100);
        assert_eq!(result.category, vec!["2000", "2020"]);
    }

    #[test]
    fn parse_jackett_item_missing_title_returns_none() {
        let item = json!({
            "Size": 1000,
            "Link": "http://example.com/dl.torrent"
        });
        assert!(parse_jackett_item(&item).is_none());
    }

    #[test]
    fn parse_jackett_item_missing_size_returns_none() {
        let item = json!({
            "Title": "Test",
            "Link": "http://example.com/dl.torrent"
        });
        assert!(parse_jackett_item(&item).is_none());
    }

    #[test]
    fn parse_jackett_item_missing_link_returns_none() {
        let item = json!({
            "Title": "Test",
            "Size": 1000
        });
        assert!(parse_jackett_item(&item).is_none());
    }

    #[test]
    fn parse_jackett_item_optional_fields_default() {
        let item = json!({
            "Title": "Minimal",
            "Size": 500,
            "Link": "http://example.com/minimal.torrent"
        });
        let result = parse_jackett_item(&item).unwrap();
        assert_eq!(result.tracker, "");
        assert_eq!(result.info_hash, None);
        assert_eq!(result.seeders, 0);
        assert!(result.category.is_empty());
    }

    #[test]
    fn parse_jackett_item_with_categories() {
        let item = json!({
            "Title": "Categorized",
            "Size": 1000,
            "Link": "http://example.com/cat.torrent",
            "Category": [2000, 2010, 2030, 2045]
        });
        let result = parse_jackett_item(&item).unwrap();
        assert_eq!(result.category, vec!["2000", "2010", "2030", "2045"]);
    }
}
