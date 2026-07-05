use crate::state::AppState;
use axum::{extract::State, routing::get, Json, Router};
use serde::Deserialize;

use pt_reseeder_core::stats::reseed::{DashboardOverview, ReseedStatsService, SiteReseedStats, TrendPoint};
use pt_reseeder_core::stats::user_info::{UserInfoAggregate, UserInfoService};

#[derive(Debug, Deserialize)]
pub struct TrendQuery {
    pub days: Option<i64>,
}

async fn get_overview(State(state): State<AppState>) -> Result<Json<DashboardOverview>, axum::http::StatusCode> {
    let svc = ReseedStatsService::new(state.inner.db_pool.clone());
    svc.get_overview()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_site_stats(State(state): State<AppState>) -> Result<Json<Vec<SiteReseedStats>>, axum::http::StatusCode> {
    let svc = ReseedStatsService::new(state.inner.db_pool.clone());
    svc.get_site_reseed_stats()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_trend(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TrendQuery>,
) -> Result<Json<Vec<TrendPoint>>, axum::http::StatusCode> {
    let days = query.days.unwrap_or(7);
    let svc = ReseedStatsService::new(state.inner.db_pool.clone());
    svc.get_trend(days)
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_user_info(State(state): State<AppState>) -> Result<Json<UserInfoAggregate>, axum::http::StatusCode> {
    let svc = UserInfoService::new(state.inner.db_pool.clone());
    svc.get_aggregated_user_info()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub site_id: i64,
    pub limit: Option<i64>,
}

async fn get_user_stats_history(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<Vec<pt_reseeder_core::stats::user_info::UserStatsHistoryPoint>>, axum::http::StatusCode> {
    let svc = UserInfoService::new(state.inner.db_pool.clone());
    svc.get_stats_history(query.site_id, query.limit.unwrap_or(30))
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stats/overview", get(get_overview))
        .route("/stats/sites", get(get_site_stats))
        .route("/stats/trend", get(get_trend))
        .route("/stats/user-info", get(get_user_info))
        .route("/stats/user-history", get(get_user_stats_history))
}
