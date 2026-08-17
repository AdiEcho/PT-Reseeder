use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

pub const FETCH_SEEDING_SIZE_CONFIG_KEY: &str = "fetch_seeding_size";

#[derive(Debug, Clone, Serialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub key: String,
    pub value: String,
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

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<Vec<ConfigEntry>>, (StatusCode, Json<ApiError>)> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT key, value, updated_at FROM app_config ORDER BY key",
    )
    .fetch_all(&state.inner.db_pool)
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(key, value, updated_at)| ConfigEntry {
                key,
                value,
                updated_at,
            })
            .collect(),
    ))
}

async fn update_config(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> Result<Json<()>, (StatusCode, Json<ApiError>)> {
    let normalized_value = if body.key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        match body.value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => "true".to_string(),
            "false" | "0" => "false".to_string(),
            _ => {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "做种大小开关的值必须为 true 或 false",
                ));
            }
        }
    } else {
        body.value
    };

    state
        .inner
        .repo
        .set_config(&body.key, &normalized_value)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if body.key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        state.inner.fetch_seeding_size.store(
            normalized_value == "true",
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    Ok(Json(()))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_config))
        .route("/config", put(update_config))
}
