use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use pt_reseeder_core::stats::reseed::ReseedStatsService;
use pt_reseeder_core::stats::user_info::UserInfoService;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DashboardQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    7
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardOverview {
    pub running_tasks: i64,
    pub today_success: i64,
    pub today_failed: i64,
    pub total_sites: i64,
    pub tracked_torrents: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteReseedStats {
    pub site_id: i64,
    pub site_name: String,
    pub matched: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub success_rate: f64,
    pub breaker_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    pub date: String,
    pub succeeded: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteUserInfo {
    pub site_id: i64,
    pub site_name: String,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub ratio: Option<f64>,
    pub bonus: Option<f64>,
    pub user_class: Option<String>,
    pub seeding_count: Option<i64>,
    pub leeching_count: Option<i64>,
    pub seeding_size: Option<i64>,
    pub upload_time_seconds: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserInfoAggregate {
    pub total_uploaded: i64,
    pub total_downloaded: i64,
    pub total_seeding: i64,
    pub total_bonus: f64,
    pub site_count: i64,
    pub sites: Vec<SiteUserInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub overview: DashboardOverview,
    pub site_stats: Vec<SiteReseedStats>,
    pub trend: Vec<TrendPoint>,
    pub user_info: UserInfoAggregate,
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
// Handler
// ---------------------------------------------------------------------------

async fn get_dashboard(
    State(state): State<AppState>,
    Query(params): Query<DashboardQuery>,
) -> Result<Json<DashboardData>, (StatusCode, Json<ApiError>)> {
    let pool = &state.inner.db_pool;

    let reseed_svc = ReseedStatsService::new(pool.clone());
    let user_svc = UserInfoService::new(pool.clone());

    let overview = reseed_svc
        .get_overview()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let site_stats = reseed_svc
        .get_site_reseed_stats()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let trend = reseed_svc
        .get_trend(params.days)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user_info = user_svc
        .get_aggregated_user_info()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(DashboardData {
        overview: DashboardOverview {
            running_tasks: overview.running_tasks,
            today_success: overview.today_success,
            today_failed: overview.today_failed,
            total_sites: overview.total_sites,
            tracked_torrents: overview.tracked_torrents,
        },
        site_stats: site_stats
            .into_iter()
            .map(|s| SiteReseedStats {
                site_id: s.site_id,
                site_name: s.site_name,
                matched: s.matched,
                succeeded: s.succeeded,
                failed: s.failed,
                skipped: s.skipped,
                success_rate: s.success_rate,
                breaker_status: s.breaker_status,
            })
            .collect(),
        trend: trend
            .into_iter()
            .map(|t| TrendPoint {
                date: t.date,
                succeeded: t.succeeded,
                failed: t.failed,
            })
            .collect(),
        user_info: UserInfoAggregate {
            total_uploaded: user_info.total_uploaded,
            total_downloaded: user_info.total_downloaded,
            total_seeding: user_info.total_seeding,
            total_bonus: user_info.total_bonus,
            site_count: user_info.site_count,
            sites: user_info
                .sites
                .into_iter()
                .map(|s| SiteUserInfo {
                    site_id: s.site_id,
                    site_name: s.site_name,
                    uploaded: s.uploaded,
                    downloaded: s.downloaded,
                    ratio: s.ratio,
                    bonus: s.bonus,
                    user_class: s.user_class,
                    seeding_count: s.seeding_count,
                    leeching_count: s.leeching_count,
                    seeding_size: s.seeding_size,
                    upload_time_seconds: s.upload_time_seconds,
                    fetched_at: s.fetched_at,
                })
                .collect(),
        },
    }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(get_dashboard))
}
