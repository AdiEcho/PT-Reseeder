use pt_reseeder_core::engine::ReseedProgress as CoreReseedProgress;
use pt_reseeder_core::stats::reseed::{DashboardOverview, SiteReseedStats};
use pt_reseeder_core::stats::user_info::UserInfoAggregate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    #[serde(rename = "dashboard_update")]
    DashboardUpdate {
        overview: Option<DashboardOverview>,
        site_stats: Option<Vec<SiteReseedStats>>,
        user_info: Option<UserInfoAggregate>,
    },
    #[serde(rename = "task_progress")]
    TaskProgress {
        task_id: i64,
        status: String,
        matched_count: i64,
        succeeded_count: i64,
        failed_count: i64,
    },
    #[serde(rename = "reseed_progress")]
    ReseedProgress {
        task_id: Option<i64>,
        progress: CoreReseedProgress,
    },
    #[serde(rename = "log_line")]
    LogLine {
        line: String,
    },
}
