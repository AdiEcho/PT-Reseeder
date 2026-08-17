use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RepostEntry {
    pub id: i64,
    pub source_site_name: String,
    pub source_torrent_id: String,
    pub target_site_name: String,
    pub status: String,
    pub review_notes: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct QueueParams {
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /repost/queue
async fn list_repost_queue(
    State(state): State<AppState>,
    Query(params): Query<QueueParams>,
) -> Result<Json<Vec<RepostEntry>>, (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;

    // Load all sites into a HashMap for ID->name mapping.
    let sites = repo
        .list_sites()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    let site_names: HashMap<i64, String> = sites.into_iter().map(|s| (s.id, s.name)).collect();

    let entries = repo
        .list_repost_entries(params.status.as_deref())
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    let result = entries
        .into_iter()
        .map(|e| {
            let source_site_name = site_names
                .get(&e.source_site_id)
                .cloned()
                .unwrap_or_else(|| format!("站点 #{}", e.source_site_id));
            let target_site_name = site_names
                .get(&e.target_site_id)
                .cloned()
                .unwrap_or_else(|| format!("站点 #{}", e.target_site_id));
            RepostEntry {
                id: e.id,
                source_site_name,
                source_torrent_id: e.source_torrent_id,
                target_site_name,
                status: e.status,
                review_notes: e.review_notes,
                submitted_at: e.submitted_at,
                created_at: e.created_at,
            }
        })
        .collect();

    Ok(Json(result))
}

/// DELETE /repost/queue/:id
async fn delete_repost_entry(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .inner
        .repo
        .delete_repost_entry(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repost/queue", get(list_repost_queue))
        .route("/repost/queue/{id}", delete(delete_repost_entry))
}
