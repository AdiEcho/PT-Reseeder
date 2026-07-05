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
}
