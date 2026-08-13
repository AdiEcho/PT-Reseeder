use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use pt_reseeder_core::browser::AutofillResult;
use pt_reseeder_core::db::models::RepostQueueEntry;
use pt_reseeder_core::repost::adapter::adapt_torrent_info;
use pt_reseeder_core::repost::models::{AdapterMapping, ReviewAction};
use pt_reseeder_core::repost::review;
use pt_reseeder_core::repost::submitter;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReviewRequest {
    pub action: ReviewAction,
    pub notes: Option<String>,
    pub mapping: Option<AdapterMapping>,
}

#[derive(Serialize)]
pub struct RepostEntryResponse {
    pub id: i64,
    pub source_site_id: i64,
    pub source_torrent_id: String,
    pub target_site_id: i64,
    pub raw_info_json: String,
    pub adapted_info_json: Option<String>,
    pub status: String,
    pub review_notes: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct AutofillResponse {
    pub entry_id: i64,
    pub success: bool,
    pub filled: Vec<String>,
    pub skipped: Vec<String>,
    pub message: String,
    pub target_site: String,
    pub confirmation_required: bool,
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

fn entry_to_response(entry: &RepostQueueEntry) -> RepostEntryResponse {
    RepostEntryResponse {
        id: entry.id,
        source_site_id: entry.source_site_id,
        source_torrent_id: entry.source_torrent_id.clone(),
        target_site_id: entry.target_site_id,
        raw_info_json: entry.raw_info_json.clone(),
        adapted_info_json: entry.adapted_info_json.clone(),
        status: entry.status.clone(),
        review_notes: entry.review_notes.clone(),
        submitted_at: entry.submitted_at.clone(),
        created_at: entry.created_at.clone(),
    }
}

fn core_error_status(error: &pt_reseeder_core::error::CoreError) -> StatusCode {
    let msg = error.to_string();
    if msg.contains("not found") {
        StatusCode::NOT_FOUND
    } else if msg.contains("cannot transition")
        || msg.contains("must be approved")
        || msg.contains("invalid state")
    {
        StatusCode::CONFLICT
    } else if msg.contains("missing") || msg.contains("failed to parse") {
        StatusCode::BAD_REQUEST
    } else if msg.contains("does not support") || msg.contains("not capable") {
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn build_adapted_json(
    state: &AppState,
    entry: &RepostQueueEntry,
    mapping: Option<&AdapterMapping>,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let raw_info: pt_reseeder_core::site::models::RawTorrentInfo =
        serde_json::from_str(&entry.raw_info_json).map_err(|e| {
            api_err(
                StatusCode::BAD_REQUEST,
                format!("failed to parse raw info: {}", e),
            )
        })?;

    let target_site = state
        .inner
        .repo
        .get_site(entry.target_site_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "target site not found"))?;

    let adapted = adapt_torrent_info(&raw_info, &target_site.name, mapping)
        .map_err(|e| api_err(core_error_status(&e), format!("adaptation failed: {}", e)))?;

    serde_json::to_string(&adapted).map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("serialization error: {}", e),
        )
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /repost/queue/:id/review -- approve or reject a pending entry
async fn review_entry(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<RepostEntryResponse>, (StatusCode, Json<ApiError>)> {
    let entry = review::get_entry(&state.inner.repo, id)
        .await
        .map_err(|e| api_err(core_error_status(&e), format!("{}", e)))?;

    if matches!(req.action, ReviewAction::Approve) {
        let adapted_json = build_adapted_json(&state, &entry, req.mapping.as_ref()).await?;

        state
            .inner
            .repo
            .update_repost_status(
                id,
                &entry.status,
                entry.review_notes.as_deref(),
                Some(&adapted_json),
                None,
            )
            .await
            .map_err(|e| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("database error: {}", e),
                )
            })?;
    }

    let updated = review::review_entry(&state.inner.repo, id, &req.action, req.notes.as_deref())
        .await
        .map_err(|e| api_err(core_error_status(&e), format!("{}", e)))?;

    Ok(Json(entry_to_response(&updated)))
}

/// POST /repost/queue/:id/autofill -- open and fill the target upload form without submitting it
async fn autofill_entry(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AutofillResponse>, (StatusCode, Json<ApiError>)> {
    let autofiller = state.inner.repost_autofiller.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            state
                .inner
                .repost_autofiller_error
                .clone()
                .unwrap_or_else(|| "headless repost autofill is unavailable".to_string()),
        )
    })?;

    let entry = review::get_entry(&state.inner.repo, id)
        .await
        .map_err(|e| api_err(core_error_status(&e), format!("{}", e)))?;
    if entry.status != "approved" {
        return Err(api_err(
            StatusCode::CONFLICT,
            format!(
                "entry must be approved before autofill; current status: '{}'",
                entry.status
            ),
        ));
    }

    let adapted_json = entry.adapted_info_json.as_deref().ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "entry has no adapted info; approve it first",
        )
    })?;
    let adapted: pt_reseeder_core::site::models::AdaptedTorrentInfo =
        serde_json::from_str(adapted_json).map_err(|e| {
            api_err(
                StatusCode::BAD_REQUEST,
                format!("failed to parse adapted info: {e}"),
            )
        })?;
    let target_site = state
        .inner
        .repo
        .get_site(entry.target_site_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "target site not found"))?;

    let AutofillResult {
        entry_id,
        success,
        filled,
        skipped,
        message,
    } = submitter::autofill_upload_page(autofiller.as_ref(), &target_site.url, id, &adapted)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("autofill failed: {e}"),
            )
        })?;

    Ok(Json(AutofillResponse {
        entry_id,
        success,
        filled,
        skipped,
        message,
        target_site: target_site.name,
        confirmation_required: true,
    }))
}

/// POST /repost/queue/:id/submit -- submit an approved entry to the target site
async fn submit_entry(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<RepostEntryResponse>, (StatusCode, Json<ApiError>)> {
    let registry = state.site_registry_snapshot().await;
    let updated = submitter::submit_entry(&state.inner.repo, &registry, id)
        .await
        .map_err(|e| api_err(core_error_status(&e), format!("{}", e)))?;

    Ok(Json(entry_to_response(&updated)))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    // Only the three endpoints the hydrated page calls directly. Everything else
    // in this domain is served by server functions (see server_fns/repost.rs);
    // these three stay because `review_entry` also writes `adapted_info_json`,
    // which `autofill_entry` requires and the server-fn path does not produce.
    Router::new()
        .route("/repost/queue/{id}/review", post(review_entry))
        .route("/repost/queue/{id}/autofill", post(autofill_entry))
        .route("/repost/queue/{id}/submit", post(submit_entry))
}
