use std::collections::HashSet;
use std::sync::atomic::Ordering;

use tracing;

use crate::db::writer::{DbWriterHandle, WriteOp};
use crate::downloader::models::AddTorrentOpts;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, EngineError};
use crate::site::models::SiteId;
use crate::torrent::parser;

use super::stats::ReseedStats;

/// A match result from the matcher phase: one torrent to add to the destination.
#[derive(Debug, Clone)]
pub struct MatchedTorrent {
    pub pieces_hash: String,
    pub site_id: SiteId,
    pub torrent_id: i64,
    pub download_url: String,
    /// save_path from the source torrent in the downloader.
    pub save_path: String,
    /// Whether to skip hash check on the destination.
    pub skip_hash_check: bool,
    /// Tag to apply.
    pub tag: Option<String>,
}

/// Download the .torrent from the site, verify its pieces_hash, and add it
/// to the destination downloader.
pub async fn add_torrent(
    matched: &MatchedTorrent,
    http_client: &reqwest::Client,
    dest_client: &dyn Downloader,
    dest_hashes: &HashSet<String>,
    db_writer: &DbWriterHandle,
    stats: &ReseedStats,
) -> Result<bool, CoreError> {
    // Download .torrent file
    let torrent_data = download_torrent(http_client, &matched.download_url).await?;

    // Parse and verify pieces_hash matches
    let meta = parser::parse_bytes(&torrent_data)?;
    if meta.pieces_hash != matched.pieces_hash {
        tracing::warn!(
            expected = %matched.pieces_hash,
            got = %meta.pieces_hash,
            url = %matched.download_url,
            "pieces_hash mismatch after download, skipping"
        );
        record_history(
            db_writer,
            &matched.pieces_hash,
            matched.site_id,
            Some(matched.torrent_id),
            Some(&meta.info_hash),
            "failed",
            Some("pieces_hash mismatch after download"),
        )
        .await?;
        stats.failed.fetch_add(1, Ordering::Relaxed);
        return Ok(false);
    }

    // Filter 3: info_hash dedup — destination already has it
    if dest_hashes.contains(&meta.info_hash) {
        tracing::debug!(
            info_hash = %meta.info_hash,
            "destination already has this torrent, skipping"
        );
        record_history(
            db_writer,
            &matched.pieces_hash,
            matched.site_id,
            Some(matched.torrent_id),
            Some(&meta.info_hash),
            "skipped",
            Some("already in destination downloader"),
        )
        .await?;
        stats.skipped_exists.fetch_add(1, Ordering::Relaxed);
        return Ok(false);
    }

    // Add to destination downloader
    let opts = AddTorrentOpts {
        torrent_data,
        save_path: matched.save_path.clone(),
        skip_hash_check: matched.skip_hash_check,
        paused: false,
        tag: matched.tag.clone(),
    };

    match dest_client.add_torrent(opts).await {
        Ok(true) => {
            tracing::info!(
                pieces_hash = %matched.pieces_hash,
                info_hash = %meta.info_hash,
                site_id = matched.site_id.0,
                torrent_id = matched.torrent_id,
                "torrent added successfully"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                Some(matched.torrent_id),
                Some(&meta.info_hash),
                "success",
                None,
            )
            .await?;
            stats.added.fetch_add(1, Ordering::Relaxed);
            Ok(true)
        }
        Ok(false) => {
            tracing::warn!(
                info_hash = %meta.info_hash,
                "downloader rejected torrent (already exists or other reason)"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                Some(matched.torrent_id),
                Some(&meta.info_hash),
                "skipped",
                Some("downloader rejected (likely duplicate)"),
            )
            .await?;
            stats.skipped_exists.fetch_add(1, Ordering::Relaxed);
            Ok(false)
        }
        Err(e) => {
            tracing::error!(
                info_hash = %meta.info_hash,
                error = %e,
                "failed to add torrent to destination"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                Some(matched.torrent_id),
                Some(&meta.info_hash),
                "failed",
                Some(&format!("add failed: {}", e)),
            )
            .await?;
            stats.failed.fetch_add(1, Ordering::Relaxed);
            Ok(false)
        }
    }
}

/// Download a .torrent file from a URL.
async fn download_torrent(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<u8>, CoreError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| EngineError::AddFailed(format!("download torrent: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(EngineError::AddFailed(format!(
            "download torrent HTTP {}: {}",
            status, url
        ))
        .into());
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| EngineError::AddFailed(format!("read torrent body: {}", e)))?;

    Ok(bytes.to_vec())
}

/// Record a reseed attempt in the history via DbWriter.
async fn record_history(
    db_writer: &DbWriterHandle,
    pieces_hash: &str,
    site_id: SiteId,
    torrent_id: Option<i64>,
    info_hash: Option<&str>,
    status: &str,
    error_reason: Option<&str>,
) -> Result<(), CoreError> {
    db_writer
        .send(WriteOp::InsertReseedHistory {
            pieces_hash: pieces_hash.to_string(),
            site_id: site_id.0,
            torrent_id,
            info_hash: info_hash.map(String::from),
            status: status.to_string(),
            error_reason: error_reason.map(String::from),
        })
        .await
}
