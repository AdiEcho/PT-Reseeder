use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{CoreError, DbError};

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
pub struct UserStatsHistoryPoint {
    pub fetched_at: String,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub bonus: Option<f64>,
    pub seeding_count: Option<i64>,
}

pub struct UserInfoService {
    pool: SqlitePool,
}

impl UserInfoService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_aggregated_user_info(&self) -> Result<UserInfoAggregate, CoreError> {
        let rows: Vec<SiteUserInfoRow> = sqlx::query_as(
            "SELECT s.id AS site_id, s.name AS site_name, \
             us.uploaded, us.downloaded, us.ratio, us.bonus, us.user_class, \
             us.seeding_count, us.leeching_count, us.seeding_size, \
             us.upload_time_seconds, us.fetched_at \
             FROM sites s \
             INNER JOIN user_stats us ON us.id = ( \
                 SELECT id FROM user_stats WHERE site_id = s.id ORDER BY fetched_at DESC LIMIT 1 \
             ) \
             WHERE s.enabled = 1 \
             ORDER BY s.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;

        let mut total_uploaded: i64 = 0;
        let mut total_downloaded: i64 = 0;
        let mut total_seeding: i64 = 0;
        let mut total_bonus: f64 = 0.0;

        let sites: Vec<SiteUserInfo> = rows
            .into_iter()
            .map(|r| {
                total_uploaded += r.uploaded.unwrap_or(0);
                total_downloaded += r.downloaded.unwrap_or(0);
                total_seeding += r.seeding_count.unwrap_or(0);
                total_bonus += r.bonus.unwrap_or(0.0);
                SiteUserInfo {
                    site_id: r.site_id,
                    site_name: r.site_name,
                    uploaded: r.uploaded,
                    downloaded: r.downloaded,
                    ratio: r.ratio,
                    bonus: r.bonus,
                    user_class: r.user_class,
                    seeding_count: r.seeding_count,
                    leeching_count: r.leeching_count,
                    seeding_size: r.seeding_size,
                    upload_time_seconds: r.upload_time_seconds,
                    fetched_at: r.fetched_at,
                }
            })
            .collect();

        let site_count = sites.len() as i64;

        Ok(UserInfoAggregate {
            total_uploaded,
            total_downloaded,
            total_seeding,
            total_bonus,
            site_count,
            sites,
        })
    }

    pub async fn get_stats_history(
        &self,
        site_id: i64,
        limit: i64,
    ) -> Result<Vec<UserStatsHistoryPoint>, CoreError> {
        let rows: Vec<UserStatsHistoryRow> = sqlx::query_as(
            "SELECT fetched_at, uploaded, downloaded, bonus, seeding_count \
             FROM user_stats WHERE site_id = ? ORDER BY fetched_at DESC LIMIT ?",
        )
        .bind(site_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;

        Ok(rows
            .into_iter()
            .rev()
            .map(|r| UserStatsHistoryPoint {
                fetched_at: r.fetched_at,
                uploaded: r.uploaded,
                downloaded: r.downloaded,
                bonus: r.bonus,
                seeding_count: r.seeding_count,
            })
            .collect())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SiteUserInfoRow {
    site_id: i64,
    site_name: String,
    uploaded: Option<i64>,
    downloaded: Option<i64>,
    ratio: Option<f64>,
    bonus: Option<f64>,
    user_class: Option<String>,
    seeding_count: Option<i64>,
    leeching_count: Option<i64>,
    seeding_size: Option<i64>,
    upload_time_seconds: Option<i64>,
    fetched_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct UserStatsHistoryRow {
    fetched_at: String,
    uploaded: Option<i64>,
    downloaded: Option<i64>,
    bonus: Option<f64>,
    seeding_count: Option<i64>,
}
