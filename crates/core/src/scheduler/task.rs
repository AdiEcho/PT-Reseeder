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
