// Repost queue: DTO and server functions for listing, review, submission and
// deletion.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepostEntry {
    pub id: i64,
    pub source_site_name: String,
    pub source_torrent_id: String,
    pub target_site_name: String,
    pub status: String,
    pub review_notes: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
}

#[server]
pub async fn get_repost_queue(
    status_filter: Option<String>,
) -> Result<Vec<RepostEntry>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let sites: std::collections::HashMap<i64, String> = repo
        .list_sites()
        .await?
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    let entries = repo
        .list_repost_entries(status_filter.as_deref())
        .await?;
    Ok(entries
        .into_iter()
        .map(|e| RepostEntry {
            id: e.id,
            source_site_name: sites
                .get(&e.source_site_id)
                .cloned()
                .unwrap_or_else(|| e.source_site_id.to_string()),
            source_torrent_id: e.source_torrent_id,
            target_site_name: sites
                .get(&e.target_site_id)
                .cloned()
                .unwrap_or_else(|| e.target_site_id.to_string()),
            status: e.status,
            review_notes: e.review_notes,
            submitted_at: e.submitted_at,
            created_at: e.created_at,
        })
        .collect())
}

#[server]
pub async fn review_repost(
    id: i64,
    action: String,
    notes: Option<String>,
) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::repost::models::ReviewAction;
    use pt_reseeder_core::repost::review;

    let action = match action.as_str() {
        "approve" | "approved" => ReviewAction::Approve,
        "reject" | "rejected" => ReviewAction::Reject,
        other => {
            return Err(ServerFnError::new(format!(
                "unknown review action: {other}"
            )))
        }
    };
    let repo = Repository::new(server_pool()?);
    review::review_entry(&repo, id, &action, notes.as_deref())
        .await
        .map(|_| ())
        .map_err(Into::into)
}

#[server]
pub async fn submit_repost(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::repost::submitter;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let registry = context.site_registry.read().await.clone();
    submitter::submit_entry(&repo, registry.as_ref(), id)
        .await
        .map(|_| ())
        .map_err(Into::into)
}

#[server]
pub async fn delete_repost(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_repost_entry(id)
        .await
        .map_err(Into::into)
}
