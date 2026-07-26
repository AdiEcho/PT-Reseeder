// Monitored folders: DTO and CRUD server functions.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub id: i64,
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    pub enabled: bool,
    pub last_scanned_at: Option<String>,
}

#[server]
pub async fn get_folders() -> Result<Vec<FolderInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .list_folders()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|f| FolderInfo {
            id: f.id,
            path: f.path,
            scan_mode: f.scan_mode,
            downloader_id: f.downloader_id,
            enabled: f.enabled,
            last_scanned_at: f.last_scanned_at,
        })
        .collect())
}

#[server]
pub async fn create_folder(
    path: String,
    scan_mode: String,
    downloader_id: Option<i64>,
) -> Result<FolderInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let id = repo
        .create_folder(&path, &scan_mode, downloader_id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    get_folders()
        .await?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| ServerFnError::new("folder created but not found"))
}

#[server]
pub async fn delete_folder(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_folder(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}
