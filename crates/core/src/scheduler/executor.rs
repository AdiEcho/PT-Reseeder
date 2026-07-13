use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::crypto::Vault;
use crate::db::models::{DownloaderRow, TaskRow};
use crate::db::repo::Repository;
use crate::db::writer::{DbWriterHandle, WriteOp};
use crate::downloader::qbittorrent::QBittorrentClient;
use crate::downloader::traits::Downloader;
use crate::downloader::transmission::TransmissionClient;
use crate::engine::{ReseedConfig, ReseedEngine};
use crate::error::{CoreError, SchedulerError};
use crate::repost::submitter::{submit_batch, SubmitBatchCriteria};
use crate::scheduler::task::next_run_at_for;
use crate::site::models::SiteId;
use crate::site::registry::SiteRegistry;

#[derive(Debug, Default, Deserialize)]
struct ReseedTaskConfig {
    default_save_path: Option<String>,
    skip_hash_check: Option<bool>,
    tag: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RepostTaskConfig {
    entry_ids: Option<Vec<i64>>,
    limit: Option<usize>,
}

/// Executes tasks by resolving their configuration and running the appropriate pipeline.
pub struct TaskExecutor {
    repo: Repository,
    db_writer: DbWriterHandle,
    site_registry: Arc<SiteRegistry>,
    cancel_token: CancellationToken,
    vault: Option<Vault>,
}

impl TaskExecutor {
    pub fn new(
        repo: Repository,
        db_writer: DbWriterHandle,
        site_registry: Arc<SiteRegistry>,
        cancel_token: CancellationToken,
        vault: Option<Vault>,
    ) -> Self {
        Self {
            repo,
            db_writer,
            site_registry,
            cancel_token,
            vault,
        }
    }

    /// Execute a task by ID. Concurrent triggers are skipped and logged.
    pub async fn execute(&self, task_id: i64) -> Result<(), CoreError> {
        let task = self
            .repo
            .get_task(task_id)
            .await?
            .ok_or(CoreError::Scheduler(SchedulerError::TaskNotFound(task_id)))?;

        if !self.repo.try_mark_task_running(task_id).await? {
            warn!(task_id, "task already running, skipping trigger");
            self.write_log(
                task_id,
                "skipped",
                0,
                0,
                0,
                None,
                Some("skipped: task already running"),
            )
            .await?;
            if task.trigger_type == "cron" {
                let next_run_at = next_run_at_for(task.cron_expression.as_deref())?;
                self.repo
                    .update_task_next_run_at(task_id, next_run_at.as_deref())
                    .await?;
            }
            return Ok(());
        }

        let start = Instant::now();
        let result = match task.task_type.as_str() {
            "reseed" => self.execute_reseed(&task).await,
            "repost" => self.execute_repost(&task).await,
            "sync_stats" => self.execute_sync_stats(&task).await,
            other => {
                let msg = format!("unknown task type: {}", other);
                Err(CoreError::Scheduler(SchedulerError::ExecutorError(msg)))
            }
        };

        let duration_ms = start.elapsed().as_millis() as i64;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let next_run_at = if task.trigger_type == "cron" {
            next_run_at_for(task.cron_expression.as_deref())?
        } else {
            None
        };

        match &result {
            Ok((matched, succeeded, failed)) => {
                let status = if *failed > 0 { "partial" } else { "success" };
                self.write_log(
                    task_id,
                    status,
                    *matched,
                    *succeeded,
                    *failed,
                    Some(duration_ms),
                    None,
                )
                .await?;
                self.repo.update_task_status(task_id, "idle").await?;
                self.repo
                    .update_task_run_times(task_id, &now, next_run_at.as_deref())
                    .await?;
                info!(
                    task_id,
                    matched, succeeded, failed, duration_ms, "task completed"
                );
            }
            Err(e) => {
                let log_text = format!("error: {}", e);
                self.write_log(
                    task_id,
                    "failed",
                    0,
                    0,
                    1,
                    Some(duration_ms),
                    Some(&log_text),
                )
                .await?;
                self.repo.update_task_status(task_id, "error").await?;
                self.repo
                    .update_task_run_times(task_id, &now, next_run_at.as_deref())
                    .await?;
                error!(task_id, %e, "task failed");
            }
        }

        result.map(|_| ())
    }

    /// Execute a reseed-type task.
    /// Returns (matched_count, succeeded_count, failed_count).
    async fn execute_reseed(&self, task: &TaskRow) -> Result<(i64, i64, i64), CoreError> {
        let folder_ids = self.repo.get_task_folders(task.id).await?;
        let site_ids = self.repo.get_task_sites(task.id).await?;

        let mut scan_folders = Vec::new();
        for fid in &folder_ids {
            if let Some(folder) = self.repo.get_folder(*fid).await? {
                if folder.enabled {
                    match folder.scan_mode.as_str() {
                        "local" => {
                            scan_folders.push(PathBuf::from(&folder.path));
                            self.repo.update_folder_scanned(*fid).await?;
                        }
                        "downloader" => {
                            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                                format!(
                                    "folder {} uses downloader scan mode, which is unsupported until downloader APIs expose .torrent data/pieces_hash",
                                    folder.id
                                ),
                            )));
                        }
                        other => {
                            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                                format!("folder {} has invalid scan_mode: {}", folder.id, other),
                            )));
                        }
                    }
                }
            }
        }

        if scan_folders.is_empty() {
            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                "no enabled folders configured for task".to_string(),
            )));
        }

        if site_ids.is_empty() {
            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                "no target sites configured for task".to_string(),
            )));
        }

        let target_site_ids: Vec<SiteId> = site_ids.iter().copied().map(SiteId::from).collect();
        let valid_sites = target_site_ids
            .iter()
            .filter(|site_id| {
                self.site_registry
                    .get(site_id)
                    .and_then(|handle| handle.reseed.as_ref())
                    .is_some()
            })
            .count();
        if valid_sites == 0 {
            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                "no target sites registered with reseed capability".to_string(),
            )));
        }

        let (dest_downloader, auto_start) = self.build_destination_downloader(task).await?;
        let task_config = parse_reseed_config(task.config_json.as_deref())?;
        let default_save_path = task_config
            .default_save_path
            .unwrap_or_else(|| "/downloads".to_string());

        let config = ReseedConfig {
            scan_folders,
            target_site_ids,
            default_save_path,
            skip_hash_check: task_config.skip_hash_check.unwrap_or(false),
            auto_start,
            tag: task_config.tag,
            jackett_config: None,
        };

        let engine = ReseedEngine::new(
            Arc::clone(&self.site_registry),
            self.repo.clone(),
            self.db_writer.clone(),
            self.cancel_token.clone(),
        );
        let stats = engine.run_sync(config, dest_downloader).await?;

        Ok((
            stats.matched as i64,
            stats.added as i64,
            stats.failed as i64,
        ))
    }

    /// Execute a repost-type task.
    /// Returns (matched_count, succeeded_count, failed_count).
    async fn execute_repost(&self, task: &TaskRow) -> Result<(i64, i64, i64), CoreError> {
        let site_ids = self.repo.get_task_sites(task.id).await?;

        if site_ids.is_empty() {
            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                "no target sites configured for repost task".to_string(),
            )));
        }

        let config = parse_repost_config(task.config_json.as_deref())?;
        let batch_result = submit_batch(
            &self.repo,
            &self.site_registry,
            SubmitBatchCriteria {
                entry_ids: config.entry_ids,
                target_site_ids: Some(site_ids),
                limit: config.limit,
            },
        )
        .await?;

        info!(
            task_id = task.id,
            candidates = batch_result.candidate_count,
            submitted = batch_result.submitted_count,
            failed = batch_result.failed_count,
            skipped = batch_result.skipped_count,
            "repost task submitted approved queue entries"
        );

        Ok((
            batch_result.candidate_count,
            batch_result.submitted_count,
            batch_result.failed_count + batch_result.skipped_count,
        ))
    }

    /// Execute a sync_stats-type task: fetch user stats from all enabled sites
    /// (or only the sites configured on this task) and store snapshots.
    /// Returns (total_sites, succeeded_count, failed_count).
    async fn execute_sync_stats(&self, task: &TaskRow) -> Result<(i64, i64, i64), CoreError> {
        use crate::db::models::UserStatRecord;
        use crate::site::models::SiteId;

        // If the task has specific sites configured, only sync those;
        // otherwise sync all sites in the registry.
        let task_site_ids = self.repo.get_task_sites(task.id).await?;
        let site_ids: Vec<SiteId> = if task_site_ids.is_empty() {
            self.site_registry.list_ids()
        } else {
            task_site_ids.into_iter().map(SiteId::from).collect()
        };

        let total = site_ids.len() as i64;
        let mut succeeded: i64 = 0;
        let mut failed: i64 = 0;

        for site_id in &site_ids {
            if self.cancel_token.is_cancelled() {
                warn!("sync_stats cancelled");
                break;
            }

            let handle = match self.site_registry.get(site_id) {
                Some(h) => h,
                None => {
                    warn!(site_id = site_id.0, "site not in registry, skipping");
                    failed += 1;
                    continue;
                }
            };

            let user_info_cap = match handle.user_info.as_ref() {
                Some(ui) => ui,
                None => {
                    info!(site_id = site_id.0, "site has no user_info capability, skipping");
                    continue;
                }
            };

            match user_info_cap.fetch_user_info().await {
                Ok(stats) => {
                    let sid = site_id.0;
                    let record = UserStatRecord {
                        id: 0,
                        site_id: sid,
                        uploaded: stats.uploaded,
                        downloaded: stats.downloaded,
                        ratio: stats.ratio,
                        bonus: stats.bonus,
                        user_class: stats.user_class,
                        seeding_count: stats.seeding_count,
                        leeching_count: stats.leeching_count,
                        seeding_size: stats.seeding_size,
                        upload_time_seconds: stats.upload_time_seconds,
                        fetched_at: String::new(),
                    };
                    if let Err(e) = self.repo.insert_user_stats(sid, &record).await {
                        error!(site_id = sid, %e, "failed to store user stats");
                        failed += 1;
                    } else {
                        succeeded += 1;
                    }
                }
                Err(e) => {
                    error!(site_id = site_id.0, %e, "failed to fetch user stats");
                    failed += 1;
                }
            }
        }

        info!(
            task_id = task.id,
            total,
            succeeded,
            failed,
            "sync_stats task completed"
        );

        Ok((total, succeeded, failed))
    }

    async fn build_destination_downloader(
        &self,
        task: &TaskRow,
    ) -> Result<(Arc<dyn Downloader>, bool), CoreError> {
        let pair_id = task.downloader_pair_id.ok_or_else(|| {
            CoreError::Scheduler(SchedulerError::ExecutorError(
                "downloader_pair_id is required for reseed tasks".to_string(),
            ))
        })?;
        let pair = self
            .repo
            .get_downloader_pair(pair_id)
            .await?
            .ok_or_else(|| {
                CoreError::Scheduler(SchedulerError::ExecutorError(format!(
                    "downloader pair {} not found",
                    pair_id
                )))
            })?;
        let row = self
            .repo
            .get_downloader(pair.destination_id)
            .await?
            .ok_or_else(|| {
                CoreError::Scheduler(SchedulerError::ExecutorError(format!(
                    "destination downloader {} not found",
                    pair.destination_id
                )))
            })?;

        if !row.enabled {
            return Err(CoreError::Scheduler(SchedulerError::ExecutorError(
                format!("destination downloader {} is disabled", row.id),
            )));
        }

        let auto_start = row.auto_start.unwrap_or(true);
        let downloader = build_downloader(&row, self.vault.as_ref()).await?;
        Ok((downloader, auto_start))
    }

    /// Write a task log entry via the DbWriter channel.
    async fn write_log(
        &self,
        task_id: i64,
        status: &str,
        matched: i64,
        succeeded: i64,
        failed: i64,
        duration_ms: Option<i64>,
        log_text: Option<&str>,
    ) -> Result<(), CoreError> {
        self.db_writer
            .send(WriteOp::InsertTaskLog {
                task_id,
                status: status.to_string(),
                matched_count: matched,
                succeeded_count: succeeded,
                failed_count: failed,
                duration_ms,
                log_text: log_text.map(|s| s.to_string()),
            })
            .await
    }
}

async fn build_downloader(
    row: &DownloaderRow,
    vault: Option<&Vault>,
) -> Result<Arc<dyn Downloader>, CoreError> {
    let username = decrypt_optional(
        vault,
        &row.encrypted_username,
        &row.username_nonce,
        "username",
    )?;
    let password = decrypt_optional(
        vault,
        &row.encrypted_password,
        &row.password_nonce,
        "password",
    )?;

    match row.dl_type.as_str() {
        "qbittorrent" => {
            let mut client = QBittorrentClient::new(
                &row.host,
                row.port as u16,
                username.as_deref().unwrap_or(""),
                password.as_deref().unwrap_or(""),
            );
            client.connect().await?;
            Ok(Arc::new(client))
        }
        "transmission" => {
            let mut client = TransmissionClient::new(
                &row.host,
                row.port as u16,
                username.as_deref(),
                password.as_deref(),
            );
            client.connect().await?;
            Ok(Arc::new(client))
        }
        other => Err(CoreError::Scheduler(SchedulerError::ExecutorError(
            format!("unsupported downloader type: {}", other),
        ))),
    }
}

fn decrypt_optional(
    vault: Option<&Vault>,
    encrypted: &Option<Vec<u8>>,
    nonce: &Option<Vec<u8>>,
    label: &str,
) -> Result<Option<String>, CoreError> {
    let (Some(encrypted), Some(nonce)) = (encrypted, nonce) else {
        return Ok(None);
    };
    let vault = vault.ok_or_else(|| {
        CoreError::Scheduler(SchedulerError::ExecutorError(format!(
            "vault is locked; cannot decrypt downloader {}",
            label
        )))
    })?;
    let nonce_arr: [u8; 12] = nonce.as_slice().try_into().map_err(|_| {
        CoreError::Scheduler(SchedulerError::ExecutorError(format!(
            "invalid downloader {} nonce length",
            label
        )))
    })?;
    let plaintext = vault.decrypt(encrypted, &nonce_arr)?;
    String::from_utf8(plaintext).map(Some).map_err(|e| {
        CoreError::Scheduler(SchedulerError::ExecutorError(format!(
            "downloader {} is not valid UTF-8: {}",
            label, e
        )))
    })
}

fn parse_reseed_config(config_json: Option<&str>) -> Result<ReseedTaskConfig, CoreError> {
    parse_task_config(config_json)
}

fn parse_repost_config(config_json: Option<&str>) -> Result<RepostTaskConfig, CoreError> {
    parse_task_config(config_json)
}

fn parse_task_config<T>(config_json: Option<&str>) -> Result<T, CoreError>
where
    T: Default + for<'de> Deserialize<'de>,
{
    match config_json.filter(|json| !json.trim().is_empty()) {
        Some(json) => serde_json::from_str(json).map_err(|e| {
            CoreError::Scheduler(SchedulerError::ExecutorError(format!(
                "invalid task config_json: {}",
                e
            )))
        }),
        None => Ok(T::default()),
    }
}
