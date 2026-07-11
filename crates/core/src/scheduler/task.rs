use chrono::Utc;
use croner::Cron;
use serde::{Deserialize, Serialize};

use crate::db::models::TaskRow;
use crate::db::repo::Repository;
use crate::error::{CoreError, SchedulerError};

/// Request payload for creating or updating a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreateRequest {
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub downloader_pair_id: Option<i64>,
    pub config_json: Option<String>,
    pub folder_ids: Vec<i64>,
    pub site_ids: Vec<i64>,
}

/// Manages CRUD operations on tasks and their associations.
pub struct TaskManager {
    repo: Repository,
}

impl TaskManager {
    pub fn new(repo: Repository) -> Self {
        Self { repo }
    }

    /// Create a new task with folder and site associations.
    pub async fn create_task(&self, req: &TaskCreateRequest) -> Result<i64, CoreError> {
        let task_id = self
            .repo
            .create_task(
                &req.name,
                &req.task_type,
                &req.trigger_type,
                req.cron_expression.as_deref(),
                req.downloader_pair_id,
                req.config_json.as_deref(),
            )
            .await?;

        if !req.folder_ids.is_empty() {
            self.repo.set_task_folders(task_id, &req.folder_ids).await?;
        }
        if !req.site_ids.is_empty() {
            self.repo.set_task_sites(task_id, &req.site_ids).await?;
        }

        if let Some(next_run_at) = next_run_at_for(req.cron_expression.as_deref())? {
            self.repo
                .update_task_next_run_at(task_id, Some(&next_run_at))
                .await?;
        }

        Ok(task_id)
    }

    /// Get a task by ID.
    pub async fn get_task(&self, id: i64) -> Result<TaskRow, CoreError> {
        self.repo
            .get_task(id)
            .await?
            .ok_or_else(|| CoreError::Scheduler(SchedulerError::TaskNotFound(id)))
    }

    /// List all tasks.
    pub async fn list_tasks(&self) -> Result<Vec<TaskRow>, CoreError> {
        self.repo.list_tasks().await
    }

    /// Update a task's core fields and associations (preserves task ID).
    pub async fn update_task(&self, id: i64, req: &TaskCreateRequest) -> Result<(), CoreError> {
        // Verify the task exists
        let _ = self.get_task(id).await?;

        // Update core fields in-place
        self.repo
            .update_task(
                id,
                &req.name,
                &req.task_type,
                &req.trigger_type,
                req.cron_expression.as_deref(),
                req.downloader_pair_id,
                req.config_json.as_deref(),
            )
            .await?;

        // Replace associations
        self.repo.set_task_folders(id, &req.folder_ids).await?;
        self.repo.set_task_sites(id, &req.site_ids).await?;

        let next_run_at = next_run_at_for(req.cron_expression.as_deref())?;
        self.repo
            .update_task_next_run_at(id, next_run_at.as_deref())
            .await?;

        Ok(())
    }

    /// Delete a task and its associations (CASCADE handles task_folders/task_sites).
    pub async fn delete_task(&self, id: i64) -> Result<(), CoreError> {
        let _ = self.get_task(id).await?;
        self.repo.delete_task(id).await
    }

    /// Update task status.
    pub async fn update_status(&self, id: i64, status: &str) -> Result<(), CoreError> {
        self.repo.update_task_status(id, status).await
    }

    /// Get folder IDs associated with a task.
    pub async fn get_task_folders(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        self.repo.get_task_folders(task_id).await
    }

    /// Get site IDs associated with a task.
    pub async fn get_task_sites(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        self.repo.get_task_sites(task_id).await
    }

    /// Set folder associations for a task.
    pub async fn set_task_folders(
        &self,
        task_id: i64,
        folder_ids: &[i64],
    ) -> Result<(), CoreError> {
        self.repo.set_task_folders(task_id, folder_ids).await
    }

    /// Set site associations for a task.
    pub async fn set_task_sites(&self, task_id: i64, site_ids: &[i64]) -> Result<(), CoreError> {
        self.repo.set_task_sites(task_id, site_ids).await
    }
}

pub(crate) fn next_run_at_for(cron_expression: Option<&str>) -> Result<Option<String>, CoreError> {
    let Some(expr) = cron_expression.filter(|expr| !expr.trim().is_empty()) else {
        return Ok(None);
    };

    let cron = Cron::new(expr)
        .with_seconds_optional()
        .parse()
        .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;
    let next = cron
        .find_next_occurrence(&Utc::now(), false)
        .map_err(|e| CoreError::Scheduler(SchedulerError::InvalidCron(e.to_string())))?;
    Ok(Some(next.to_rfc3339()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};

    #[test]
    fn next_run_at_for_none_returns_none() {
        let result = next_run_at_for(None).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn next_run_at_for_empty_string_returns_none() {
        let result = next_run_at_for(Some("")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn next_run_at_for_whitespace_only_returns_none() {
        let result = next_run_at_for(Some("   ")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn next_run_at_for_valid_cron_returns_future_time() {
        let result = next_run_at_for(Some("0 0 * * *")).unwrap();
        assert!(result.is_some(), "expected Some for valid cron");
        let time_str = result.unwrap();
        let parsed =
            DateTime::parse_from_rfc3339(&time_str).expect("should be a valid RFC 3339 datetime");
        assert!(parsed > Utc::now(), "next run time should be in the future");
    }

    #[test]
    fn next_run_at_for_every_minute_returns_within_two_minutes() {
        let now = Utc::now();
        let result = next_run_at_for(Some("* * * * *")).unwrap();
        assert!(result.is_some(), "expected Some for every-minute cron");
        let time_str = result.unwrap();
        let parsed =
            DateTime::parse_from_rfc3339(&time_str).expect("should be a valid RFC 3339 datetime");
        let upper_bound = now + Duration::minutes(2);
        assert!(
            parsed <= upper_bound,
            "next run time ({parsed}) should be within 2 minutes of now ({upper_bound})"
        );
    }

    #[test]
    fn next_run_at_for_invalid_cron_returns_error() {
        let result = next_run_at_for(Some("invalid cron"));
        assert!(result.is_err(), "expected Err for invalid cron expression");
    }

    #[test]
    fn task_create_request_serializes_to_json_and_back() {
        let request = TaskCreateRequest {
            name: "test-task".to_string(),
            task_type: "reseed".to_string(),
            trigger_type: "cron".to_string(),
            cron_expression: Some("0 0 * * *".to_string()),
            downloader_pair_id: Some(42),
            config_json: Some(r#"{"key":"value"}"#.to_string()),
            folder_ids: vec![1, 2, 3],
            site_ids: vec![10, 20],
        };

        let json = serde_json::to_string(&request).expect("should serialize to JSON");
        let deserialized: TaskCreateRequest =
            serde_json::from_str(&json).expect("should deserialize from JSON");

        assert_eq!(deserialized.name, request.name);
        assert_eq!(deserialized.task_type, request.task_type);
        assert_eq!(deserialized.trigger_type, request.trigger_type);
        assert_eq!(deserialized.cron_expression, request.cron_expression);
        assert_eq!(deserialized.downloader_pair_id, request.downloader_pair_id);
        assert_eq!(deserialized.config_json, request.config_json);
        assert_eq!(deserialized.folder_ids, request.folder_ids);
        assert_eq!(deserialized.site_ids, request.site_ids);
    }
}
