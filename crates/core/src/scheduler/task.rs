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
    pub destination_downloader_id: Option<i64>,
    pub config_json: Option<String>,
    pub folder_ids: Vec<i64>,
    pub site_ids: Vec<i64>,
    #[serde(default)]
    pub source_downloader_ids: Vec<i64>,
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
        validate_task_request(req)?;

        // Create core row first, then associations. On any later failure, delete the
        // orphan task so callers never observe a half-configured reseed task.
        let task_id = self
            .repo
            .create_task(
                &req.name,
                &req.task_type,
                &req.trigger_type,
                req.cron_expression.as_deref(),
                req.destination_downloader_id,
                req.config_json.as_deref(),
            )
            .await?;

        if let Err(error) = self
            .persist_associations_and_schedule(task_id, req)
            .await
        {
            if let Err(cleanup_error) = self.repo.delete_task(task_id).await {
                tracing::error!(
                    task_id,
                    error = %cleanup_error,
                    "failed to clean up orphan task after create association error"
                );
            }
            return Err(error);
        }

        Ok(task_id)
    }

    async fn persist_associations_and_schedule(
        &self,
        task_id: i64,
        req: &TaskCreateRequest,
    ) -> Result<(), CoreError> {
        self.repo.set_task_folders(task_id, &req.folder_ids).await?;
        self.repo.set_task_sites(task_id, &req.site_ids).await?;
        self.repo
            .set_task_source_downloaders(task_id, &req.source_downloader_ids)
            .await?;

        if let Some(next_run_at) = next_run_at_for(req.cron_expression.as_deref())? {
            self.repo
                .update_task_next_run_at(task_id, Some(&next_run_at))
                .await?;
        }
        Ok(())
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
        validate_task_request(req)?;

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
                req.destination_downloader_id,
                req.config_json.as_deref(),
            )
            .await?;

        // Replace associations
        self.repo.set_task_folders(id, &req.folder_ids).await?;
        self.repo.set_task_sites(id, &req.site_ids).await?;
        self.repo
            .set_task_source_downloaders(id, &req.source_downloader_ids)
            .await?;

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

    /// Get source downloader IDs associated with a task.
    pub async fn get_task_source_downloaders(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        self.repo.get_task_source_downloaders(task_id).await
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

    /// Set source downloader associations for a task.
    pub async fn set_task_source_downloaders(
        &self,
        task_id: i64,
        downloader_ids: &[i64],
    ) -> Result<(), CoreError> {
        self.repo
            .set_task_source_downloaders(task_id, downloader_ids)
            .await
    }
}

fn validate_task_request(req: &TaskCreateRequest) -> Result<(), CoreError> {
    if req.trigger_type == "cron" {
        // Ensure cron expression is parseable when provided for cron tasks.
        let _ = next_run_at_for(req.cron_expression.as_deref())?;
        if req
            .cron_expression
            .as_ref()
            .map_or(true, |s| s.trim().is_empty())
        {
            return Err(CoreError::Scheduler(SchedulerError::InvalidConfig(
                "cron_expression is required when trigger_type is 'cron'".to_string(),
            )));
        }
    }

    match req.task_type.as_str() {
        "reseed" => {
            if req.site_ids.is_empty() {
                return Err(CoreError::Scheduler(SchedulerError::InvalidConfig(
                    "reseed tasks require at least one site".to_string(),
                )));
            }
            if req.source_downloader_ids.is_empty() && req.folder_ids.is_empty() {
                return Err(CoreError::Scheduler(SchedulerError::InvalidConfig(
                    "reseed tasks require at least one source (folder or source downloader)"
                        .to_string(),
                )));
            }
            if req.destination_downloader_id.is_none() {
                return Err(CoreError::Scheduler(SchedulerError::InvalidConfig(
                    "reseed tasks require destination_downloader_id".to_string(),
                )));
            }
        }
        "repost" | "sync_stats" => {}
        other => {
            return Err(CoreError::Scheduler(SchedulerError::InvalidConfig(
                format!("unsupported task_type: {}", other),
            )));
        }
    }

    Ok(())
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

    fn base_request(task_type: &str) -> TaskCreateRequest {
        TaskCreateRequest {
            name: "test-task".to_string(),
            task_type: task_type.to_string(),
            trigger_type: "manual".to_string(),
            cron_expression: None,
            destination_downloader_id: None,
            config_json: None,
            folder_ids: vec![],
            site_ids: vec![],
            source_downloader_ids: vec![],
        }
    }

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
            destination_downloader_id: Some(7),
            config_json: Some(r#"{"key":"value"}"#.to_string()),
            folder_ids: vec![1, 2, 3],
            site_ids: vec![10, 20],
            source_downloader_ids: vec![5, 6],
        };

        let json = serde_json::to_string(&request).expect("should serialize to JSON");
        let deserialized: TaskCreateRequest =
            serde_json::from_str(&json).expect("should deserialize from JSON");

        assert_eq!(deserialized.name, request.name);
        assert_eq!(deserialized.task_type, request.task_type);
        assert_eq!(deserialized.trigger_type, request.trigger_type);
        assert_eq!(deserialized.cron_expression, request.cron_expression);
        assert_eq!(
            deserialized.destination_downloader_id,
            request.destination_downloader_id
        );
        assert_eq!(deserialized.config_json, request.config_json);
        assert_eq!(deserialized.folder_ids, request.folder_ids);
        assert_eq!(deserialized.site_ids, request.site_ids);
        assert_eq!(
            deserialized.source_downloader_ids,
            request.source_downloader_ids
        );
    }

    #[test]
    fn validate_reseed_requires_sites_sources_destination() {
        let mut req = base_request("reseed");
        assert!(validate_task_request(&req).is_err());

        req.site_ids = vec![1];
        assert!(validate_task_request(&req).is_err());

        req.folder_ids = vec![2];
        assert!(validate_task_request(&req).is_err());

        req.destination_downloader_id = Some(3);
        assert!(validate_task_request(&req).is_ok());
    }

    #[test]
    fn validate_reseed_accepts_folder_only_or_downloader_only() {
        let mut folder_only = base_request("reseed");
        folder_only.site_ids = vec![1];
        folder_only.folder_ids = vec![2];
        folder_only.destination_downloader_id = Some(3);
        assert!(validate_task_request(&folder_only).is_ok());

        let mut dl_only = base_request("reseed");
        dl_only.site_ids = vec![1];
        dl_only.source_downloader_ids = vec![9];
        dl_only.destination_downloader_id = Some(3);
        assert!(validate_task_request(&dl_only).is_ok());
    }

    #[test]
    fn validate_non_reseed_skips_source_rules() {
        let sync = base_request("sync_stats");
        assert!(validate_task_request(&sync).is_ok());

        let mut repost = base_request("repost");
        repost.site_ids = vec![];
        assert!(validate_task_request(&repost).is_ok());
    }

}
