use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{CoreError, DbError};

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

pub struct ReseedStatsService {
    pool: SqlitePool,
}

impl ReseedStatsService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_overview(&self) -> Result<DashboardOverview, CoreError> {
        let running_tasks: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tasks WHERE status = 'running'")
                .fetch_one(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;

        let today_success: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM reseed_history \
             WHERE status = 'success' AND created_at >= date('now')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;

        let today_failed: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM reseed_history \
             WHERE status = 'failed' AND created_at >= date('now')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;

        let total_sites: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sites WHERE enabled = 1")
            .fetch_one(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;

        let tracked_torrents: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT pieces_hash) FROM pieces_cache")
                .fetch_one(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;

        Ok(DashboardOverview {
            running_tasks: running_tasks.0,
            today_success: today_success.0,
            today_failed: today_failed.0,
            total_sites: total_sites.0,
            tracked_torrents: tracked_torrents.0,
        })
    }

    pub async fn get_site_reseed_stats(&self) -> Result<Vec<SiteReseedStats>, CoreError> {
        let rows: Vec<SiteReseedRow> = sqlx::query_as(
            "SELECT s.id AS site_id, s.name AS site_name, \
             COALESCE(SUM(CASE WHEN rh.status = 'success' THEN 1 ELSE 0 END), 0) AS succeeded, \
             COALESCE(SUM(CASE WHEN rh.status = 'failed' THEN 1 ELSE 0 END), 0) AS failed, \
             COALESCE(SUM(CASE WHEN rh.status = 'skipped' THEN 1 ELSE 0 END), 0) AS skipped, \
             COUNT(rh.id) AS matched \
             FROM sites s \
             LEFT JOIN reseed_history rh ON rh.site_id = s.id \
             WHERE s.enabled = 1 \
             GROUP BY s.id, s.name \
             ORDER BY matched DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let success_rate = if r.matched > 0 {
                    r.succeeded as f64 / r.matched as f64 * 100.0
                } else {
                    0.0
                };
                SiteReseedStats {
                    site_id: r.site_id,
                    site_name: r.site_name,
                    matched: r.matched,
                    succeeded: r.succeeded,
                    failed: r.failed,
                    skipped: r.skipped,
                    success_rate,
                    breaker_status: "unknown".to_string(),
                }
            })
            .collect())
    }

    pub async fn get_trend(&self, days: i64) -> Result<Vec<TrendPoint>, CoreError> {
        let rows: Vec<TrendRow> = if days <= 0 {
            sqlx::query_as(
                "SELECT date(created_at) AS date, \
                 SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS succeeded, \
                 SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed \
                 FROM reseed_history \
                 GROUP BY date(created_at) \
                 ORDER BY date(created_at)",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?
        } else {
            sqlx::query_as(
                "SELECT date(created_at) AS date, \
                 SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS succeeded, \
                 SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed \
                 FROM reseed_history \
                 WHERE created_at >= date('now', '-' || ?1 || ' days') \
                 GROUP BY date(created_at) \
                 ORDER BY date(created_at)",
            )
            .bind(days)
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?
        };

        Ok(rows
            .into_iter()
            .map(|r| TrendPoint {
                date: r.date,
                succeeded: r.succeeded,
                failed: r.failed,
            })
            .collect())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SiteReseedRow {
    site_id: i64,
    site_name: String,
    matched: i64,
    succeeded: i64,
    failed: i64,
    skipped: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TrendRow {
    date: String,
    succeeded: i64,
    failed: i64,
}
