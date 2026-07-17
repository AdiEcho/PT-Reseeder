use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use pt_reseeder_core::db::models::{TaskLog, TaskRow};
use pt_reseeder_core::error::{CoreError, SchedulerError};
use pt_reseeder_core::scheduler::task::{TaskCreateRequest, TaskManager};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub destination_downloader_id: Option<i64>,
    pub config_json: Option<String>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    #[serde(default)]
    pub site_ids: Vec<i64>,
    #[serde(default)]
    pub source_downloader_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub destination_downloader_id: Option<i64>,
    pub config_json: Option<String>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    #[serde(default)]
    pub site_ids: Vec<i64>,
    #[serde(default)]
    pub source_downloader_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct TaskLogsQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct TaskResponse {
    pub id: i64,
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub status: String,
    pub destination_downloader_id: Option<i64>,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: Option<i64>,
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct TaskDetailResponse {
    #[serde(flatten)]
    pub task: TaskResponse,
    pub folder_ids: Vec<i64>,
    pub site_ids: Vec<i64>,
    pub source_downloader_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct RunTaskResponse {
    pub message: String,
    pub task_id: i64,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

fn map_task_manager_error(e: CoreError, action: &str) -> (StatusCode, Json<ApiError>) {
    match e {
        CoreError::Scheduler(SchedulerError::InvalidConfig(msg))
        | CoreError::Scheduler(SchedulerError::InvalidCron(msg)) => {
            api_err(StatusCode::BAD_REQUEST, msg)
        }
        CoreError::Scheduler(SchedulerError::TaskNotFound(id)) => {
            api_err(StatusCode::NOT_FOUND, format!("task not found: {id}"))
        }
        other => api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to {action}: {other}"),
        ),
    }
}

fn task_to_response(row: &TaskRow) -> TaskResponse {
    TaskResponse {
        id: row.id,
        name: row.name.clone(),
        task_type: row.task_type.clone(),
        trigger_type: row.trigger_type.clone(),
        cron_expression: row.cron_expression.clone(),
        status: row.status.clone(),
        destination_downloader_id: row.destination_downloader_id,
        last_run_at: row.last_run_at.clone(),
        next_run_at: row.next_run_at.clone(),
        run_count: row.run_count,
        config_json: row.config_json.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

async fn load_task_detail(
    task_manager: &TaskManager,
    task_id: i64,
) -> Result<TaskDetailResponse, (StatusCode, Json<ApiError>)> {
    let task = task_manager.get_task(task_id).await.map_err(|e| {
        map_task_manager_error(e, "load task")
    })?;

    let folder_ids = task_manager.get_task_folders(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let site_ids = task_manager.get_task_sites(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let source_downloader_ids = task_manager
        .get_task_source_downloaders(task_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {e}"),
            )
        })?;

    Ok(TaskDetailResponse {
        task: task_to_response(&task),
        folder_ids,
        site_ids,
        source_downloader_ids,
    })
}

fn validate_trigger_type(trigger_type: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    if trigger_type != "manual" && trigger_type != "cron" && trigger_type != "file_watch" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid trigger_type: '{trigger_type}', must be 'manual', 'cron', or 'file_watch'"
            ),
        ));
    }
    Ok(())
}

fn validate_task_type(task_type: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    if task_type != "reseed" && task_type != "repost" && task_type != "sync_stats" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid task_type: '{task_type}', must be 'reseed', 'repost', or 'sync_stats'"
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /tasks -- create a new task with folder and site associations
async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskDetailResponse>), (StatusCode, Json<ApiError>)> {
    validate_task_type(&req.task_type)?;
    validate_trigger_type(&req.trigger_type)?;

    // If trigger_type is "cron", cron_expression must be provided
    if req.trigger_type == "cron" && req.cron_expression.as_ref().map_or(true, |s| s.is_empty()) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "cron_expression is required when trigger_type is 'cron'",
        ));
    }

    let task_manager = TaskManager::new(state.inner.repo.clone());
    let create_req = TaskCreateRequest {
        name: req.name,
        task_type: req.task_type,
        trigger_type: req.trigger_type,
        cron_expression: req.cron_expression,
        destination_downloader_id: req.destination_downloader_id,
        config_json: req.config_json,
        folder_ids: req.folder_ids,
        site_ids: req.site_ids,
        source_downloader_ids: req.source_downloader_ids,
    };

    let task_id = task_manager
        .create_task(&create_req)
        .await
        .map_err(|e| map_task_manager_error(e, "create task"))?;

    state.reconfigure_task_runtime(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to configure task runtime: {e}"),
        )
    })?;

    let detail = load_task_detail(&task_manager, task_id).await?;
    info!("created task '{}' (id={})", detail.task.name, detail.task.id);
    Ok((StatusCode::CREATED, Json(detail)))
}

/// GET /tasks -- list all tasks
async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskResponse>>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let tasks = task_manager.list_tasks().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    let responses: Vec<TaskResponse> = tasks.iter().map(task_to_response).collect();
    Ok(Json(responses))
}

/// GET /tasks/:id -- get task detail with associations
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TaskDetailResponse>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let detail = load_task_detail(&task_manager, id).await?;
    Ok(Json(detail))
}

/// PUT /tasks/:id -- update a task
async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<TaskDetailResponse>, (StatusCode, Json<ApiError>)> {
    validate_task_type(&req.task_type)?;
    validate_trigger_type(&req.trigger_type)?;

    // If trigger_type is "cron", cron_expression must be provided
    if req.trigger_type == "cron" && req.cron_expression.as_ref().map_or(true, |s| s.is_empty()) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "cron_expression is required when trigger_type is 'cron'",
        ));
    }

    let task_manager = TaskManager::new(state.inner.repo.clone());
    let update_req = TaskCreateRequest {
        name: req.name,
        task_type: req.task_type,
        trigger_type: req.trigger_type,
        cron_expression: req.cron_expression,
        destination_downloader_id: req.destination_downloader_id,
        config_json: req.config_json,
        folder_ids: req.folder_ids,
        site_ids: req.site_ids,
        source_downloader_ids: req.source_downloader_ids,
    };

    task_manager
        .update_task(id, &update_req)
        .await
        .map_err(|e| map_task_manager_error(e, "update task"))?;

    state.reconfigure_task_runtime(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to configure task runtime: {e}"),
        )
    })?;

    let detail = load_task_detail(&task_manager, id).await?;
    info!("updated task '{}' (id={})", detail.task.name, detail.task.id);
    Ok(Json(detail))
}

/// DELETE /tasks/:id -- delete a task
async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());

    state.remove_task_runtime(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to remove task runtime: {e}"),
        )
    })?;

    task_manager
        .delete_task(id)
        .await
        .map_err(|e| map_task_manager_error(e, "delete task"))?;

    info!("deleted task id={id}");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /tasks/:id/run -- manually trigger task execution
async fn run_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<RunTaskResponse>), (StatusCode, Json<ApiError>)> {
    // Verify task exists
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let _task = task_manager
        .get_task(id)
        .await
        .map_err(|e| map_task_manager_error(e, "load task"))?;

    // Spawn execution asynchronously to avoid timeout
    let exec_state = state.clone();
    tokio::spawn(async move {
        let executor = exec_state.task_executor().await;
        if let Err(e) = executor.execute(id).await {
            warn!(task_id = id, "task execution failed: {}", e);
        }
    });

    info!("triggered task execution id={id}");
    Ok((
        StatusCode::ACCEPTED,
        Json(RunTaskResponse {
            message: "task execution started".to_string(),
            task_id: id,
        }),
    ))
}

/// GET /tasks/:id/logs -- get task run logs
async fn get_task_logs(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<TaskLogsQuery>,
) -> Result<Json<Vec<TaskLog>>, (StatusCode, Json<ApiError>)> {
    // Verify task exists
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let _task = task_manager
        .get_task(id)
        .await
        .map_err(|e| map_task_manager_error(e, "load task"))?;

    let limit = query.limit.unwrap_or(50);
    let logs = state
        .inner
        .repo
        .get_task_logs(id, limit)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {e}"),
            )
        })?;

    Ok(Json(logs))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", post(create_task).get(list_tasks))
        .route(
            "/tasks/{id}",
            get(get_task).put(update_task).delete(delete_task),
        )
        .route("/tasks/{id}/run", post(run_task))
        .route("/tasks/{id}/logs", get(get_task_logs))
}
