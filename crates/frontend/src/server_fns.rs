use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardOverview {
    pub running_tasks: i64,
    pub today_success: i64,
    pub today_failed: i64,
    pub total_sites: i64,
    pub tracked_torrents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteReseedStats {
    pub site_id: i64,
    pub site_name: String,
    pub matched: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub success_rate: f64,
    /// Circuit breaker status: "ok" | "tripped" | "unknown"
    pub breaker_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub date: String,
    pub succeeded: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoAggregate {
    pub total_uploaded: i64,
    pub total_downloaded: i64,
    pub total_seeding: i64,
    pub total_bonus: f64,
    pub site_count: i64,
    pub sites: Vec<SiteUserInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub overview: DashboardOverview,
    pub site_stats: Vec<SiteReseedStats>,
    pub trend: Vec<TrendPoint>,
    pub user_info: UserInfoAggregate,
}

#[server]
pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
    let _ = (&username, &password);
    Ok(())
}

#[server]
pub async fn register(username: String, password: String) -> Result<(), ServerFnError> {
    let _ = (&username, &password);
    Ok(())
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    Ok(None)
}

#[server]
pub async fn get_dashboard_data(days: i64) -> Result<DashboardData, ServerFnError> {
    use pt_reseeder_core::stats::reseed::ReseedStatsService;
    use pt_reseeder_core::stats::user_info::UserInfoService;

    let pool: sqlx::SqlitePool = expect_context();

    let reseed_svc = ReseedStatsService::new(pool.clone());
    let user_svc = UserInfoService::new(pool);

    let overview = reseed_svc
        .get_overview()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let site_stats = reseed_svc
        .get_site_reseed_stats()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let trend = reseed_svc
        .get_trend(days)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let user_info = user_svc
        .get_aggregated_user_info()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    Ok(DashboardData {
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
    })
}
