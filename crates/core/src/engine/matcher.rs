use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use futures::future;
use tokio_util::sync::CancellationToken;
use tracing;

use crate::db::repo::Repository;
use crate::error::{CoreError, EngineError, SiteError};
use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

use super::adder::MatchedTorrent;
use super::filter;
use super::scanner::ScanResult;
use super::stats::ReseedStats;

/// Per-site batch size, adaptive: starts at site definition value, halves on 413/400/500.
struct SiteBatchState {
    batch_size: usize,
    min_batch_size: usize,
}

impl SiteBatchState {
    fn new(initial: usize) -> Self {
        Self {
            batch_size: initial,
            min_batch_size: 50,
        }
    }

    fn halve(&mut self) {
        let new_size = (self.batch_size / 2).max(self.min_batch_size);
        if new_size != self.batch_size {
            tracing::warn!(
                old = self.batch_size,
                new = new_size,
                "adaptive batch_size reduction"
            );
            self.batch_size = new_size;
        }
    }
}

/// Match result from one site: list of (pieces_hash, torrent_id) hits.
pub type SiteMatches = Vec<(String, i64)>;

/// A `(site_id, pieces_hash)` pair Filter-2 skipped because `reseed_history`
/// already has a `status='success'` row for it.
///
/// Not a `MatchedTorrent`: there is no `download_url` and nothing will be
/// downloaded or added. It exists only to produce a run-detail row.
#[derive(Debug, Clone)]
pub struct HistorySkippedTorrent {
    pub pieces_hash: String,
    pub site_id: SiteId,
    /// Newest non-NULL `torrent_id` from history; detail-link decoration only.
    pub torrent_id: Option<i64>,
    pub save_path: String,
}

/// Everything phase 2 produces.
#[derive(Debug, Default)]
pub struct MatchPhaseOutput {
    pub matched: Vec<MatchedTorrent>,
    pub history_skipped: Vec<HistorySkippedTorrent>,
}

/// Run the match phase: query all target sites in parallel for pieces_hash matches,
/// applying three-layer filtering before and after.
///
/// Returns matched torrents ready for the adder, plus history-skipped pairs
/// that should appear in the run-detail list without being queried or added.
// 参数多但各自独立（数据源、注册表、目标、仓储、四个行为开关）。拆分超长函数
// 与参数打包不在本次重构范围内（见 .omc/specs 的 Non-Goals）。
#[allow(clippy::too_many_arguments)]
pub async fn match_all_sites(
    scan: &ScanResult,
    registry: &SiteRegistry,
    target_site_ids: &[SiteId],
    repo: &Repository,
    default_save_path: &str,
    skip_hash_check: bool,
    tag: Option<&str>,
    stats: &ReseedStats,
    cancel: &CancellationToken,
) -> Result<MatchPhaseOutput, CoreError> {
    if scan.pieces_groups.is_empty() {
        return Ok(MatchPhaseOutput::default());
    }

    // pieces_hash -> preferred save_path from any source info_hash that has one.
    let save_path_by_pieces: HashMap<String, String> = scan
        .pieces_groups
        .iter()
        .filter_map(|(pieces_hash, info_hashes)| {
            info_hashes
                .iter()
                .find_map(|ih| scan.save_paths.get(ih).cloned())
                .map(|path| (pieces_hash.clone(), path))
        })
        .collect();

    let all_pieces_hashes: Arc<[String]> = scan.pieces_groups.keys().cloned().collect();

    tracing::info!(
        sites = target_site_ids.len(),
        pieces_hashes = all_pieces_hashes.len(),
        "starting match phase"
    );

    // Batch-load announce URLs for all pieces hashes once.
    let announce_by_hash = repo
        .find_announce_urls_by_pieces_hashes(all_pieces_hashes.as_ref())
        .await?;

    // Launch one task per site, all in parallel
    let mut handles = Vec::new();
    let mut history_skipped: Vec<HistorySkippedTorrent> = Vec::new();

    for &site_id in target_site_ids {
        let handle = registry.get(&site_id);
        let handle = match handle {
            Some(h) => h,
            None => {
                tracing::warn!(site_id = site_id.0, "site not in registry, skipping");
                continue;
            }
        };

        // Must have reseed capability
        let reseed = match &handle.reseed {
            Some(r) => Arc::clone(r),
            None => {
                tracing::debug!(
                    site_id = site_id.0,
                    "site lacks Reseed capability, skipping"
                );
                continue;
            }
        };

        let rate_limiter = Arc::clone(&handle.rate_limiter);
        let base_url = handle.core.base_url().to_string();

        let initial_batch_size = reseed.batch_size().max(1);

        let hashes = Arc::clone(&all_pieces_hashes);
        let repo = repo.clone();
        let cancel = cancel.clone();
        let default_save_path = default_save_path.to_string();
        let save_path_by_pieces = save_path_by_pieces.clone();
        let tag = tag.map(String::from);

        // Filter-1: tracker pre-filter using preloaded announce URLs
        let mut tracker_filtered: HashSet<String> = HashSet::new();
        for ph in hashes.iter() {
            let cached_urls = announce_by_hash.get(ph).cloned().unwrap_or_default();
            if filter::filter_by_tracker(&cached_urls, &base_url) {
                tracker_filtered.insert(ph.clone());
                stats.skipped_tracker.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Remaining hashes after tracker filter
        let remaining: Vec<String> = hashes
            .iter()
            .filter(|h| !tracker_filtered.contains(*h))
            .cloned()
            .collect();

        // Filter-2: history check via batch load. Success alone is the skip
        // criterion — the info_hash no longer has to still be present in a
        // downloader.
        let history_by_hash = repo
            .find_successful_reseed_torrent_ids(&remaining, site_id.0)
            .await?;
        let mut query_hashes: Vec<String> = Vec::with_capacity(remaining.len());
        for ph in remaining {
            // Copy out first so the map borrow ends before `ph` moves (E0505).
            let history_torrent_id = history_by_hash.get(&ph).copied();
            match history_torrent_id {
                Some(torrent_id) => {
                    stats.skipped_history.fetch_add(1, Ordering::Relaxed);
                    let save_path = save_path_by_pieces
                        .get(&ph)
                        .cloned()
                        .unwrap_or_else(|| default_save_path.clone());
                    history_skipped.push(HistorySkippedTorrent {
                        pieces_hash: ph,
                        site_id,
                        torrent_id,
                        save_path,
                    });
                }
                None => query_hashes.push(ph),
            }
        }

        if query_hashes.is_empty() {
            tracing::debug!(site_id = site_id.0, "all hashes filtered, skipping site");
            continue;
        }

        let cancel_inner = cancel.clone();
        handles.push(tokio::spawn(async move {
            match_single_site(
                site_id,
                reseed,
                rate_limiter,
                query_hashes,
                initial_batch_size,
                &default_save_path,
                &save_path_by_pieces,
                skip_hash_check,
                tag.as_deref(),
                &cancel_inner,
            )
            .await
        }));
    }

    // Await all site tasks
    let results = future::join_all(handles).await;

    let mut matched_torrents = Vec::new();
    for result in results {
        match result {
            Ok(Ok(matches)) => {
                stats
                    .matched
                    .fetch_add(matches.len() as u64, Ordering::Relaxed);
                matched_torrents.extend(matches);
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, "site matching failed");
            }
            Err(e) => {
                tracing::error!(error = %e, "site matching task panicked");
            }
        }
    }

    tracing::info!(
        total_matches = matched_torrents.len(),
        history_skipped = history_skipped.len(),
        "match phase complete"
    );

    Ok(MatchPhaseOutput {
        matched: matched_torrents,
        history_skipped,
    })
}

/// Query a single site with batched pieces_hash requests, respecting rate limits.
// 单站点匹配的入参由 match_all_sites 逐项透传，打包成 struct 只会把复杂度
// 搬到调用方。
#[allow(clippy::too_many_arguments)]
async fn match_single_site(
    site_id: SiteId,
    reseed: Arc<dyn crate::site::traits::ReseedCapable>,
    rate_limiter: Arc<crate::site::rate_limiter::SiteRateLimiter>,
    hashes: Vec<String>,
    initial_batch_size: usize,
    default_save_path: &str,
    save_path_by_pieces: &HashMap<String, String>,
    skip_hash_check: bool,
    tag: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Vec<MatchedTorrent>, CoreError> {
    let mut batch_state = SiteBatchState::new(initial_batch_size);
    let mut all_matches = Vec::new();

    let mut cursor = 0usize;

    while cursor < hashes.len() {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        let end = (cursor + batch_state.batch_size).min(hashes.len());
        let chunk = &hashes[cursor..end];

        // Rate limit
        rate_limiter.acquire().await?;

        tracing::debug!(
            site_id = site_id.0,
            start = cursor,
            end,
            total = hashes.len(),
            batch_size = chunk.len(),
            "querying pieces_hash batch"
        );

        match reseed.query_pieces_hash(chunk).await {
            Ok(matches) => {
                rate_limiter.record_success();

                for (pieces_hash, torrent_id) in matches {
                    let download_url = reseed.build_download_url(torrent_id);
                    let save_path = save_path_by_pieces
                        .get(&pieces_hash)
                        .cloned()
                        .unwrap_or_else(|| default_save_path.to_string());
                    all_matches.push(MatchedTorrent {
                        pieces_hash,
                        site_id,
                        torrent_id: Some(torrent_id),
                        download_url,
                        save_path,
                        skip_hash_check,
                        tag: tag.map(String::from),
                    });
                }
                cursor = end;
            }
            Err(e) => {
                let is_batch_too_large = matches!(
                    &e,
                    CoreError::Site(SiteError::HttpError(msg))
                    if msg.contains("413") || msg.contains("400") || msg.contains("500")
                );

                if is_batch_too_large && batch_state.batch_size > batch_state.min_batch_size {
                    batch_state.halve();
                    tracing::warn!(
                        site_id = site_id.0,
                        new_batch_size = batch_state.batch_size,
                        "batch too large, reducing batch_size and retrying from current cursor"
                    );
                    continue;
                }

                let tripped = rate_limiter.record_error().await;
                tracing::error!(
                    site_id = site_id.0,
                    error = %e,
                    circuit_tripped = tripped,
                    "pieces_hash query failed"
                );
                if tripped {
                    tracing::warn!(
                        site_id = site_id.0,
                        "circuit breaker tripped, aborting site"
                    );
                    return Ok(all_matches);
                }
                cursor = end;
            }
        }
    }

    tracing::info!(
        site_id = site_id.0,
        matches = all_matches.len(),
        "site matching complete"
    );

    Ok(all_matches)
}
