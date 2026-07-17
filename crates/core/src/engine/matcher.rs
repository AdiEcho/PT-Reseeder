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

/// Per-site batch size, adaptive: starts at site definition value, halves on 413/400.
struct SiteBatchState {
    batch_size: usize,
    min_batch_size: usize,
}

impl SiteBatchState {
    fn new(initial: usize) -> Self {
        Self {
            batch_size: initial,
            min_batch_size: 100,
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

/// Run the match phase: query all target sites in parallel for pieces_hash matches,
/// applying three-layer filtering before and after.
///
/// Returns a flat list of `MatchedTorrent` ready for the adder.
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
) -> Result<Vec<MatchedTorrent>, CoreError> {
    if scan.pieces_groups.is_empty() {
        return Ok(Vec::new());
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
    let present_info_hashes: Arc<HashSet<String>> = Arc::new(
        scan.dest_hashes
            .iter()
            .cloned()
            .chain(scan.torrents.keys().cloned())
            .collect(),
    );

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
        let present_info_hashes = Arc::clone(&present_info_hashes);
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

        // Filter-2: history check via batch load
        let history_by_hash = repo
            .find_successful_reseed_info_hashes(&remaining, site_id.0)
            .await?;
        let mut history_filtered: HashSet<String> = HashSet::new();
        for ph in &remaining {
            if let Some(success_hashes) = history_by_hash.get(ph) {
                if success_hashes
                    .iter()
                    .any(|ih| present_info_hashes.contains(ih))
                {
                    history_filtered.insert(ph.clone());
                    stats.skipped_history.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Remaining hashes to query
        let query_hashes: Vec<String> = remaining
            .into_iter()
            .filter(|h| !history_filtered.contains(h))
            .collect();

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
                &base_url,
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
        "match phase complete"
    );

    Ok(matched_torrents)
}

/// Query a single site with batched pieces_hash requests, respecting rate limits.
async fn match_single_site(
    site_id: SiteId,
    reseed: Arc<dyn crate::site::traits::ReseedCapable>,
    rate_limiter: Arc<crate::site::rate_limiter::SiteRateLimiter>,
    _base_url: &str,
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
                    if msg.contains("413") || msg.contains("400")
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
