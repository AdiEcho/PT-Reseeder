use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use pt_reseeder_core::db::models::{TaskLog, TaskRow};
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
    pub downloader_pair_id: Option<i64>,
    pub config_json: Option<String>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    #[serde(default)]
    pub site_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub downloader_pair_id: Option<i64>,
    pub config_json: Option<String>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    #[serde(default)]
    pub site_ids: Vec<i64>,
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
    pub downloader_pair_id: Option<i64>,
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

fn task_to_response(row: &TaskRow) -> TaskResponse {
    TaskResponse {
        id: row.id,
        name: row.name.clone(),
        task_type: row.task_type.clone(),
        trigger_type: row.trigger_type.clone(),
        cron_expression: row.cron_expression.clone(),
        status: row.status.clone(),
        downloader_pair_id: row.downloader_pair_id,
        last_run_at: row.last_run_at.clone(),
        next_run_at: row.next_run_at.clone(),
        run_count: row.run_count,
        config_json: row.config_json.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /tasks -- create a new task with folder and site associations
async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskDetailResponse>), (StatusCode, Json<ApiError>)> {
    // Validate task_type
    if req.task_type != "reseed" && req.task_type != "repost" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid task_type: '{}', must be 'reseed' or 'repost'",
                req.task_type
            ),
        ));
    }

    // Validate trigger_type
    if req.trigger_type != "manual"
        && req.trigger_type != "cron"
        && req.trigger_type != "file_watch"
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid trigger_type: '{}', must be 'manual', 'cron', or 'file_watch'",
                req.trigger_type
            ),
        ));
    }

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
        downloader_pair_id: req.downloader_pair_id,
        config_json: req.config_json,
        folder_ids: req.folder_ids,
        site_ids: req.site_ids,
    };

    let task_id = task_manager.create_task(&create_req).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to create task: {}", e),
        )
    })?;

    state.reconfigure_task_runtime(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to configure task runtime: {}", e),
        )
    })?;

    let task = task_manager.get_task(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("task created but not found: {}", e),
        )
    })?;

    let folder_ids = task_manager.get_task_folders(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let site_ids = task_manager.get_task_sites(task_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    info!("created task '{}' (id={})", task.name, task.id);
    Ok((
        StatusCode::CREATED,
        Json(TaskDetailResponse {
            task: task_to_response(&task),
            folder_ids,
            site_ids,
        }),
    ))
}

/// GET /tasks -- list all tasks
async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskResponse>>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());
    let tasks = task_manager.list_tasks().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let responses: Vec<TaskResponse> = tasks.iter().map(task_to_response).collect();
    Ok(Json(responses))
}

/// GET /tasks/:id -- get task detail with folder_ids and site_ids
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TaskDetailResponse>, (StatusCode, Json<ApiError>)> {
    let task_manager = TaskManager::new(state.inner.repo.clone());

    let task = task_manager
        .get_task(id)
        .await
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("task not found: {}", e)))?;

    let folder_ids = task_manager.get_task_folders(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let site_ids = task_manager.get_task_sites(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    Ok(Json(TaskDetailResponse {
        task: task_to_response(&task),
        folder_ids,
        site_ids,
    }))
}

/// PUT /tasks/:id -- update a task
async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<TaskDetailResponse>, (StatusCode, Json<ApiError>)> {
    // Validate task_type
    if req.task_type != "reseed" && req.task_type != "repost" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid task_type: '{}', must be 'reseed' or 'repost'",
                req.task_type
            ),
        ));
    }

    // Validate trigger_type
    if req.trigger_type != "manual"
        && req.trigger_type != "cron"
        && req.trigger_type != "file_watch"
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid trigger_type: '{}', must be 'manual', 'cron', or 'file_watch'",
                req.trigger_type
            ),
        ));
    }

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
        downloader_pair_id: req.downloader_pair_id,
        config_json: req.config_json,
        folder_ids: req.folder_ids,
        site_ids: req.site_ids,
    };

    task_manager
        .update_task(id, &update_req)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to update task: {}", e),
            )
        })?;

    state.reconfigure_task_runtime(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to configure task runtime: {}", e),
        )
    })?;

    let task = task_manager.get_task(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("task updated but not found: {}", e),
        )
    })?;

    let folder_ids = task_manager.get_task_folders(task.id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let site_ids = task_manager.get_task_sites(task.id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    info!("updated task '{}' (id={})", task.name, task.id);
    Ok(Json(TaskDetailResponse {
        task: task_to_response(&task),
        folder_ids,
        site_ids,
    }))
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
            format!("failed to remove task runtime: {}", e),
        )
    })?;

    task_manager
        .delete_task(id)
        .await
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("task not found: {}", e)))?;

    info!("deleted task id={}", id);
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
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("task not found: {}", e)))?;

    // Spawn execution asynchronously to avoid timeout
    let exec_state = state.clone();
    tokio::spawn(async move {
        let executor = exec_state.task_executor().await;
        if let Err(e) = executor.execute(id).await {
            warn!(task_id = id, "task execution failed: {}", e);
        }
    });

    info!("triggered task execution id={}", id);
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
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("task not found: {}", e)))?;

    let limit = query.limit.unwrap_or(50);
    let logs = state
        .inner
        .repo
        .get_task_logs(id, limit)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
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
