use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub id: i64,
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    pub enabled: bool,
    pub last_scanned_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_folders(State(state): State<AppState>) -> ApiResult<Vec<FolderInfo>> {
    let rows = state
        .inner
        .repo
        .list_folders()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        rows.into_iter()
            .map(|f| FolderInfo {
                id: f.id,
                path: f.path,
                scan_mode: f.scan_mode,
                downloader_id: f.downloader_id,
                enabled: f.enabled,
                last_scanned_at: f.last_scanned_at,
            })
            .collect(),
    ))
}

async fn create_folder(
    State(state): State<AppState>,
    Json(req): Json<CreateFolderRequest>,
) -> ApiResult<FolderInfo> {
    let path = req.path.trim().to_string();
    if path.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "路径不能为空"));
    }

    let id = state
        .inner
        .repo
        .create_folder(&path, &req.scan_mode, req.downloader_id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let folder = state
        .inner
        .repo
        .get_folder(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "folder created but not found"))?;

    Ok(Json(FolderInfo {
        id: folder.id,
        path: folder.path,
        scan_mode: folder.scan_mode,
        downloader_id: folder.downloader_id,
        enabled: folder.enabled,
        last_scanned_at: folder.last_scanned_at,
    }))
}

async fn delete_folder_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .inner
        .repo
        .delete_folder(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/folders", get(list_folders).post(create_folder))
        .route("/folders/{id}", delete(delete_folder_handler))
}
