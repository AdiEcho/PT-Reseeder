// Tasks: DTOs and server functions for CRUD, manual triggers, run logs and
// dry-run previews.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: i64,
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub status: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: i64,
    pub site_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub source_downloader_ids: Vec<i64>,
    pub destination_downloader_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogInfo {
    pub id: i64,
    pub status: String,
    pub matched_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub duration_ms: Option<i64>,
    pub log_text: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunPreviewInfo {
    pub version: u32,
    pub would_add_count: usize,
    /// True when the run was a dry-run preview (add phase skipped).
    #[serde(default)]
    pub dry_run: bool,
    pub items: Vec<DryRunPreviewItemInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunPreviewItemInfo {
    pub site_id: i64,
    pub site_name: String,
    pub pieces_hash: String,
    pub torrent_id: Option<i64>,
    pub title: Option<String>,
    pub save_path: String,
    /// Content size in bytes when known.
    #[serde(default)]
    pub total_size: Option<i64>,
    /// Public torrent detail page URL (no credentials).
    #[serde(default)]
    pub detail_url: Option<String>,
}

/// One reseed task run shown on the reseeding results page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedRunInfo {
    pub log_id: i64,
    pub task_id: i64,
    pub task_name: String,
    pub status: String,
    pub matched_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub duration_ms: Option<i64>,
    pub dry_run: bool,
    pub item_count: usize,
    pub total_size: Option<i64>,
    pub created_at: String,
}

/// Full reseed-run detail (summary + matched items).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReseedRunDetail {
    pub run: ReseedRunInfo,
    pub items: Vec<DryRunPreviewItemInfo>,
}

#[server]
pub async fn get_tasks() -> Result<Vec<TaskInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let rows = repo
        .list_tasks()
        .await?;

    let mut tasks = Vec::with_capacity(rows.len());
    for t in rows {
        let site_ids = repo
            .get_task_sites(t.id)
            .await?;
        let folder_ids = repo
            .get_task_folders(t.id)
            .await?;
        let source_downloader_ids = repo
            .get_task_source_downloaders(t.id)
            .await?;
        tasks.push(TaskInfo {
            id: t.id,
            name: t.name,
            task_type: t.task_type,
            trigger_type: t.trigger_type,
            cron_expression: t.cron_expression,
            status: t.status,
            last_run_at: t.last_run_at,
            next_run_at: t.next_run_at,
            run_count: t.run_count.unwrap_or_default(),
            site_ids,
            folder_ids,
            source_downloader_ids,
            destination_downloader_id: t.destination_downloader_id,
        });
    }
    Ok(tasks)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskInput {
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub site_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    #[serde(default)]
    pub source_downloader_ids: Vec<i64>,
    pub destination_downloader_id: Option<i64>,
}

#[server]
pub async fn create_task(input: CreateTaskInput) -> Result<TaskInfo, ServerFnError> {
    use pt_reseeder_core::error::{CoreError, SchedulerError};
    use pt_reseeder_core::scheduler::task::{TaskCreateRequest, TaskManager};

    let context = server_context()?;
    let task_manager = TaskManager::new(pt_reseeder_core::db::repo::Repository::new(
        context.pool.clone(),
    ));

    let req = TaskCreateRequest {
        name: input.name,
        task_type: input.task_type,
        trigger_type: input.trigger_type,
        cron_expression: input.cron_expression,
        destination_downloader_id: input.destination_downloader_id,
        config_json: None,
        folder_ids: input.folder_ids,
        site_ids: input.site_ids,
        source_downloader_ids: input.source_downloader_ids,
    };

    let id = task_manager.create_task(&req).await.map_err(|e| match e {
        CoreError::Scheduler(SchedulerError::InvalidConfig(msg))
        | CoreError::Scheduler(SchedulerError::InvalidCron(msg)) => ServerFnError::new(msg),
        other => ServerFnError::new(format!("{other}")),
    })?;

    // Runtime configure is best-effort for manual tasks; failures should not leave
    // the UI thinking create failed after the row is already persisted.
    if let Err(error) = (context.reconfigure_task_runtime)(id).await {
        eprintln!("task {id} created but runtime configure failed: {error}");
    }

    // Prefer full readback; if that fails, still return the created associations so
    // the client can close the form and show the task.
    match get_tasks().await {
        Ok(tasks) => tasks
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ServerFnError::new("task created but not found")),
        Err(_) => Ok(TaskInfo {
            id,
            name: req.name,
            task_type: req.task_type,
            trigger_type: req.trigger_type,
            cron_expression: req.cron_expression,
            status: "idle".to_string(),
            last_run_at: None,
            next_run_at: None,
            run_count: 0,
            site_ids: req.site_ids,
            folder_ids: req.folder_ids,
            source_downloader_ids: req.source_downloader_ids,
            destination_downloader_id: req.destination_downloader_id,
        }),
    }
}

#[server]
pub async fn delete_task(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    // Mirror REST: unschedule cron/file-watch before deleting the DB row.
    if let Err(error) = (context.remove_task_runtime)(id).await {
        return Err(ServerFnError::new(format!(
            "failed to remove task runtime: {error}"
        )));
    }

    Repository::new(context.pool.clone())
        .delete_task(id)
        .await
        .map_err(Into::into)
}

#[server]
pub async fn trigger_task(id: i64, dry_run: bool) -> Result<(), ServerFnError> {
    let context = server_context()?;
    if dry_run {
        use pt_reseeder_core::db::repo::Repository;
        let task = Repository::new(context.pool.clone())
            .get_task(id)
            .await?
            .ok_or_else(|| ServerFnError::new(format!("task not found: {id}")))?;
        if task.task_type != "reseed" {
            return Err(ServerFnError::new(
                "dry-run is only supported for reseed tasks",
            ));
        }
    }
    (context.trigger_task_execution)(id, dry_run);
    Ok(())
}

#[server]
pub async fn get_task_logs(id: i64) -> Result<Vec<TaskLogInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .get_task_logs(id, 50)
        .await?;
    Ok(rows
        .into_iter()
        .map(|l| TaskLogInfo {
            id: l.id,
            status: l.status,
            matched_count: l.matched_count.unwrap_or_default(),
            succeeded_count: l.succeeded_count.unwrap_or_default(),
            failed_count: l.failed_count.unwrap_or_default(),
            duration_ms: l.duration_ms,
            log_text: l.log_text,
            created_at: l.created_at,
        })
        .collect())
}

#[server]
pub async fn get_latest_dry_run_preview(
    task_id: i64,
) -> Result<Option<DryRunPreviewInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::engine::DryRunPreview;

    let rows = Repository::new(server_pool()?)
        .get_task_logs(task_id, 20)
        .await?;

    for log in rows {
        if log.status != "dry_run" {
            continue;
        }
        let Some(text) = log.log_text.as_deref() else {
            return Ok(Some(DryRunPreviewInfo {
                version: 1,
                would_add_count: 0,
                dry_run: true,
                items: vec![],
            }));
        };
        let preview: DryRunPreview = serde_json::from_str(text)
            .map_err(|e| ServerFnError::new(format!("invalid dry-run preview: {e}")))?;
        return Ok(Some(preview_to_info(preview, true)));
    }

    Ok(None)
}

/// List recent reseed runs (dry-run + real) across all reseed tasks.
#[server]
pub async fn get_reseed_runs(limit: i64) -> Result<Vec<ReseedRunInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::engine::DryRunPreview;
    use std::collections::HashMap;

    let limit = limit.clamp(1, 200);
    let repo = Repository::new(server_pool()?);
    let logs = repo.list_recent_reseed_task_logs(limit).await?;

    // Cache task names to avoid N+1 lookups for the same task.
    let mut task_names: HashMap<i64, String> = HashMap::new();
    let mut out = Vec::with_capacity(logs.len());

    for log in logs {
        let task_name = if let Some(name) = task_names.get(&log.task_id) {
            name.clone()
        } else {
            let name = match repo.get_task(log.task_id).await? {
                Some(task) => task.name,
                None => format!("任务 #{}", log.task_id),
            };
            task_names.insert(log.task_id, name.clone());
            name
        };

        let parsed = log
            .log_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<DryRunPreview>(text).ok());

        let dry_run = log.status == "dry_run"
            || parsed
                .as_ref()
                .map(|p| p.dry_run)
                .unwrap_or(false);

        let (item_count, total_size) = match &parsed {
            Some(p) => {
                let sum = p
                    .items
                    .iter()
                    .filter_map(|item| item.total_size)
                    .fold(0i64, |acc, n| acc.saturating_add(n));
                let total = if sum > 0 { Some(sum) } else { None };
                (p.items.len(), total)
            }
            None => (0, None),
        };

        out.push(ReseedRunInfo {
            log_id: log.id,
            task_id: log.task_id,
            task_name,
            status: log.status,
            matched_count: log.matched_count.unwrap_or_default(),
            succeeded_count: log.succeeded_count.unwrap_or_default(),
            failed_count: log.failed_count.unwrap_or_default(),
            duration_ms: log.duration_ms,
            dry_run,
            item_count,
            total_size,
            created_at: log.created_at,
        });
    }

    Ok(out)
}

/// Fetch one reseed run with its matched items.
#[server]
pub async fn get_reseed_run_detail(log_id: i64) -> Result<Option<ReseedRunDetail>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::engine::DryRunPreview;

    let repo = Repository::new(server_pool()?);
    let Some(log) = repo.get_task_log(log_id).await? else {
        return Ok(None);
    };

    let task = repo.get_task(log.task_id).await?;
    if task
        .as_ref()
        .is_some_and(|t| t.task_type != "reseed")
    {
        return Ok(None);
    }
    let task_name = task
        .map(|t| t.name)
        .unwrap_or_else(|| format!("任务 #{}", log.task_id));

    let parsed = log
        .log_text
        .as_deref()
        .and_then(|text| serde_json::from_str::<DryRunPreview>(text).ok());

    let dry_run = log.status == "dry_run"
        || parsed
            .as_ref()
            .map(|p| p.dry_run)
            .unwrap_or(false);

    let items = parsed
        .as_ref()
        .map(|p| {
            p.items
                .iter()
                .map(|item| DryRunPreviewItemInfo {
                    site_id: item.site_id,
                    site_name: item.site_name.clone(),
                    pieces_hash: item.pieces_hash.clone(),
                    torrent_id: item.torrent_id,
                    title: item.title.clone(),
                    save_path: item.save_path.clone(),
                    total_size: item.total_size,
                    detail_url: item.detail_url.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let total_size = {
        let sum = items
            .iter()
            .filter_map(|item| item.total_size)
            .fold(0i64, |acc, n| acc.saturating_add(n));
        if sum > 0 {
            Some(sum)
        } else {
            None
        }
    };

    Ok(Some(ReseedRunDetail {
        run: ReseedRunInfo {
            log_id: log.id,
            task_id: log.task_id,
            task_name,
            status: log.status,
            matched_count: log.matched_count.unwrap_or_default(),
            succeeded_count: log.succeeded_count.unwrap_or_default(),
            failed_count: log.failed_count.unwrap_or_default(),
            duration_ms: log.duration_ms,
            dry_run,
            item_count: items.len(),
            total_size,
            created_at: log.created_at,
        },
        items,
    }))
}

// Only compiled for the SSR binary: the hydrate/WASM client does not link
// pt-reseeder-core, and this helper is only called from #[server] bodies.
#[cfg(feature = "ssr")]
fn preview_to_info(
    preview: pt_reseeder_core::engine::DryRunPreview,
    force_dry_run: bool,
) -> DryRunPreviewInfo {
    DryRunPreviewInfo {
        version: preview.version,
        would_add_count: preview.would_add_count,
        dry_run: force_dry_run || preview.dry_run,
        items: preview
            .items
            .into_iter()
            .map(|item| DryRunPreviewItemInfo {
                site_id: item.site_id,
                site_name: item.site_name,
                pieces_hash: item.pieces_hash,
                torrent_id: item.torrent_id,
                title: item.title,
                save_path: item.save_path,
                total_size: item.total_size,
                detail_url: item.detail_url,
            })
            .collect(),
    }
}
