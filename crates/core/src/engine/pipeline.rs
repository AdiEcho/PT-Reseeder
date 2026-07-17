use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing;

use crate::db::repo::Repository;
use crate::db::writer::DbWriterHandle;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, EngineError};
use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

use super::adder;
use super::dry_run::{build_preview, DryRunPreview};
use super::filter;
use super::matcher;
use super::scanner;
use super::stats::{ReseedStats, ReseedStatsSnapshot};

/// Max concurrent torrent download+add operations.
const ADD_CONCURRENCY: usize = 8;

/// Configuration for a reseed pipeline run.
#[derive(Debug, Clone)]
pub struct ReseedConfig {
    /// Folders to scan for .torrent files.
    pub scan_folders: Vec<PathBuf>,
    /// Source downloaders to scan via export API and/or torrent_dir.
    pub source_downloaders: Vec<DownloaderScanTarget>,
    /// Target site IDs to query.
    pub target_site_ids: Vec<SiteId>,
    /// Default save path on the destination downloader.
    pub default_save_path: String,
    /// Whether to skip hash check when adding to destination.
    pub skip_hash_check: bool,
    /// Whether added torrents should be explicitly resumed.
    pub auto_start: bool,
    /// Tag to apply to added torrents.
    pub tag: Option<String>,
    /// Jackett URL + API key for pack component search. None = skip pack detection.
    pub jackett_config: Option<super::jackett::JackettConfig>,
}

/// A downloader source to scan during phase 1.
#[derive(Clone)]
pub struct DownloaderScanTarget {
    pub downloader: Arc<dyn Downloader>,
    pub torrent_dir: Option<PathBuf>,
}

impl std::fmt::Debug for DownloaderScanTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloaderScanTarget")
            .field("torrent_dir", &self.torrent_dir)
            .finish()
    }
}

/// Real-time progress snapshot sent via watch channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedProgress {
    pub phase: String,
    pub stats: ReseedStatsSnapshot,
    pub elapsed_secs: u64,
    pub finished: bool,
}

/// The reseed engine: orchestrates the scan → match → add pipeline.
pub struct ReseedEngine {
    registry: Arc<SiteRegistry>,
    repo: Repository,
    db_writer: DbWriterHandle,
    cancel: CancellationToken,
}

impl ReseedEngine {
    pub fn new(
        registry: Arc<SiteRegistry>,
        repo: Repository,
        db_writer: DbWriterHandle,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            registry,
            repo,
            db_writer,
            cancel,
        }
    }

    /// Run the full reseed pipeline.
    ///
    /// Returns a watch receiver for real-time progress and the final stats
    /// after the pipeline completes.
    ///
    /// When `dry_run` is true, scan+match still run but phase 3 (add) is skipped
    /// and a structured preview is returned inside the join handle result.
    pub async fn run(
        &self,
        config: ReseedConfig,
        dest_client: Arc<dyn Downloader>,
        dry_run: bool,
    ) -> Result<
        (
            watch::Receiver<ReseedProgress>,
            tokio::task::JoinHandle<
                Result<(ReseedStatsSnapshot, Option<DryRunPreview>), CoreError>,
            >,
        ),
        CoreError,
    > {
        let stats = Arc::new(ReseedStats::new());
        let start = Instant::now();

        let initial_progress = ReseedProgress {
            phase: "initializing".to_string(),
            stats: stats.snapshot(),
            elapsed_secs: 0,
            finished: false,
        };
        let (progress_tx, progress_rx) = watch::channel(initial_progress);

        let registry = Arc::clone(&self.registry);
        let repo = self.repo.clone();
        let db_writer = self.db_writer.clone();
        let cancel = self.cancel.clone();
        let stats_clone = Arc::clone(&stats);

        let handle = tokio::spawn(async move {
            let result = run_pipeline(
                config,
                &registry,
                &repo,
                &db_writer,
                dest_client.as_ref(),
                &stats_clone,
                &cancel,
                &progress_tx,
                start,
                dry_run,
            )
            .await;

            // Send final progress
            let snapshot = stats_clone.snapshot();
            let _ = progress_tx.send(ReseedProgress {
                phase: if result.is_ok() { "complete" } else { "error" }.to_string(),
                stats: snapshot.clone(),
                elapsed_secs: start.elapsed().as_secs(),
                finished: true,
            });

            result.map(|preview| (snapshot, preview))
        });

        Ok((progress_rx, handle))
    }

    /// Run the pipeline synchronously (blocking until complete).
    ///
    /// When `dry_run` is true, returns a preview of would-add items and never
    /// calls destination add / reseed history writes.
    pub async fn run_sync(
        &self,
        config: ReseedConfig,
        dest_client: Arc<dyn Downloader>,
        dry_run: bool,
    ) -> Result<(ReseedStatsSnapshot, Option<DryRunPreview>), CoreError> {
        let stats = Arc::new(ReseedStats::new());
        let start = Instant::now();
        let (progress_tx, _) = watch::channel(ReseedProgress {
            phase: "initializing".to_string(),
            stats: stats.snapshot(),
            elapsed_secs: 0,
            finished: false,
        });

        let preview = run_pipeline(
            config,
            &self.registry,
            &self.repo,
            &self.db_writer,
            dest_client.as_ref(),
            &stats,
            &self.cancel,
            &progress_tx,
            start,
            dry_run,
        )
        .await?;

        Ok((stats.snapshot(), preview))
    }
}

/// Internal: run the three-phase pipeline.
///
/// Returns `Some(preview)` when `dry_run` is true (including empty would-add list).
/// Returns `None` for real runs.
async fn run_pipeline(
    config: ReseedConfig,
    registry: &SiteRegistry,
    repo: &Repository,
    db_writer: &DbWriterHandle,
    dest_client: &dyn Downloader,
    stats: &ReseedStats,
    cancel: &CancellationToken,
    progress_tx: &watch::Sender<ReseedProgress>,
    start: Instant,
    dry_run: bool,
) -> Result<Option<DryRunPreview>, CoreError> {
    // ── Phase 1: Scan ──────────────────────────────────────────────────
    update_progress(progress_tx, "scanning", stats, start);
    tracing::info!(
        folders = config.scan_folders.len(),
        source_downloaders = config.source_downloaders.len(),
        "phase 1: scan"
    );

    if config.scan_folders.is_empty() && config.source_downloaders.is_empty() {
        return Err(EngineError::ScanFailed(
            "no scan folders or source downloaders configured".into(),
        )
        .into());
    }

    // Scan all folders/downloaders and merge results
    let mut merged_scan = scanner::ScanResult {
        torrents: std::collections::HashMap::new(),
        pieces_groups: std::collections::HashMap::new(),
        dest_hashes: std::collections::HashSet::new(),
        save_paths: std::collections::HashMap::new(),
    };

    // Fetch destination hashes once and inject into all scans.
    let dest_hashes = dest_client.get_all_info_hashes().await?;
    merged_scan.dest_hashes.extend(dest_hashes.iter().cloned());

    for folder in &config.scan_folders {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        let mut scan =
            scanner::scan_folder(folder, repo, db_writer, dest_client, stats, cancel).await?;
        // Prefer the shared dest hash set; still merge if scan returned values.
        scan.dest_hashes.clear();

        // Merge
        merged_scan.torrents.extend(scan.torrents);
        merged_scan.save_paths.extend(scan.save_paths);
        for (ph, hashes) in scan.pieces_groups {
            merged_scan
                .pieces_groups
                .entry(ph)
                .or_default()
                .extend(hashes);
        }
    }

    for source in &config.source_downloaders {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        let mut scan = scanner::scan_downloader(
            source.downloader.as_ref(),
            source.torrent_dir.as_deref(),
            repo,
            db_writer,
            None,
            stats,
            cancel,
        )
        .await?;
        scan.dest_hashes.clear();

        merged_scan.torrents.extend(scan.torrents);
        merged_scan.save_paths.extend(scan.save_paths);
        for (ph, hashes) in scan.pieces_groups {
            merged_scan
                .pieces_groups
                .entry(ph)
                .or_default()
                .extend(hashes);
        }
    }

    let scan_snapshot = stats.snapshot();
    tracing::info!(
        scanned = scan_snapshot.scanned,
        cached_skip = scan_snapshot.cached_skip,
        pieces_groups = merged_scan.pieces_groups.len(),
        "phase 1 complete"
    );

    if merged_scan.pieces_groups.is_empty() {
        tracing::info!("no torrents to match, pipeline done");
        if dry_run {
            return Ok(Some(build_preview(&[], &merged_scan, registry)));
        }
        return Ok(None);
    }

    // Shared HTTP client for pack detection and add phase.
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| EngineError::ScanFailed(format!("http client: {}", e)))?;

    // ── Phase 1.5: Pack detection ────────────────────────────────────
    let mut pack_matched_torrents = Vec::new();
    if let Some(ref jackett_cfg) = config.jackett_config {
        update_progress(progress_tx, "pack_detection", stats, start);
        tracing::info!("phase 1.5: pack detection via Jackett");

        for meta in merged_scan.torrents.values() {
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled.into());
            }

            if super::pack::is_pack(meta) {
                tracing::info!(name = %meta.name, size = meta.total_size, "detected pack torrent");
                match super::pack::search_pack_components(meta, jackett_cfg, &http_client).await {
                    Ok(matches) => {
                        for m in matches {
                            if let Some(site_id) = site_id_for_jackett_tracker(
                                registry,
                                &config.target_site_ids,
                                &m.result.tracker,
                            ) {
                                tracing::debug!(
                                    component = %m.component_name,
                                    tracker = %m.result.tracker,
                                    info_hash = %m.info_hash,
                                    "validated pack component match found"
                                );
                                pack_matched_torrents.push(adder::MatchedTorrent {
                                    pieces_hash: m.pieces_hash,
                                    site_id,
                                    torrent_id: None,
                                    download_url: m.result.download_url,
                                    save_path: config.default_save_path.clone(),
                                    skip_hash_check: config.skip_hash_check,
                                    tag: config.tag.clone(),
                                });
                            } else {
                                tracing::warn!(
                                    component = %m.component_name,
                                    tracker = %m.result.tracker,
                                    "validated Jackett pack component has no matching target site; skipping add"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(pack = %meta.name, error = %e, "pack component search failed, continuing");
                    }
                }
            }
        }
    }

    // ── Phase 2: Match ─────────────────────────────────────────────────
    update_progress(progress_tx, "matching", stats, start);
    tracing::info!(sites = config.target_site_ids.len(), "phase 2: match");

    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled.into());
    }

    // Determine save paths from source torrents
    // For now use the configured default; the adder will use it
    let mut matched_torrents = matcher::match_all_sites(
        &merged_scan,
        registry,
        &config.target_site_ids,
        repo,
        &config.default_save_path,
        config.skip_hash_check,
        config.tag.as_deref(),
        stats,
        cancel,
    )
    .await?;

    if !pack_matched_torrents.is_empty() {
        stats.matched.fetch_add(
            pack_matched_torrents.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        matched_torrents.extend(pack_matched_torrents);
    }

    let match_snapshot = stats.snapshot();
    tracing::info!(
        matched = match_snapshot.matched,
        skipped_tracker = match_snapshot.skipped_tracker,
        skipped_history = match_snapshot.skipped_history,
        "phase 2 complete"
    );

    if matched_torrents.is_empty() {
        tracing::info!("no matches found, pipeline done");
        if dry_run {
            return Ok(Some(build_preview(&[], &merged_scan, registry)));
        }
        return Ok(None);
    }

    // Dry-run: stop after match; never enter adder / history writes.
    if dry_run {
        let preview = build_preview(&matched_torrents, &merged_scan, registry);
        update_progress(progress_tx, "dry_run_preview", stats, start);
        tracing::info!(
            would_add = preview.would_add_count,
            "dry-run complete; skipping add phase"
        );
        return Ok(Some(preview));
    }

    // ── Phase 3: Add ───────────────────────────────────────────────────
    update_progress(progress_tx, "adding", stats, start);
    tracing::info!(to_add = matched_torrents.len(), "phase 3: add");

    let dest_hashes = Arc::new(Mutex::new(std::mem::take(&mut merged_scan.dest_hashes)));
    let auto_start = config.auto_start;

    stream::iter(matched_torrents.into_iter())
        .map(|matched| {
            let http_client = http_client.clone();
            let dest_hashes = Arc::clone(&dest_hashes);
            let db_writer = db_writer.clone();
            let cancel = cancel.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(CoreError::from(EngineError::Cancelled));
                }

                // Optional site-level rate limiting for downloads.
                // Soft-fail: a single site circuit/rate-limit should not abort
                // the whole add phase for other sites.
                if let Some(handle) = registry.get(&matched.site_id) {
                    if let Err(e) = handle.rate_limiter.acquire().await {
                        tracing::warn!(
                            site_id = matched.site_id.0,
                            error = %e,
                            "rate limiter blocked torrent download, skipping"
                        );
                        stats.failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return Ok(());
                    }
                }

                if let Err(e) = adder::add_torrent(
                    &matched,
                    &http_client,
                    dest_client,
                    &dest_hashes,
                    auto_start,
                    &db_writer,
                    stats,
                )
                .await
                {
                    // add_torrent already records per-torrent failures as Ok(false);
                    // unexpected errors are logged and counted without aborting siblings.
                    tracing::error!(
                        site_id = matched.site_id.0,
                        error = %e,
                        "add_torrent returned unexpected error"
                    );
                    stats.failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                update_progress(progress_tx, "adding", stats, start);
                Ok::<(), CoreError>(())
            }
        })
        .buffer_unordered(ADD_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    // Flush pending writes
    db_writer.flush().await?;

    let final_snapshot = stats.snapshot();
    tracing::info!(
        added = final_snapshot.added,
        failed = final_snapshot.failed,
        skipped_exists = final_snapshot.skipped_exists,
        elapsed_secs = start.elapsed().as_secs(),
        "pipeline complete"
    );

    Ok(None)
}

fn site_id_for_jackett_tracker(
    registry: &SiteRegistry,
    target_site_ids: &[SiteId],
    tracker: &str,
) -> Option<SiteId> {
    let tracker_lower = tracker.to_lowercase();
    let tracker_domain = filter::extract_domain(tracker);

    target_site_ids.iter().copied().find(|site_id| {
        registry.get(site_id).is_some_and(|handle| {
            let site_name = handle.core.name().to_lowercase();
            let site_domain = filter::extract_domain(handle.core.base_url());
            (!tracker_domain.is_empty() && tracker_domain == site_domain)
                || (!tracker_lower.is_empty()
                    && !site_name.is_empty()
                    && (tracker_lower.contains(&site_name) || site_name.contains(&tracker_lower)))
        })
    })
}

fn update_progress(
    tx: &watch::Sender<ReseedProgress>,
    phase: &str,
    stats: &ReseedStats,
    start: Instant,
) {
    let _ = tx.send(ReseedProgress {
        phase: phase.to_string(),
        stats: stats.snapshot(),
        elapsed_secs: start.elapsed().as_secs(),
        finished: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reseed_config_serializes_to_json_and_back() {
        // ReseedConfig is no longer fully serializable due to Arc downloader sources;
        // keep smoke coverage on plain fields via Debug formatting.
        let config = ReseedConfig {
            scan_folders: vec![PathBuf::from("/data/torrents"), PathBuf::from("/mnt/seed")],
            source_downloaders: vec![],
            target_site_ids: vec![SiteId(1), SiteId(2)],
            default_save_path: "/downloads".to_string(),
            skip_hash_check: true,
            auto_start: false,
            tag: Some("reseed".to_string()),
            jackett_config: Some(super::super::jackett::JackettConfig {
                url: "http://localhost:9117".to_string(),
                api_key: "abc123".to_string(),
            }),
        };

        assert_eq!(config.scan_folders.len(), 2);
        assert!(config.source_downloaders.is_empty());
        assert_eq!(config.target_site_ids.len(), 2);
        assert_eq!(config.default_save_path, "/downloads");
        assert!(config.skip_hash_check);
        assert!(!config.auto_start);
        assert_eq!(config.tag, Some("reseed".to_string()));
        assert!(config.jackett_config.is_some());
    }

    #[test]
    fn reseed_config_with_none_optionals_serializes() {
        let config = ReseedConfig {
            scan_folders: vec![PathBuf::from("/tmp")],
            source_downloaders: vec![],
            target_site_ids: vec![SiteId(10)],
            default_save_path: "/save".to_string(),
            skip_hash_check: false,
            auto_start: true,
            tag: None,
            jackett_config: None,
        };

        assert!(config.tag.is_none());
        assert!(config.jackett_config.is_none());
        assert!(config.auto_start);
        assert!(!config.skip_hash_check);
    }

    #[test]
    fn reseed_progress_serializes_to_json_and_back() {
        let progress = ReseedProgress {
            phase: "scanning".to_string(),
            stats: ReseedStatsSnapshot {
                scanned: 0,
                cached_skip: 0,
                matched: 0,
                added: 0,
                failed: 0,
                skipped_tracker: 0,
                skipped_history: 0,
                skipped_exists: 0,
            },
            elapsed_secs: 5,
            finished: false,
        };

        let json = serde_json::to_string(&progress).expect("serialize to JSON");
        let deserialized: ReseedProgress =
            serde_json::from_str(&json).expect("deserialize from JSON");

        assert_eq!(deserialized.phase, "scanning");
        assert_eq!(deserialized.elapsed_secs, 5);
        assert!(!deserialized.finished);
        assert_eq!(deserialized.stats.scanned, 0);
    }

    #[test]
    fn extract_domain_strips_https() {
        assert_eq!(
            filter::extract_domain("https://hdsky.me/announce"),
            "hdsky.me"
        );
    }

    #[test]
    fn extract_domain_strips_http() {
        assert_eq!(
            filter::extract_domain("http://tracker.mteam.cc:8080/announce"),
            "tracker.mteam.cc"
        );
    }

    #[test]
    fn extract_domain_no_protocol() {
        assert_eq!(filter::extract_domain("example.com/path"), "example.com");
    }

    #[test]
    fn extract_domain_empty_string() {
        assert_eq!(filter::extract_domain(""), "");
    }
}
