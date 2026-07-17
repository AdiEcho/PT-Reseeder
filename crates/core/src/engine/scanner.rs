use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tokio_util::sync::CancellationToken;
use tracing;

use crate::db::repo::Repository;
use crate::db::writer::{BulkPiecesCacheItem, DbWriterHandle, WriteOp};
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, EngineError};
use crate::torrent::models::TorrentMeta;
use crate::torrent::parser;

/// Scan source: either a local folder of .torrent files or a downloader's torrent list.
pub enum ScanSource<'a> {
    /// Scan a local directory for .torrent files.
    Folder { path: &'a Path },
    /// Read torrent list from a downloader's torrent_dir.
    Downloader {
        client: &'a dyn Downloader,
        torrent_dir: &'a Path,
    },
}

/// Result of scanning: grouped by pieces_hash for batch matching.
#[derive(Debug)]
pub struct ScanResult {
    /// All parsed torrent metadata, keyed by info_hash.
    pub torrents: HashMap<String, TorrentMeta>,
    /// pieces_hash -> set of info_hashes that share it.
    pub pieces_groups: HashMap<String, Vec<String>>,
    /// info_hashes already present in the destination downloader.
    pub dest_hashes: HashSet<String>,
    /// Source torrent save paths keyed by info_hash (from downloader list API).
    /// Used so reseed add keeps the original download directory.
    pub save_paths: HashMap<String, String>,
}

/// Scan .torrent files from a folder, parse them, deduplicate against the cache,
/// and populate the pieces_cache via DbWriter.
pub async fn scan_folder(
    path: &Path,
    repo: &Repository,
    db_writer: &DbWriterHandle,
    dest_client: &dyn Downloader,
    stats: &super::stats::ReseedStats,
    cancel: &CancellationToken,
) -> Result<ScanResult, CoreError> {
    let mut result = empty_scan_result();

    // Collect all .torrent file paths off the async runtime.
    let dir = path.to_path_buf();
    let entries = tokio::task::spawn_blocking(move || collect_torrent_files(&dir))
        .await
        .map_err(|e| EngineError::ScanFailed(format!("directory walk task failed: {}", e)))??;
    tracing::info!(folder = %path.display(), count = entries.len(), "scanning torrent files");

    // Parse all files first so we can batch-check the cache.
    let mut parsed: Vec<(Option<PathBuf>, TorrentMeta)> = Vec::with_capacity(entries.len());
    for entry_path in &entries {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        stats.scanned.fetch_add(1, Ordering::Relaxed);

        match parser::parse_file(entry_path) {
            Ok(meta) => parsed.push((Some(entry_path.clone()), meta)),
            Err(e) => {
                tracing::warn!(
                    path = %entry_path.display(),
                    error = %e,
                    "failed to parse torrent file, skipping"
                );
            }
        }
    }

    ingest_parsed_metas(parsed, &mut result, repo, db_writer, stats, cancel).await?;

    // Get all info_hashes from destination downloader for dedup
    result.dest_hashes = dest_client.get_all_info_hashes().await?;

    tracing::info!(
        parsed = result.torrents.len(),
        pieces_groups = result.pieces_groups.len(),
        dest_torrents = result.dest_hashes.len(),
        "scan complete"
    );

    Ok(result)
}

/// Scan from a downloader via torrent_dir and/or export API.
pub async fn scan_downloader(
    client: &dyn Downloader,
    torrent_dir: Option<&Path>,
    repo: &Repository,
    db_writer: &DbWriterHandle,
    dest_client: Option<&dyn Downloader>,
    stats: &super::stats::ReseedStats,
    cancel: &CancellationToken,
) -> Result<ScanResult, CoreError> {
    let mut result = empty_scan_result();
    let mut parsed: Vec<(Option<PathBuf>, TorrentMeta)> = Vec::new();
    let mut seen_info_hashes = HashSet::new();

    if let Some(dir) = torrent_dir {
        if dir.is_dir() {
            let dir_buf = dir.to_path_buf();
            let entries = tokio::task::spawn_blocking(move || collect_torrent_files(&dir_buf))
                .await
                .map_err(|e| {
                    EngineError::ScanFailed(format!("directory walk task failed: {}", e))
                })??;
            tracing::info!(
                folder = %dir.display(),
                count = entries.len(),
                "scanning downloader torrent_dir"
            );

            for entry_path in &entries {
                if cancel.is_cancelled() {
                    return Err(EngineError::Cancelled.into());
                }
                stats.scanned.fetch_add(1, Ordering::Relaxed);
                match parser::parse_file(entry_path) {
                    Ok(meta) => {
                        if seen_info_hashes.insert(meta.info_hash.clone()) {
                            parsed.push((Some(entry_path.clone()), meta));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %entry_path.display(),
                            error = %e,
                            "failed to parse torrent file, skipping"
                        );
                    }
                }
            }
        } else {
            tracing::warn!(
                path = %dir.display(),
                "configured torrent_dir is not a directory, skipping local scan"
            );
        }
    }

    // Prefer list_torrents so we can capture each torrent's save_path.
    // Fall back to get_all_info_hashes when the client cannot list metadata.
    let listed = client.list_torrents().await.unwrap_or_default();
    let mut save_paths = HashMap::new();
    let hashes: HashSet<String> = if listed.is_empty() {
        client.get_all_info_hashes().await?
    } else {
        for t in &listed {
            if !t.save_path.is_empty() {
                save_paths.insert(t.info_hash.clone(), t.save_path.clone());
            }
        }
        listed.into_iter().map(|t| t.info_hash).collect()
    };

    tracing::info!(count = hashes.len(), "exporting torrents from downloader");
    let mut export_attempts = 0usize;
    let mut export_hits = 0usize;
    for hash in &hashes {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }
        if seen_info_hashes.contains(hash) {
            continue;
        }

        stats.scanned.fetch_add(1, Ordering::Relaxed);
        export_attempts += 1;
        match client.export_torrent(hash).await {
            Ok(Some(bytes)) => match parser::parse_bytes(&bytes) {
                Ok(meta) => {
                    if seen_info_hashes.insert(meta.info_hash.clone()) {
                        export_hits += 1;
                        parsed.push((None, meta));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        info_hash = %hash,
                        error = %e,
                        "failed to parse exported torrent, skipping"
                    );
                }
            },
            Ok(None) => {
                tracing::debug!(info_hash = %hash, "export unavailable for torrent, skipping");
            }
            Err(e) => {
                tracing::warn!(
                    info_hash = %hash,
                    error = %e,
                    "export failed for torrent, skipping"
                );
            }
        }
    }

    // If the client has torrents but we could not materialize any metadata (export
    // unavailable and torrent_dir empty/unusable), fail hard instead of succeeding
    // with zero matches.
    if !hashes.is_empty() && parsed.is_empty() {
        return Err(EngineError::ScanFailed(format!(
            "downloader source has {} torrents but none were materializable via torrent_dir or export (export_attempts={}, export_hits={}); configure torrent_dir or use a client that supports export",
            hashes.len(),
            export_attempts,
            export_hits
        ))
        .into());
    }

    ingest_parsed_metas(parsed, &mut result, repo, db_writer, stats, cancel).await?;
    result.save_paths = save_paths;

    if let Some(dest) = dest_client {
        result.dest_hashes = dest.get_all_info_hashes().await?;
    }

    tracing::info!(
        parsed = result.torrents.len(),
        pieces_groups = result.pieces_groups.len(),
        dest_torrents = result.dest_hashes.len(),
        save_paths = result.save_paths.len(),
        "downloader scan complete"
    );

    Ok(result)
}

fn empty_scan_result() -> ScanResult {
    ScanResult {
        torrents: HashMap::new(),
        pieces_groups: HashMap::new(),
        dest_hashes: HashSet::new(),
        save_paths: HashMap::new(),
    }
}

async fn ingest_parsed_metas(
    parsed: Vec<(Option<PathBuf>, TorrentMeta)>,
    result: &mut ScanResult,
    repo: &Repository,
    db_writer: &DbWriterHandle,
    stats: &super::stats::ReseedStats,
    cancel: &CancellationToken,
) -> Result<(), CoreError> {
    let info_hashes: Vec<String> = parsed
        .iter()
        .map(|(_, meta)| meta.info_hash.clone())
        .collect();
    let existing_hashes = repo.find_existing_info_hashes(&info_hashes).await?;

    let mut cache_batch: Vec<BulkPiecesCacheItem> = Vec::new();

    for (entry_path, meta) in parsed {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        // Incremental: skip DB write if info_hash already cached
        if existing_hashes.contains(&meta.info_hash) {
            stats.cached_skip.fetch_add(1, Ordering::Relaxed);
            result
                .pieces_groups
                .entry(meta.pieces_hash.clone())
                .or_default()
                .push(meta.info_hash.clone());
            result.torrents.insert(meta.info_hash.clone(), meta);
            continue;
        }

        // Collect announce URL (first from announce, then first of announce-list)
        let announce_url = meta.announce.clone().or_else(|| {
            meta.announce_list
                .first()
                .and_then(|tier| tier.first())
                .cloned()
        });

        cache_batch.push(BulkPiecesCacheItem {
            pieces_hash: meta.pieces_hash.clone(),
            info_hash: meta.info_hash.clone(),
            torrent_name: Some(meta.name.clone()),
            file_path: entry_path.map(|p| p.to_string_lossy().into_owned()),
            total_size: Some(meta.total_size as i64),
            announce_url,
        });

        result
            .pieces_groups
            .entry(meta.pieces_hash.clone())
            .or_default()
            .push(meta.info_hash.clone());
        result.torrents.insert(meta.info_hash.clone(), meta);

        // Flush cache in batches of 500
        if cache_batch.len() >= 500 {
            let batch = std::mem::take(&mut cache_batch);
            db_writer
                .send(WriteOp::BulkUpsertPiecesCache(batch))
                .await?;
        }
    }

    // Flush remaining
    if !cache_batch.is_empty() {
        db_writer
            .send(WriteOp::BulkUpsertPiecesCache(cache_batch))
            .await?;
    }

    Ok(())
}

/// Collect all .torrent file paths recursively from a directory.
fn collect_torrent_files(dir: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let mut result = Vec::new();

    if !dir.is_dir() {
        return Err(
            EngineError::ScanFailed(format!("path is not a directory: {}", dir.display())).into(),
        );
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CoreError> {
        let entries = std::fs::read_dir(dir).map_err(|e| {
            EngineError::ScanFailed(format!("cannot read dir {}: {}", dir.display(), e))
        })?;

        for entry in entries {
            let entry =
                entry.map_err(|e| EngineError::ScanFailed(format!("dir entry error: {}", e)))?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
            {
                out.push(path);
            }
        }
        Ok(())
    }

    walk(dir, &mut result)?;
    result.sort();
    Ok(result)
}

/// Build the announce URL set for a given pieces_hash from the cache.
pub async fn get_cached_announce_urls(
    repo: &Repository,
    pieces_hash: &str,
) -> Result<HashSet<String>, CoreError> {
    let entries = repo.find_by_pieces_hash(pieces_hash).await?;
    let mut urls = HashSet::new();
    for entry in entries {
        if let Some(url) = entry.announce_url {
            urls.insert(url);
        }
    }
    Ok(urls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crate::downloader::models::{AddTorrentOpts, TorrentInfo};
    use crate::db::writer::spawn_writer;
    use crate::engine::stats::ReseedStats;

    struct MockDownloader {
        hashes: HashSet<String>,
        exports: HashMap<String, Vec<u8>>,
    }

    #[async_trait]
    impl Downloader for MockDownloader {
        async fn connect(&mut self) -> Result<(), CoreError> {
            Ok(())
        }
        async fn test_connection(&self) -> Result<bool, CoreError> {
            Ok(true)
        }
        async fn get_torrent_info(
            &self,
            _info_hash: &str,
        ) -> Result<Option<TorrentInfo>, CoreError> {
            Ok(None)
        }
        async fn get_all_info_hashes(&self) -> Result<HashSet<String>, CoreError> {
            Ok(self.hashes.clone())
        }
        async fn add_torrent(&self, _opts: AddTorrentOpts) -> Result<bool, CoreError> {
            Ok(true)
        }
        async fn resume_torrent(&self, _info_hash: &str) -> Result<bool, CoreError> {
            Ok(true)
        }
        async fn pause_torrent(&self, _info_hash: &str) -> Result<bool, CoreError> {
            Ok(true)
        }
        async fn export_torrent(&self, info_hash: &str) -> Result<Option<Vec<u8>>, CoreError> {
            Ok(self.exports.get(info_hash).cloned())
        }
        async fn close(&mut self) -> Result<(), CoreError> {
            Ok(())
        }
    }

    fn sample_torrent_bytes(name: &str) -> Vec<u8> {
        let pieces = [9u8; 20];
        let mut data = Vec::new();
        data.extend_from_slice(b"d4:infod6:lengthi10e4:name");
        data.extend_from_slice(format!("{}:{}", name.len(), name).as_bytes());
        data.extend_from_slice(b"12:piece lengthi10e6:pieces20:");
        data.extend_from_slice(&pieces);
        data.extend_from_slice(b"ee");
        data
    }

    #[tokio::test]
    async fn scan_downloader_parses_exported_torrent() {
        let db_dir = tempfile::tempdir().unwrap();
        let database_url = format!("sqlite://{}", db_dir.path().join("test.db").display());
        let pool = crate::db::init_db(&database_url).await.unwrap();
        let repo = Repository::new(pool);
        let writer = spawn_writer(&database_url, 10).unwrap();
        let bytes = sample_torrent_bytes("a");
        let meta = parser::parse_bytes(&bytes).unwrap();
        let client = MockDownloader {
            hashes: HashSet::from([meta.info_hash.clone()]),
            exports: HashMap::from([(meta.info_hash.clone(), bytes)]),
        };
        let dest = MockDownloader {
            hashes: HashSet::new(),
            exports: HashMap::new(),
        };
        let stats = ReseedStats::new();
        let cancel = CancellationToken::new();

        let result = scan_downloader(
            &client,
            None,
            &repo,
            &writer,
            Some(&dest),
            &stats,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(result.torrents.len(), 1);
        assert!(result.torrents.contains_key(&meta.info_hash));
        assert!(!result.pieces_groups.is_empty());
    }

    #[tokio::test]
    async fn scan_downloader_uses_torrent_dir_fallback() {
        let db_dir = tempfile::tempdir().unwrap();
        let database_url = format!("sqlite://{}", db_dir.path().join("test.db").display());
        let pool = crate::db::init_db(&database_url).await.unwrap();
        let repo = Repository::new(pool);
        let writer = spawn_writer(&database_url, 10).unwrap();

        let torrent_dir = tempfile::tempdir().unwrap();
        let bytes = sample_torrent_bytes("dir-torrent");
        let meta = parser::parse_bytes(&bytes).unwrap();
        std::fs::write(torrent_dir.path().join("x.torrent"), &bytes).unwrap();

        let client = MockDownloader {
            hashes: HashSet::new(),
            exports: HashMap::new(),
        };
        let dest = MockDownloader {
            hashes: HashSet::new(),
            exports: HashMap::new(),
        };
        let stats = ReseedStats::new();
        let cancel = CancellationToken::new();

        let result = scan_downloader(
            &client,
            Some(torrent_dir.path()),
            &repo,
            &writer,
            Some(&dest),
            &stats,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(result.torrents.len(), 1);
        assert!(result.torrents.contains_key(&meta.info_hash));
    }
}
