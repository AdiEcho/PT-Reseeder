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
    /// Tag to apply to added torrents.
    pub tag: Option<String>,
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
    ) -> Result<(watch::Receiver<ReseedProgress>, tokio::task::JoinHandle<Result<ReseedStatsSnapshot, CoreError>>), CoreError> {
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

        let scan = scanner::scan_folder(
            folder,
            repo,
            db_writer,
            dest_client,
            stats,
            cancel,
        )
        .await?;

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

    // ── Phase 2: Match ─────────────────────────────────────────────────
    update_progress(progress_tx, "matching", stats, start);
    tracing::info!(
        sites = config.target_site_ids.len(),
        "phase 2: match"
    );

    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled.into());
    }

    // Determine save paths from source torrents
    // For now use the configured default; the adder will use it
    let matched_torrents = matcher::match_all_sites(
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
    tracing::info!(
        to_add = matched_torrents.len(),
        "phase 3: add"
    );

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
            &merged_scan.dest_hashes,
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
