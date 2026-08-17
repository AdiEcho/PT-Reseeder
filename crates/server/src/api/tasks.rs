use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::engine::DryRunPreview;
use pt_reseeder_core::error::{CoreError, SchedulerError};
use pt_reseeder_core::scheduler::task::{TaskCreateRequest, TaskManager};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
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

#[derive(Deserialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct DryRunPreviewInfo {
    pub version: u32,
    pub would_add_count: usize,
    pub dry_run: bool,
    pub items: Vec<DryRunPreviewItemInfo>,
}

#[derive(Serialize)]
pub struct DryRunPreviewItemInfo {
    pub site_id: i64,
    pub site_name: String,
    pub pieces_hash: String,
    pub torrent_id: Option<i64>,
    pub title: Option<String>,
    pub save_path: String,
    pub total_size: Option<i64>,
    pub detail_url: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Serialize)]
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
    pub history_skipped_count: usize,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ReseedRunDetail {
    pub run: ReseedRunInfo,
    pub items: Vec<DryRunPreviewItemInfo>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TriggerParams {
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize)]
pub struct ReseedRunsParams {
    pub limit: Option<i64>,
    pub task_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

fn map_task_manager_error(e: CoreError) -> (StatusCode, Json<ApiError>) {
    match e {
        CoreError::Scheduler(SchedulerError::InvalidConfig(msg))
        | CoreError::Scheduler(SchedulerError::InvalidCron(msg)) => {
            api_err(StatusCode::BAD_REQUEST, msg)
        }
        CoreError::Scheduler(SchedulerError::TaskRunning(_)) => api_err(
            StatusCode::CONFLICT,
            "任务正在运行，请等待结束后再编辑。",
        ),
        other => api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{other}")),
    }
}

fn input_to_request(input: CreateTaskInput) -> TaskCreateRequest {
    TaskCreateRequest {
        name: input.name,
        task_type: input.task_type,
        trigger_type: input.trigger_type,
        cron_expression: input.cron_expression,
        destination_downloader_id: input.destination_downloader_id,
        config_json: None,
        folder_ids: input.folder_ids,
        site_ids: input.site_ids,
        source_downloader_ids: input.source_downloader_ids,
    }
}

async fn read_task_info(repo: &Repository, id: i64) -> Result<TaskInfo, (StatusCode, Json<ApiError>)> {
    let task = repo
        .get_task(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("task not found: {id}")))?;

    let site_ids = repo
        .get_task_sites(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    let folder_ids = repo
        .get_task_folders(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    let source_downloader_ids = repo
        .get_task_source_downloaders(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    Ok(TaskInfo {
        id: task.id,
        name: task.name,
        task_type: task.task_type,
        trigger_type: task.trigger_type,
        cron_expression: task.cron_expression,
        status: task.status,
        last_run_at: task.last_run_at,
        next_run_at: task.next_run_at,
        run_count: task.run_count.unwrap_or_default(),
        site_ids,
        folder_ids,
        source_downloader_ids,
        destination_downloader_id: task.destination_downloader_id,
    })
}

fn preview_to_info(preview: DryRunPreview, force_dry_run: bool) -> DryRunPreviewInfo {
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
                outcome: item.outcome,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /tasks
async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskInfo>>, (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;
    let rows = repo
        .list_tasks()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    let mut tasks = Vec::with_capacity(rows.len());
    for t in rows {
        let site_ids = repo
            .get_task_sites(t.id)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
        let folder_ids = repo
            .get_task_folders(t.id)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
        let source_downloader_ids = repo
            .get_task_source_downloaders(t.id)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
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
    Ok(Json(tasks))
}

/// POST /tasks
async fn create_task(
    State(state): State<AppState>,
    Json(input): Json<CreateTaskInput>,
) -> Result<Json<TaskInfo>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let req = input_to_request(input);

    let id = task_manager
        .create_task(&req)
        .await
        .map_err(map_task_manager_error)?;

    // Reconfigure runtime (best-effort).
    if let Err(e) = state.reconfigure_task_runtime(id).await {
        tracing::error!(task_id = id, %e, "task created but runtime configure failed");
    }

    let info = read_task_info(&state.inner.repo, id).await?;
    Ok(Json(info))
}

/// PUT /tasks/:id
async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<CreateTaskInput>,
) -> Result<Json<TaskInfo>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let req = input_to_request(input);

    task_manager
        .update_task(id, &req)
        .await
        .map_err(map_task_manager_error)?;

    if let Err(e) = state.reconfigure_task_runtime(id).await {
        tracing::error!(task_id = id, %e, "task updated but runtime configure failed");
    }

    let info = read_task_info(&state.inner.repo, id).await?;
    Ok(Json(info))
}

/// DELETE /tasks/:id
async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Remove runtime BEFORE deleting the DB row.
    state
        .remove_task_runtime(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    state
        .inner
        .repo
        .delete_task(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /tasks/:id/trigger
async fn trigger_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<TriggerParams>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if params.dry_run {
        let task = state
            .inner
            .repo
            .get_task(id)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("task not found: {id}")))?;
        if task.task_type != "reseed" {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "dry-run is only supported for reseed tasks",
            ));
        }
    }

    let executor = state.task_executor().await;
    let dry_run = params.dry_run;
    tokio::spawn(async move {
        if let Err(e) = executor.execute(id, dry_run).await {
            tracing::error!(task_id = id, dry_run, %e, "task execution failed");
        }
    });

    Ok(StatusCode::ACCEPTED)
}

/// GET /tasks/:id/logs
async fn get_task_logs(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<TaskLogInfo>>, (StatusCode, Json<ApiError>)> {
    let rows = state
        .inner
        .repo
        .get_task_logs(id, 50)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    let logs = rows
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
        .collect();

    Ok(Json(logs))
}

/// GET /tasks/:id/dry-run-preview
async fn get_latest_dry_run_preview(
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<Json<Option<DryRunPreviewInfo>>, (StatusCode, Json<ApiError>)> {
    let rows = state
        .inner
        .repo
        .get_task_logs(task_id, 20)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    for log in rows {
        if log.status != "dry_run" {
            continue;
        }
        let Some(text) = log.log_text.as_deref() else {
            return Ok(Json(Some(DryRunPreviewInfo {
                version: 1,
                would_add_count: 0,
                dry_run: true,
                items: vec![],
            })));
        };
        let preview: DryRunPreview = serde_json::from_str(text).map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("invalid dry-run preview: {e}"),
            )
        })?;
        return Ok(Json(Some(preview_to_info(preview, true))));
    }

    Ok(Json(None))
}

/// GET /reseed-runs
async fn get_reseed_runs(
    State(state): State<AppState>,
    Query(params): Query<ReseedRunsParams>,
) -> Result<Json<Vec<ReseedRunInfo>>, (StatusCode, Json<ApiError>)> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let repo = &state.inner.repo;
    let logs = repo
        .list_recent_reseed_task_logs(limit, params.task_id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    let mut task_names: HashMap<i64, String> = HashMap::new();
    let mut out = Vec::with_capacity(logs.len());

    for log in logs {
        let task_name = if let Some(name) = task_names.get(&log.task_id) {
            name.clone()
        } else {
            let name = match repo.get_task(log.task_id).await {
                Ok(Some(task)) => task.name,
                _ => format!("任务 #{}", log.task_id),
            };
            task_names.insert(log.task_id, name.clone());
            name
        };

        let parsed = log
            .log_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<DryRunPreview>(text).ok());

        let dry_run =
            log.status == "dry_run" || parsed.as_ref().map(|p| p.dry_run).unwrap_or(false);

        let (item_count, total_size, history_skipped_count) = match &parsed {
            Some(p) => {
                let sum = p
                    .items
                    .iter()
                    .filter(|item| item.outcome.as_deref() != Some("skipped"))
                    .filter_map(|item| item.total_size)
                    .fold(0i64, |acc, n| acc.saturating_add(n));
                let total = if sum > 0 { Some(sum) } else { None };
                (p.items.len(), total, p.history_skipped_count)
            }
            None => (0, None, 0),
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
            history_skipped_count,
            created_at: log.created_at,
        });
    }

    Ok(Json(out))
}

/// GET /reseed-runs/:id
async fn get_reseed_run_detail(
    State(state): State<AppState>,
    Path(log_id): Path<i64>,
) -> Result<Json<Option<ReseedRunDetail>>, (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;
    let Some(log) = repo
        .get_task_log(log_id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
    else {
        return Ok(Json(None));
    };

    let task = repo
        .get_task(log.task_id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    if task.as_ref().is_some_and(|t| t.task_type != "reseed") {
        return Ok(Json(None));
    }
    let task_name = task
        .map(|t| t.name)
        .unwrap_or_else(|| format!("任务 #{}", log.task_id));

    let parsed = log
        .log_text
        .as_deref()
        .and_then(|text| serde_json::from_str::<DryRunPreview>(text).ok());

    let dry_run = log.status == "dry_run" || parsed.as_ref().map(|p| p.dry_run).unwrap_or(false);

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
                    outcome: item.outcome.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let total_size = {
        let sum = items
            .iter()
            .filter(|item| item.outcome.as_deref() != Some("skipped"))
            .filter_map(|item| item.total_size)
            .fold(0i64, |acc, n| acc.saturating_add(n));
        if sum > 0 { Some(sum) } else { None }
    };

    Ok(Json(Some(ReseedRunDetail {
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
            history_skipped_count: parsed
                .as_ref()
                .map(|p| p.history_skipped_count)
                .unwrap_or(0),
            created_at: log.created_at,
        },
        items,
    })))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", put(update_task).delete(delete_task))
        .route("/tasks/{id}/trigger", post(trigger_task))
        .route("/tasks/{id}/logs", get(get_task_logs))
        .route("/tasks/{id}/dry-run-preview", get(get_latest_dry_run_preview))
        .route("/reseed-runs", get(get_reseed_runs))
        .route("/reseed-runs/{id}", get(get_reseed_run_detail))
}
