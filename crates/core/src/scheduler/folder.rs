use crate::db::models::FolderRow;
use crate::db::repo::Repository;
use crate::error::{CoreError, SchedulerError};

/// Manages CRUD operations on scan folders.
pub struct FolderManager {
    repo: Repository,
}

impl FolderManager {
    pub fn new(repo: Repository) -> Self {
        Self { repo }
    }

    /// Create a new folder.
    pub async fn create_folder(
        &self,
        path: &str,
        scan_mode: &str,
        downloader_id: Option<i64>,
    ) -> Result<i64, CoreError> {
        self.repo
            .create_folder(path, scan_mode, downloader_id)
            .await
    }

    /// Get a folder by ID.
    pub async fn get_folder(&self, id: i64) -> Result<FolderRow, CoreError> {
        self.repo
            .get_folder(id)
            .await?
            .ok_or_else(|| CoreError::Scheduler(SchedulerError::FolderNotFound(id)))
    }

    /// List all folders.
    pub async fn list_folders(&self) -> Result<Vec<FolderRow>, CoreError> {
        self.repo.list_folders().await
    }

    /// Update a folder.
    pub async fn update_folder(
        &self,
        id: i64,
        path: &str,
        scan_mode: &str,
        downloader_id: Option<i64>,
        enabled: bool,
    ) -> Result<(), CoreError> {
        // Verify folder exists
        let _ = self.get_folder(id).await?;
        self.repo
            .update_folder(id, path, scan_mode, downloader_id, enabled)
            .await
    }

    /// Delete a folder.
    pub async fn delete_folder(&self, id: i64) -> Result<(), CoreError> {
        let _ = self.get_folder(id).await?;
        self.repo.delete_folder(id).await
    }

    /// Update last_scanned_at timestamp.
    pub async fn mark_scanned(&self, id: i64) -> Result<(), CoreError> {
        self.repo.update_folder_scanned(id).await
    }
}
