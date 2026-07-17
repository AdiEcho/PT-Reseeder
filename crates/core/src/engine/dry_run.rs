use serde::{Deserialize, Serialize};

use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

use super::adder::MatchedTorrent;
use super::scanner::ScanResult;

/// Structured dry-run preview persisted in `task_logs.log_text`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunPreview {
    /// Schema version for forward-compatible parsing.
    pub version: u32,
    pub would_add_count: usize,
    pub items: Vec<DryRunPreviewItem>,
}

/// One would-add candidate from scan+match (add phase skipped).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunPreviewItem {
    pub site_id: i64,
    pub site_name: String,
    pub pieces_hash: String,
    pub torrent_id: Option<i64>,
    /// Readable title if available from scan/cache metadata.
    pub title: Option<String>,
    pub save_path: String,
}

pub const DRY_RUN_PREVIEW_VERSION: u32 = 1;

/// Build a dry-run preview from post-match candidates.
///
/// Title resolution order:
/// 1. scan `pieces_groups` → `torrents[info_hash].name`
/// 2. None (UI falls back to hash / torrent_id)
pub fn build_preview(
    matched: &[MatchedTorrent],
    scan: &ScanResult,
    registry: &SiteRegistry,
) -> DryRunPreview {
    let items = matched
        .iter()
        .map(|m| DryRunPreviewItem {
            site_id: m.site_id.0,
            site_name: site_name(registry, m.site_id),
            pieces_hash: m.pieces_hash.clone(),
            torrent_id: m.torrent_id,
            title: title_from_scan(scan, &m.pieces_hash),
            save_path: m.save_path.clone(),
        })
        .collect::<Vec<_>>();

    DryRunPreview {
        version: DRY_RUN_PREVIEW_VERSION,
        would_add_count: items.len(),
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

fn title_from_scan(scan: &ScanResult, pieces_hash: &str) -> Option<String> {
    let info_hashes = scan.pieces_groups.get(pieces_hash)?;
    for info_hash in info_hashes {
        if let Some(meta) = scan.torrents.get(info_hash) {
            if !meta.name.is_empty() {
                return Some(meta.name.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::models::SiteId;
    use crate::torrent::models::TorrentMeta;
    use std::collections::{HashMap, HashSet};

    fn sample_meta(name: &str, pieces_hash: &str, info_hash: &str) -> TorrentMeta {
        TorrentMeta {
            info_hash: info_hash.to_string(),
            pieces_hash: pieces_hash.to_string(),
            name: name.to_string(),
            total_size: 1,
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
        let preview = build_preview(&[], &scan, &SiteRegistry::new());
        assert_eq!(preview.version, DRY_RUN_PREVIEW_VERSION);
        assert_eq!(preview.would_add_count, 0);
        assert!(preview.items.is_empty());
    }

    #[test]
    fn build_preview_resolves_title_from_scan() {
        let pieces_hash = "phash-abc".to_string();
        let info_hash = "ihash-1".to_string();
        let mut torrents = HashMap::new();
        torrents.insert(
            info_hash.clone(),
            sample_meta("Movie.Title.2024", &pieces_hash, &info_hash),
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
            download_url: "https://example.test/dl".to_string(),
            save_path: "/downloads".to_string(),
            skip_hash_check: false,
            tag: None,
        }];

        let preview = build_preview(&matched, &scan, &SiteRegistry::new());
        assert_eq!(preview.would_add_count, 1);
        assert_eq!(preview.items[0].site_id, 42);
        assert_eq!(preview.items[0].site_name, "42");
        assert_eq!(preview.items[0].torrent_id, Some(7));
        assert_eq!(
            preview.items[0].title.as_deref(),
            Some("Movie.Title.2024")
        );
        assert_eq!(preview.items[0].save_path, "/downloads");
    }
}
