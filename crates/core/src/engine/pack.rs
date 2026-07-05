use tracing;

use crate::error::CoreError;
use crate::torrent::models::{TorrentFile, TorrentMeta};

use super::jackett::{self, JackettConfig, JackettResult};

/// Heuristic: a torrent is a "pack" if it has 2+ top-level directories
/// and total size > 10 GB.
const PACK_MIN_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
const PACK_MIN_TOP_DIRS: usize = 2;

/// Check whether a torrent looks like a pack (multi-release bundle).
pub fn is_pack(meta: &TorrentMeta) -> bool {
    if meta.total_size < PACK_MIN_SIZE {
        return false;
    }

    let top_dirs = count_top_level_dirs(&meta.files);
    top_dirs >= PACK_MIN_TOP_DIRS
}

/// Count distinct top-level directories in the file list.
fn count_top_level_dirs(files: &[TorrentFile]) -> usize {
    let mut dirs = std::collections::HashSet::new();
    for f in files {
        // Multi-file: first path component is the top-level dir
        if f.path.len() >= 2 {
            dirs.insert(f.path[0].clone());
        }
    }
    dirs.len()
}

/// For a detected pack, try to match each sub-folder individually
/// via Jackett search (name + size).
pub async fn search_pack_components(
    meta: &TorrentMeta,
    jackett_config: &JackettConfig,
    http_client: &reqwest::Client,
) -> Result<Vec<PackComponentMatch>, CoreError> {
    let components = extract_components(meta);

    if components.is_empty() {
        return Ok(Vec::new());
    }

    tracing::info!(
        pack = %meta.name,
        components = components.len(),
        "searching pack components via Jackett"
    );

    let mut matches = Vec::new();

    for comp in &components {
        // Only search components >= 1 GB
        if comp.size < 1_073_741_824 {
            continue;
        }

        let results =
            jackett::search(jackett_config, http_client, &comp.name, Some(comp.size)).await?;

        for result in results {
            // Verify file list overlap if possible
            matches.push(PackComponentMatch {
                component_name: comp.name.clone(),
                component_size: comp.size,
                result,
            });
        }
    }

    tracing::info!(
        pack = %meta.name,
        matches = matches.len(),
        "pack component search complete"
    );

    Ok(matches)
}

/// A component (sub-folder) within a pack.
#[derive(Debug, Clone)]
struct PackComponent {
    name: String,
    size: u64,
    #[allow(dead_code)]
    file_count: usize,
}

/// A match result for a single pack component.
#[derive(Debug, Clone)]
pub struct PackComponentMatch {
    pub component_name: String,
    pub component_size: u64,
    pub result: JackettResult,
}

/// Extract components (top-level sub-folders) from a pack torrent.
fn extract_components(meta: &TorrentMeta) -> Vec<PackComponent> {
    let mut component_map: std::collections::HashMap<String, (u64, usize)> =
        std::collections::HashMap::new();

    for f in &meta.files {
        if f.path.len() >= 2 {
            let top_dir = &f.path[0];
            let entry = component_map.entry(top_dir.clone()).or_insert((0, 0));
            entry.0 += f.length;
            entry.1 += 1;
        }
    }

    component_map
        .into_iter()
        .map(|(name, (size, file_count))| PackComponent {
            name,
            size,
            file_count,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torrent::models::{TorrentFile, TorrentMeta};

    fn make_meta(files: Vec<TorrentFile>, total_size: u64) -> TorrentMeta {
        TorrentMeta {
            info_hash: "abc".to_string(),
            pieces_hash: "def".to_string(),
            name: "test-pack".to_string(),
            total_size,
            files,
            announce: None,
            announce_list: vec![],
            piece_length: 4194304,
            pieces_count: 10,
        }
    }

    #[test]
    fn test_is_pack_true() {
        let files = vec![
            TorrentFile {
                path: vec!["Movie.A".into(), "movie.mkv".into()],
                length: 6_000_000_000,
            },
            TorrentFile {
                path: vec!["Movie.B".into(), "movie.mkv".into()],
                length: 5_000_000_000,
            },
        ];
        let meta = make_meta(files, 11_000_000_000);
        assert!(is_pack(&meta));
    }

    #[test]
    fn test_is_pack_false_small() {
        let files = vec![
            TorrentFile {
                path: vec!["Dir.A".into(), "file.mkv".into()],
                length: 3_000_000_000,
            },
            TorrentFile {
                path: vec!["Dir.B".into(), "file.mkv".into()],
                length: 2_000_000_000,
            },
        ];
        // total < 10 GB
        let meta = make_meta(files, 5_000_000_000);
        assert!(!is_pack(&meta));
    }

    #[test]
    fn test_is_pack_false_single_dir() {
        let files = vec![
            TorrentFile {
                path: vec!["Movie".into(), "part1.mkv".into()],
                length: 6_000_000_000,
            },
            TorrentFile {
                path: vec!["Movie".into(), "part2.mkv".into()],
                length: 5_000_000_000,
            },
        ];
        let meta = make_meta(files, 11_000_000_000);
        assert!(!is_pack(&meta)); // Only 1 top-level dir
    }
}
