use serde::{Deserialize, Serialize};

use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

use super::adder::MatchedTorrent;
use super::scanner::ScanResult;

/// Structured reseed-run result persisted in `task_logs.log_text`.
///
/// Used for both dry-run previews and real reseed runs so the UI can show
/// matched torrents after a task finishes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunPreview {
    /// Schema version for forward-compatible parsing.
    pub version: u32,
    pub would_add_count: usize,
    /// Whether this result came from a dry-run (add phase skipped).
    #[serde(default)]
    pub dry_run: bool,
    pub items: Vec<DryRunPreviewItem>,
}

/// One matched candidate from scan+match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunPreviewItem {
    pub site_id: i64,
    pub site_name: String,
    pub pieces_hash: String,
    pub torrent_id: Option<i64>,
    /// Readable title if available from scan/cache metadata.
    pub title: Option<String>,
    /// Local save path of the source torrent / content directory.
    pub save_path: String,
    /// Content size in bytes when known from scan metadata.
    #[serde(default)]
    pub total_size: Option<i64>,
    /// Public torrent detail page URL (never includes passkey/token).
    #[serde(default)]
    pub detail_url: Option<String>,
}

/// Current schema version written by new runs.
///
/// v1: site/title/hash/torrent_id/save_path
/// v2: + dry_run envelope flag, total_size, detail_url
pub const DRY_RUN_PREVIEW_VERSION: u32 = 2;

/// Build a reseed-run result from post-match candidates.
///
/// Title / size resolution order:
/// 1. scan `pieces_groups` → `torrents[info_hash].{name,total_size}`
/// 2. None (UI falls back to hash / torrent_id / "-")
pub fn build_preview(
    matched: &[MatchedTorrent],
    scan: &ScanResult,
    registry: &SiteRegistry,
    dry_run: bool,
) -> DryRunPreview {
    let items = matched
        .iter()
        .map(|m| {
            let (title, total_size) = meta_from_scan(scan, &m.pieces_hash);
            DryRunPreviewItem {
                site_id: m.site_id.0,
                site_name: site_name(registry, m.site_id),
                pieces_hash: m.pieces_hash.clone(),
                torrent_id: m.torrent_id,
                title,
                save_path: m.save_path.clone(),
                total_size,
                detail_url: detail_url(registry, m.site_id, m.torrent_id),
            }
        })
        .collect::<Vec<_>>();

    DryRunPreview {
        version: DRY_RUN_PREVIEW_VERSION,
        would_add_count: items.len(),
        dry_run,
        items,
    }
}

fn site_name(registry: &SiteRegistry, site_id: SiteId) -> String {
    registry
        .get(&site_id)
        .map(|handle| handle.core.name().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| site_id.0.to_string())
}

fn site_base_url(registry: &SiteRegistry, site_id: SiteId) -> Option<String> {
    registry
        .get(&site_id)
        .map(|handle| handle.core.base_url().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
}

/// Build a public details-page URL without credentials.
///
/// Most built-in adapters expose a human-readable details page; NexusPHP-style
/// `/details.php?id=` covers the majority of sites in this project. When the
/// site has no torrent id (e.g. Jackett pack matches) we omit the link.
fn detail_url(registry: &SiteRegistry, site_id: SiteId, torrent_id: Option<i64>) -> Option<String> {
    let id = torrent_id?;
    let base = site_base_url(registry, site_id)?;
    Some(format!("{base}/details.php?id={id}"))
}

fn meta_from_scan(scan: &ScanResult, pieces_hash: &str) -> (Option<String>, Option<i64>) {
    let Some(info_hashes) = scan.pieces_groups.get(pieces_hash) else {
        return (None, None);
    };

    let mut title = None;
    let mut total_size = None;
    for info_hash in info_hashes {
        let Some(meta) = scan.torrents.get(info_hash) else {
            continue;
        };
        if title.is_none() && !meta.name.is_empty() {
            title = Some(meta.name.clone());
        }
        if total_size.is_none() && meta.total_size > 0 {
            total_size = Some(meta.total_size as i64);
        }
        if title.is_some() && total_size.is_some() {
            break;
        }
    }
    (title, total_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::models::SiteId;
    use crate::torrent::models::TorrentMeta;
    use std::collections::{HashMap, HashSet};

    fn sample_meta(name: &str, pieces_hash: &str, info_hash: &str, total_size: u64) -> TorrentMeta {
        TorrentMeta {
            info_hash: info_hash.to_string(),
            pieces_hash: pieces_hash.to_string(),
            name: name.to_string(),
            total_size,
            files: vec![],
            announce: None,
            announce_list: vec![],
            piece_length: 0,
            pieces_count: 0,
        }
    }

    #[test]
    fn build_preview_empty_matched() {
        let scan = ScanResult {
            torrents: HashMap::new(),
            pieces_groups: HashMap::new(),
            dest_hashes: HashSet::new(),
            save_paths: HashMap::new(),
        };
        let preview = build_preview(&[], &scan, &SiteRegistry::new(), true);
        assert_eq!(preview.version, DRY_RUN_PREVIEW_VERSION);
        assert_eq!(preview.would_add_count, 0);
        assert!(preview.dry_run);
        assert!(preview.items.is_empty());
    }

    #[test]
    fn build_preview_resolves_title_size_and_detail_url() {
        let pieces_hash = "phash-abc".to_string();
        let info_hash = "ihash-1".to_string();
        let mut torrents = HashMap::new();
        torrents.insert(
            info_hash.clone(),
            sample_meta("Movie.Title.2024", &pieces_hash, &info_hash, 1_024_000),
        );
        let mut pieces_groups = HashMap::new();
        pieces_groups.insert(pieces_hash.clone(), vec![info_hash]);

        let scan = ScanResult {
            torrents,
            pieces_groups,
            dest_hashes: HashSet::new(),
            save_paths: HashMap::new(),
        };

        let matched = vec![MatchedTorrent {
            pieces_hash: pieces_hash.clone(),
            site_id: SiteId(42),
            torrent_id: Some(7),
            download_url: "https://example.test/dl?passkey=SECRET".to_string(),
            save_path: "/downloads/movie".to_string(),
            skip_hash_check: false,
            tag: None,
        }];

        let preview = build_preview(&matched, &scan, &SiteRegistry::new(), false);
        assert!(!preview.dry_run);
        assert_eq!(preview.would_add_count, 1);
        assert_eq!(preview.items[0].site_id, 42);
        assert_eq!(preview.items[0].site_name, "42");
        assert_eq!(preview.items[0].torrent_id, Some(7));
        assert_eq!(
            preview.items[0].title.as_deref(),
            Some("Movie.Title.2024")
        );
        assert_eq!(preview.items[0].save_path, "/downloads/movie");
        assert_eq!(preview.items[0].total_size, Some(1_024_000));
        // No registry handle → no base_url → detail_url omitted (never invent secrets).
        assert_eq!(preview.items[0].detail_url, None);
    }

    #[test]
    fn v1_json_deserializes_with_defaults() {
        let json = r#"{
            "version": 1,
            "would_add_count": 1,
            "items": [{
                "site_id": 1,
                "site_name": "HD",
                "pieces_hash": "abc",
                "torrent_id": 9,
                "title": "T",
                "save_path": "/d"
            }]
        }"#;
        let preview: DryRunPreview = serde_json::from_str(json).unwrap();
        assert_eq!(preview.version, 1);
        assert!(!preview.dry_run);
        assert_eq!(preview.items[0].total_size, None);
        assert_eq!(preview.items[0].detail_url, None);
    }
}
