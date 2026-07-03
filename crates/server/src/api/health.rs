use axum::{Router, routing::get, Json, extract::State};
use serde::Serialize;
use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub db_status: String,
    pub timestamp: String,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.inner.start_time.elapsed().as_secs();

    let db_status = match sqlx::query("SELECT 1")
        .execute(&state.inner.db_pool)
        .await
    {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("error: {}", e),
    };

    Json(HealthResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        db_status,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}
