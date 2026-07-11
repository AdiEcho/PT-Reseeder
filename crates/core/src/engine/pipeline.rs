use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing;

use crate::db::repo::Repository;
use crate::db::writer::DbWriterHandle;
use crate::downloader::traits::Downloader;
use crate::error::{CoreError, EngineError};
use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

use super::adder;
use super::matcher;
use super::scanner;
use super::stats::{ReseedStats, ReseedStatsSnapshot};

/// Configuration for a reseed pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedConfig {
    /// Folders to scan for .torrent files.
    pub scan_folders: Vec<PathBuf>,
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
    pub async fn run(
        &self,
        config: ReseedConfig,
        dest_client: Arc<dyn Downloader>,
    ) -> Result<
        (
            watch::Receiver<ReseedProgress>,
            tokio::task::JoinHandle<Result<ReseedStatsSnapshot, CoreError>>,
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

            result.map(|_| snapshot)
        });

        Ok((progress_rx, handle))
    }

    /// Run the pipeline synchronously (blocking until complete).
    pub async fn run_sync(
        &self,
        config: ReseedConfig,
        dest_client: Arc<dyn Downloader>,
    ) -> Result<ReseedStatsSnapshot, CoreError> {
        let stats = Arc::new(ReseedStats::new());
        let start = Instant::now();
        let (progress_tx, _) = watch::channel(ReseedProgress {
            phase: "initializing".to_string(),
            stats: stats.snapshot(),
            elapsed_secs: 0,
            finished: false,
        });

        run_pipeline(
            config,
            &self.registry,
            &self.repo,
            &self.db_writer,
            dest_client.as_ref(),
            &stats,
            &self.cancel,
            &progress_tx,
            start,
        )
        .await?;

        Ok(stats.snapshot())
    }
}

/// Internal: run the three-phase pipeline.
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
) -> Result<(), CoreError> {
    // ── Phase 1: Scan ──────────────────────────────────────────────────
    update_progress(progress_tx, "scanning", stats, start);
    tracing::info!(folders = config.scan_folders.len(), "phase 1: scan");

    if config.scan_folders.is_empty() {
        return Err(EngineError::ScanFailed("no scan folders configured".into()).into());
    }

    // Scan all folders and merge results
    let mut merged_scan = scanner::ScanResult {
        torrents: std::collections::HashMap::new(),
        pieces_groups: std::collections::HashMap::new(),
        dest_hashes: std::collections::HashSet::new(),
    };

    for folder in &config.scan_folders {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled.into());
        }

        let scan =
            scanner::scan_folder(folder, repo, db_writer, dest_client, stats, cancel).await?;

        // Merge
        merged_scan.torrents.extend(scan.torrents);
        for (ph, hashes) in scan.pieces_groups {
            merged_scan
                .pieces_groups
                .entry(ph)
                .or_default()
                .extend(hashes);
        }
        merged_scan.dest_hashes.extend(scan.dest_hashes);
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
        return Ok(());
    }

    // ── Phase 1.5: Pack detection ────────────────────────────────────
    let mut pack_matched_torrents = Vec::new();
    if let Some(ref jackett_cfg) = config.jackett_config {
        update_progress(progress_tx, "pack_detection", stats, start);
        tracing::info!("phase 1.5: pack detection via Jackett");

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| EngineError::ScanFailed(format!("http client: {}", e)))?;

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
        return Ok(());
    }

    // ── Phase 3: Add ───────────────────────────────────────────────────
    update_progress(progress_tx, "adding", stats, start);
    tracing::info!(to_add = matched_torrents.len(), "phase 3: add");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| EngineError::AddFailed(format!("http client: {}", e)))?;

    for matched in &matched_torrents {
        if cancel.is_cancelled() {
            // Graceful: in-flight batch done, partial results already in DB
            tracing::info!("cancelled during add phase, partial results saved");
            return Err(EngineError::Cancelled.into());
        }

        adder::add_torrent(
            matched,
            &http_client,
            dest_client,
            &mut merged_scan.dest_hashes,
            config.auto_start,
            db_writer,
            stats,
        )
        .await?;

        update_progress(progress_tx, "adding", stats, start);
    }

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

    Ok(())
}

fn site_id_for_jackett_tracker(
    registry: &SiteRegistry,
    target_site_ids: &[SiteId],
    tracker: &str,
) -> Option<SiteId> {
    let tracker_lower = tracker.to_lowercase();
    let tracker_domain = extract_domain(tracker);

    target_site_ids.iter().copied().find(|site_id| {
        registry.get(site_id).is_some_and(|handle| {
            let site_name = handle.core.name().to_lowercase();
            let site_domain = extract_domain(handle.core.base_url());
            (!tracker_domain.is_empty() && tracker_domain == site_domain)
                || (!tracker_lower.is_empty()
                    && !site_name.is_empty()
                    && (tracker_lower.contains(&site_name) || site_name.contains(&tracker_lower)))
        })
    })
}

fn extract_domain(url: &str) -> String {
    let without_proto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    without_proto
        .split(|c| c == '/' || c == ':')
        .next()
        .unwrap_or("")
        .to_lowercase()
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
        let config = ReseedConfig {
            scan_folders: vec![PathBuf::from("/data/torrents"), PathBuf::from("/mnt/seed")],
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

        let json = serde_json::to_string(&config).expect("serialize to JSON");
        let deserialized: ReseedConfig =
            serde_json::from_str(&json).expect("deserialize from JSON");

        assert_eq!(deserialized.scan_folders.len(), 2);
        assert_eq!(deserialized.scan_folders[0], PathBuf::from("/data/torrents"));
        assert_eq!(deserialized.target_site_ids.len(), 2);
        assert_eq!(deserialized.target_site_ids[0], SiteId(1));
        assert_eq!(deserialized.default_save_path, "/downloads");
        assert!(deserialized.skip_hash_check);
        assert!(!deserialized.auto_start);
        assert_eq!(deserialized.tag, Some("reseed".to_string()));
        assert!(deserialized.jackett_config.is_some());
    }

    #[test]
    fn reseed_config_with_none_optionals_serializes() {
        let config = ReseedConfig {
            scan_folders: vec![PathBuf::from("/tmp")],
            target_site_ids: vec![SiteId(10)],
            default_save_path: "/save".to_string(),
            skip_hash_check: false,
            auto_start: true,
            tag: None,
            jackett_config: None,
        };

        let json = serde_json::to_string(&config).expect("serialize to JSON");
        let deserialized: ReseedConfig =
            serde_json::from_str(&json).expect("deserialize from JSON");

        assert!(deserialized.tag.is_none());
        assert!(deserialized.jackett_config.is_none());
        assert!(deserialized.auto_start);
        assert!(!deserialized.skip_hash_check);
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
        assert_eq!(extract_domain("https://hdsky.me/announce"), "hdsky.me");
    }

    #[test]
    fn extract_domain_strips_http() {
        assert_eq!(
            extract_domain("http://tracker.mteam.cc:8080/announce"),
            "tracker.mteam.cc"
        );
    }

    #[test]
    fn extract_domain_no_protocol() {
        assert_eq!(extract_domain("example.com/path"), "example.com");
    }

    #[test]
    fn extract_domain_empty_string() {
        assert_eq!(extract_domain(""), "");
    }
}
