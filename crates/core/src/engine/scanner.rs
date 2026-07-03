use std::collections::{HashMap, HashSet};
use std::path::Path;
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
    let mut torrents: HashMap<String, TorrentMeta> = HashMap::new();
    let mut pieces_groups: HashMap<String, Vec<String>> = HashMap::new();

    // Collect all .torrent file paths
    let entries = collect_torrent_files(path)?;
    tracing::info!(folder = %path.display(), count = entries.len(), "scanning torrent files");

    // Batch parse
    let mut cache_batch: Vec<BulkPiecesCacheItem> = Vec::new();

    for entry_path in &entries {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        stats.scanned.fetch_add(1, Ordering::Relaxed);

        let meta = match parser::parse_file(entry_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    path = %entry_path.display(),
                    error = %e,
                    "failed to parse torrent file, skipping"
                );
                continue;
            }
        };

        // Incremental: skip if info_hash already cached
        if repo.find_by_info_hash(&meta.info_hash).await?.is_some() {
            stats.cached_skip.fetch_add(1, Ordering::Relaxed);
            // Still track for matching
            pieces_groups
                .entry(meta.pieces_hash.clone())
                .or_default()
                .push(meta.info_hash.clone());
            torrents.insert(meta.info_hash.clone(), meta);
            continue;
        }

        // Collect announce URL (first from announce, then first of announce-list)
        let announce_url = meta
            .announce
            .clone()
            .or_else(|| {
                meta.announce_list
                    .first()
                    .and_then(|tier| tier.first())
                    .cloned()
            });

        cache_batch.push(BulkPiecesCacheItem {
            pieces_hash: meta.pieces_hash.clone(),
            info_hash: meta.info_hash.clone(),
            torrent_name: Some(meta.name.clone()),
            file_path: Some(entry_path.to_string_lossy().into_owned()),
            total_size: Some(meta.total_size as i64),
            announce_url,
        });

        pieces_groups
            .entry(meta.pieces_hash.clone())
            .or_default()
            .push(meta.info_hash.clone());
        torrents.insert(meta.info_hash.clone(), meta);

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

    // Get all info_hashes from destination downloader for dedup
    let dest_hashes = dest_client.get_all_info_hashes().await?;

    tracing::info!(
        parsed = torrents.len(),
        pieces_groups = pieces_groups.len(),
        dest_torrents = dest_hashes.len(),
        "scan complete"
    );

    Ok(ScanResult {
        torrents,
        pieces_groups,
        dest_hashes,
    })
}

/// Scan from a downloader's torrent_dir: read .torrent files from the directory
/// where the downloader stores them.
pub async fn scan_downloader(
    _client: &dyn Downloader,
    torrent_dir: &Path,
    repo: &Repository,
    db_writer: &DbWriterHandle,
    dest_client: &dyn Downloader,
    stats: &super::stats::ReseedStats,
    cancel: &CancellationToken,
) -> Result<ScanResult, CoreError> {
    // Downloaders store .torrent files in torrent_dir; scan it just like a folder
    scan_folder(torrent_dir, repo, db_writer, dest_client, stats, cancel).await
}

/// Collect all .torrent file paths recursively from a directory.
fn collect_torrent_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, CoreError> {
    let mut result = Vec::new();

    if !dir.is_dir() {
        return Err(EngineError::ScanFailed(format!(
            "path is not a directory: {}",
            dir.display()
        ))
        .into());
    }

    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), CoreError> {
        let entries = std::fs::read_dir(dir).map_err(|e| {
            EngineError::ScanFailed(format!("cannot read dir {}: {}", dir.display(), e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                EngineError::ScanFailed(format!("dir entry error: {}", e))
            })?;
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
