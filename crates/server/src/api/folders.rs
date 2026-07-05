use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use pt_reseeder_core::db::models::FolderRow;
use pt_reseeder_core::scheduler::folder::FolderManager;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateFolderRequest {
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateFolderRequest {
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Serialize)]
pub struct FolderResponse {
    pub id: i64,
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    pub enabled: bool,
    pub last_scanned_at: Option<String>,
    pub created_at: String,
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

fn folder_to_response(row: &FolderRow) -> FolderResponse {
    FolderResponse {
        id: row.id,
        path: row.path.clone(),
        scan_mode: row.scan_mode.clone(),
        downloader_id: row.downloader_id,
        enabled: row.enabled,
        last_scanned_at: row.last_scanned_at.clone(),
        created_at: row.created_at.clone(),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /folders -- create a new folder
async fn create_folder(
    State(state): State<AppState>,
    Json(req): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<FolderResponse>), (StatusCode, Json<ApiError>)> {
    // Validate scan_mode
    if req.scan_mode != "local" && req.scan_mode != "downloader" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid scan_mode: '{}', must be 'local' or 'downloader'",
                req.scan_mode
            ),
        ));
    }

    // If scan_mode is "downloader", downloader_id must be provided
    if req.scan_mode == "downloader" && req.downloader_id.is_none() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "downloader_id is required when scan_mode is 'downloader'",
        ));
    }

    let folder_manager = FolderManager::new(state.inner.repo.clone());

    let folder_id = folder_manager
        .create_folder(&req.path, &req.scan_mode, req.downloader_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to create folder: {}", e),
            )
        })?;

    let folder = folder_manager.get_folder(folder_id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("folder created but not found: {}", e),
        )
    })?;

    info!("created folder '{}' (id={})", folder.path, folder.id);
    Ok((StatusCode::CREATED, Json(folder_to_response(&folder))))
}

/// GET /folders -- list all folders
async fn list_folders(
    State(state): State<AppState>,
) -> Result<Json<Vec<FolderResponse>>, (StatusCode, Json<ApiError>)> {
    let folder_manager = FolderManager::new(state.inner.repo.clone());
    let folders = folder_manager.list_folders().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let responses: Vec<FolderResponse> = folders.iter().map(folder_to_response).collect();
    Ok(Json(responses))
}

/// GET /folders/:id -- get folder detail
async fn get_folder(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<FolderResponse>, (StatusCode, Json<ApiError>)> {
    let folder_manager = FolderManager::new(state.inner.repo.clone());

    let folder = folder_manager
        .get_folder(id)
        .await
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("folder not found: {}", e)))?;

    Ok(Json(folder_to_response(&folder)))
}

/// PUT /folders/:id -- update a folder
async fn update_folder(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateFolderRequest>,
) -> Result<Json<FolderResponse>, (StatusCode, Json<ApiError>)> {
    // Validate scan_mode
    if req.scan_mode != "local" && req.scan_mode != "downloader" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!(
                "invalid scan_mode: '{}', must be 'local' or 'downloader'",
                req.scan_mode
            ),
        ));
    }

    // If scan_mode is "downloader", downloader_id must be provided
    if req.scan_mode == "downloader" && req.downloader_id.is_none() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "downloader_id is required when scan_mode is 'downloader'",
        ));
    }

    let folder_manager = FolderManager::new(state.inner.repo.clone());

    folder_manager
        .update_folder(
            id,
            &req.path,
            &req.scan_mode,
            req.downloader_id,
            req.enabled,
        )
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to update folder: {}", e),
            )
        })?;

    let folder = folder_manager.get_folder(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("folder updated but not found: {}", e),
        )
    })?;

    info!("updated folder '{}' (id={})", folder.path, folder.id);
    Ok(Json(folder_to_response(&folder)))
}

/// DELETE /folders/:id -- delete a folder
async fn delete_folder(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let folder_manager = FolderManager::new(state.inner.repo.clone());

    folder_manager
        .delete_folder(id)
        .await
        .map_err(|e| api_err(StatusCode::NOT_FOUND, format!("folder not found: {}", e)))?;

    info!("deleted folder id={}", id);
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/folders", post(create_folder).get(list_folders))
        .route(
            "/folders/{id}",
            get(get_folder).put(update_folder).delete(delete_folder),
        )
}
