use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing;

use crate::db::writer::{DbWriterHandle, WriteOp};
use crate::downloader::models::AddTorrentOpts;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, EngineError};
use crate::site::models::SiteId;
use crate::site::rate_limiter::{
    is_download_rate_limited, parse_wait_seconds, SiteRateLimiter, MAX_DOWNLOAD_WAIT_SECS,
};
use crate::torrent::parser;

use super::stats::ReseedStats;

/// Extra attempts after a site says "please wait N seconds".
const DOWNLOAD_RATE_LIMIT_RETRIES: u32 = 3;

/// A match result from the matcher phase: one torrent to add to the destination.
#[derive(Debug, Clone)]
pub struct MatchedTorrent {
    pub pieces_hash: String,
    pub site_id: SiteId,
    pub torrent_id: Option<i64>,
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
// 下载、校验、去重、入库和站点限速共用一条路径；拆 struct 不在本次范围内。
#[allow(clippy::too_many_arguments)]
pub async fn add_torrent(
    matched: &MatchedTorrent,
    http_client: &reqwest::Client,
    dest_client: &dyn Downloader,
    dest_hashes: &Arc<Mutex<HashSet<String>>>,
    auto_start: bool,
    db_writer: &DbWriterHandle,
    stats: &ReseedStats,
    rate_limiter: Option<&SiteRateLimiter>,
    task_id: Option<i64>,
) -> Result<bool, CoreError> {
    tracing::info!(
        task_id,
        site_id = matched.site_id.0,
        torrent_id = ?matched.torrent_id,
        pieces_hash = %matched.pieces_hash,
        "downloading torrent for reseed"
    );
    // Download .torrent file
    let torrent_data = download_torrent(http_client, &matched.download_url, rate_limiter).await?;

    // Parse and verify pieces_hash matches
    let meta = parser::parse_bytes(&torrent_data)?;
    if meta.pieces_hash != matched.pieces_hash {
        tracing::warn!(
            task_id,
            expected = %matched.pieces_hash,
            got = %meta.pieces_hash,
            url = %redact_url(&matched.download_url),
            "pieces_hash mismatch after download, skipping"
        );
        record_history(
            db_writer,
            &matched.pieces_hash,
            matched.site_id,
            matched.torrent_id,
            Some(&meta.info_hash),
            "failed",
            Some("pieces_hash mismatch after download"),
        )
        .await?;
        stats.failed.fetch_add(1, Ordering::Relaxed);
        return Ok(false);
    }

    // Filter 3: claim info_hash under the lock so concurrent adders cannot
    // race past the same dedup check.
    {
        let mut hashes = dest_hashes.lock().await;
        if !hashes.insert(meta.info_hash.clone()) {
            tracing::debug!(
                task_id,
                info_hash = %meta.info_hash,
                "destination already has this torrent, skipping"
            );
            drop(hashes);
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                matched.torrent_id,
                Some(&meta.info_hash),
                "skipped",
                Some("already in destination downloader"),
            )
            .await?;
            stats.skipped_exists.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
    }

    // Add to destination downloader
    let opts = AddTorrentOpts {
        torrent_data,
        save_path: matched.save_path.clone(),
        skip_hash_check: matched.skip_hash_check,
        paused: !auto_start,
        tag: matched.tag.clone(),
    };

    match dest_client.add_torrent(opts).await {
        Ok(true) => {
            tracing::info!(
                task_id,
                pieces_hash = %matched.pieces_hash,
                info_hash = %meta.info_hash,
                site_id = matched.site_id.0,
                torrent_id = ?matched.torrent_id,
                "torrent added successfully"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                matched.torrent_id,
                Some(&meta.info_hash),
                "success",
                None,
            )
            .await?;
            stats.added.fetch_add(1, Ordering::Relaxed);
            if auto_start {
                if let Err(e) = dest_client.resume_torrent(&meta.info_hash).await {
                    tracing::warn!(
                        task_id,
                        info_hash = %meta.info_hash,
                        error = %e,
                        "torrent added but explicit resume failed"
                    );
                }
            }
            Ok(true)
        }
        Ok(false) => {
            // Keep the claim: downloader already has it (or equivalent).
            tracing::warn!(
                task_id,
                info_hash = %meta.info_hash,
                "downloader rejected torrent (already exists or other reason)"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                matched.torrent_id,
                Some(&meta.info_hash),
                "skipped",
                Some("downloader rejected (likely duplicate)"),
            )
            .await?;
            stats.skipped_exists.fetch_add(1, Ordering::Relaxed);
            Ok(false)
        }
        Err(e) => {
            // Release claim so a later retry can re-attempt.
            dest_hashes.lock().await.remove(&meta.info_hash);
            tracing::error!(
                task_id,
                info_hash = %meta.info_hash,
                error = %e,
                "failed to add torrent to destination"
            );
            record_history(
                db_writer,
                &matched.pieces_hash,
                matched.site_id,
                matched.torrent_id,
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
///
/// Site-level download spacing is applied when a limiter is present. HTTP 500
/// bodies that say "下载过于频繁，请等待 N 秒" are treated as rate limits:
/// wait, then retry a few times instead of failing the add.
async fn download_torrent(
    client: &reqwest::Client,
    url: &str,
    rate_limiter: Option<&SiteRateLimiter>,
) -> Result<Vec<u8>, CoreError> {
    let mut last_wait = None;
    for attempt in 0..=DOWNLOAD_RATE_LIMIT_RETRIES {
        if let Some(limiter) = rate_limiter {
            limiter.acquire_download().await?;
        }

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| EngineError::AddFailed(format!("download torrent: {}", e)))?;

        let status = resp.status();
        if status.is_success() {
            if let Some(limiter) = rate_limiter {
                limiter.record_success();
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| EngineError::AddFailed(format!("read torrent body: {}", e)))?;
            return Ok(bytes.to_vec());
        }

        let body = resp.text().await.unwrap_or_default();
        let preview = preview_body(&body);
        if is_download_rate_limited(status, &body) && attempt < DOWNLOAD_RATE_LIMIT_RETRIES {
            let wait_secs = parse_wait_seconds(&body).unwrap_or(5);
            last_wait = Some((status, preview.clone(), wait_secs));
            tracing::warn!(
                status = %status,
                wait_secs,
                attempt = attempt + 1,
                url = %redact_url(url),
                body = %preview,
                "torrent download rate-limited, waiting before retry"
            );
            if let Some(limiter) = rate_limiter {
                limiter.record_extra_wait(wait_secs);
                let _ = limiter.record_error().await;
            } else {
                let capped = wait_secs.min(MAX_DOWNLOAD_WAIT_SECS);
                tokio::time::sleep(std::time::Duration::from_secs(capped)).await;
            }
            continue;
        }

        if let Some(limiter) = rate_limiter {
            let _ = limiter.record_error().await;
        }
        return Err(EngineError::AddFailed(format!(
            "download torrent HTTP {} from {}",
            status,
            redact_url(url)
        ))
        .into());
    }

    let (status, _preview, wait_secs) =
        last_wait.unwrap_or((reqwest::StatusCode::INTERNAL_SERVER_ERROR, String::new(), 0));
    Err(EngineError::AddFailed(format!(
        "download torrent HTTP {} after {} retries (last wait {}s) from {}",
        status,
        DOWNLOAD_RATE_LIMIT_RETRIES,
        wait_secs,
        redact_url(url)
    ))
    .into())
}

fn redact_url(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

fn preview_body(body: &str) -> String {
    const MAX: usize = 200;
    let trimmed = body.trim();
    let collapsed: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX {
        collapsed
    } else {
        collapsed.chars().take(MAX).collect()
    }
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
