use tracing::info;

use crate::db::models::RepostQueueEntry;
use crate::db::repo::Repository;
use crate::error::{CoreError, RepostError};

use super::models::{RepostStatus, ReviewAction};

/// Review a pending repost queue entry: approve or reject it.
pub async fn review_entry(
    repo: &Repository,
    entry_id: i64,
    action: &ReviewAction,
    notes: Option<&str>,
) -> Result<RepostQueueEntry, CoreError> {
    let entry = repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found",
            entry_id
        )))
    })?;

    let current = RepostStatus::from_str(&entry.status).ok_or_else(|| {
        CoreError::Repost(RepostError::InvalidState(format!(
            "unknown status '{}'",
            entry.status
        )))
    })?;

    let target = match action {
        ReviewAction::Approve => RepostStatus::Approved,
        ReviewAction::Reject => RepostStatus::Rejected,
    };

    if !current.can_transition_to(&target) {
        return Err(CoreError::Repost(RepostError::InvalidState(format!(
            "cannot transition from '{}' to '{}'",
            current.as_str(),
            target.as_str()
        ))));
    }

    repo.update_repost_status(
        entry_id,
        target.as_str(),
        notes,
        entry.adapted_info_json.as_deref(),
        None,
    )
    .await?;

    info!(
        entry_id = entry_id,
        action = target.as_str(),
        "repost entry reviewed"
    );

    let updated = repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found after update",
            entry_id
        )))
    })?;

    Ok(updated)
}

/// Retry a failed repost queue entry by moving it back to approved.
pub async fn retry_entry(
    repo: &Repository,
    entry_id: i64,
    notes: Option<&str>,
) -> Result<RepostQueueEntry, CoreError> {
    let entry = repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found",
            entry_id
        )))
    })?;

    let current = RepostStatus::from_str(&entry.status).ok_or_else(|| {
        CoreError::Repost(RepostError::InvalidState(format!(
            "unknown status '{}'",
            entry.status
        )))
    })?;

    if !current.can_transition_to(&RepostStatus::Approved) {
        return Err(CoreError::Repost(RepostError::InvalidState(format!(
            "cannot transition from '{}' to '{}'",
            current.as_str(),
            RepostStatus::Approved.as_str()
        ))));
    }

    repo.update_repost_status(
        entry_id,
        RepostStatus::Approved.as_str(),
        notes.or(entry.review_notes.as_deref()),
        entry.adapted_info_json.as_deref(),
        None,
    )
    .await?;

    info!(entry_id = entry_id, "repost entry queued for retry");

    repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found after update",
            entry_id
        )))
    })
}

/// List repost queue entries, optionally filtered by status.
pub async fn list_entries(
    repo: &Repository,
    status_filter: Option<&str>,
) -> Result<Vec<RepostQueueEntry>, CoreError> {
    // Validate the filter if provided
    if let Some(status) = status_filter {
        if RepostStatus::from_str(status).is_none() {
            return Err(CoreError::Repost(RepostError::InvalidState(format!(
                "unknown status filter '{}'",
                status
            ))));
        }
    }

    repo.list_repost_entries(status_filter).await
}

/// Get a single repost queue entry by ID.
pub async fn get_entry(repo: &Repository, entry_id: i64) -> Result<RepostQueueEntry, CoreError> {
    repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found",
            entry_id
        )))
    })
}
