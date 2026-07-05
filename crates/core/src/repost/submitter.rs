use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::models::RepostQueueEntry;
use crate::db::repo::Repository;
use crate::error::{CoreError, RepostError};
use crate::site::models::{AdaptedTorrentInfo, SiteId};
use crate::site::registry::SiteRegistry;

use super::models::RepostStatus;

/// Criteria for submitting approved repost queue entries in a batch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmitBatchCriteria {
    pub entry_ids: Option<Vec<i64>>,
    pub target_site_ids: Option<Vec<i64>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitBatchEntryResult {
    pub entry_id: i64,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmitBatchResult {
    pub candidate_count: i64,
    pub submitted_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub entries: Vec<SubmitBatchEntryResult>,
}

/// Submit an adapted torrent to the target site using its RepostCapable adapter.
///
/// Returns the new torrent ID/URL from the target site on success.
pub async fn submit_torrent(
    registry: &SiteRegistry,
    target_site_id: SiteId,
    adapted: &AdaptedTorrentInfo,
) -> Result<String, CoreError> {
    let handle = registry.get(&target_site_id).ok_or_else(|| {
        CoreError::Repost(RepostError::SiteNotCapable(format!(
            "target site {} not found in registry",
            target_site_id.0
        )))
    })?;

    let repost_adapter = handle.repost.as_ref().ok_or_else(|| {
        CoreError::Repost(RepostError::SiteNotCapable(format!(
            "site {} does not support repost submission",
            handle.core.name()
        )))
    })?;

    // Respect rate limiter before submitting
    handle.rate_limiter.acquire().await?;

    info!(
        site = handle.core.name(),
        target_site = adapted.target_site,
        "submitting adapted torrent to target site"
    );

    let result_id = repost_adapter.submit_torrent(adapted).await.map_err(|e| {
        CoreError::Repost(RepostError::SubmissionFailed(format!(
            "failed to submit to site {}: {}",
            handle.core.name(),
            e
        )))
    })?;

    info!(
        site = handle.core.name(),
        result_id = result_id,
        "torrent submitted successfully"
    );

    Ok(result_id)
}

/// Submit a single approved repost queue entry and persist the state transition.
pub async fn submit_entry(
    repo: &Repository,
    registry: &SiteRegistry,
    entry_id: i64,
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

    if current != RepostStatus::Approved {
        return Err(CoreError::Repost(RepostError::InvalidState(format!(
            "entry must be approved before submission; current status: '{}'",
            entry.status
        ))));
    }

    let adapted_json = entry.adapted_info_json.as_deref().ok_or_else(|| {
        CoreError::Repost(RepostError::InvalidState(
            "entry has no adapted info; approve it first".to_string(),
        ))
    })?;

    let adapted: AdaptedTorrentInfo = serde_json::from_str(adapted_json).map_err(|e| {
        CoreError::Repost(RepostError::SubmissionFailed(format!(
            "failed to parse adapted info: {}",
            e
        )))
    })?;

    let target_site_id = SiteId::from(entry.target_site_id);
    match submit_torrent(registry, target_site_id, &adapted).await {
        Ok(result_id) => {
            let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            repo.update_repost_status(
                entry_id,
                RepostStatus::Submitted.as_str(),
                entry.review_notes.as_deref(),
                entry.adapted_info_json.as_deref(),
                Some(&now),
            )
            .await?;
            info!(entry_id, %result_id, "repost entry submitted successfully");
        }
        Err(e) => {
            let note = format!("submission failed: {}", e);
            let _ = repo
                .update_repost_status(
                    entry_id,
                    RepostStatus::Failed.as_str(),
                    Some(&note),
                    entry.adapted_info_json.as_deref(),
                    None,
                )
                .await;
            return Err(e);
        }
    }

    repo.get_repost_entry(entry_id).await?.ok_or_else(|| {
        CoreError::Repost(RepostError::NotFound(format!(
            "repost entry {} not found after update",
            entry_id
        )))
    })
}

/// Submit approved repost queue entries. A failure in one entry does not stop the rest.
pub async fn submit_batch(
    repo: &Repository,
    registry: &SiteRegistry,
    criteria: SubmitBatchCriteria,
) -> Result<SubmitBatchResult, CoreError> {
    let mut candidates = if let Some(entry_ids) = &criteria.entry_ids {
        let mut entries = Vec::new();
        for entry_id in entry_ids {
            if let Some(entry) = repo.get_repost_entry(*entry_id).await? {
                entries.push(entry);
            }
        }
        entries
    } else {
        repo.list_repost_entries(Some(RepostStatus::Approved.as_str()))
            .await?
    };

    if let Some(target_site_ids) = &criteria.target_site_ids {
        candidates.retain(|entry| target_site_ids.contains(&entry.target_site_id));
    }

    if let Some(limit) = criteria.limit {
        candidates.truncate(limit);
    }

    let mut result = SubmitBatchResult {
        candidate_count: candidates.len() as i64,
        ..Default::default()
    };

    for entry in candidates {
        if entry.status != RepostStatus::Approved.as_str() {
            result.skipped_count += 1;
            result.entries.push(SubmitBatchEntryResult {
                entry_id: entry.id,
                status: "skipped".to_string(),
                message: format!("entry status is '{}'", entry.status),
            });
            continue;
        }

        match submit_entry(repo, registry, entry.id).await {
            Ok(updated) => {
                result.submitted_count += 1;
                result.entries.push(SubmitBatchEntryResult {
                    entry_id: updated.id,
                    status: updated.status,
                    message: "submitted".to_string(),
                });
            }
            Err(e) => {
                result.failed_count += 1;
                warn!(entry_id = entry.id, %e, "failed to submit repost entry");
                result.entries.push(SubmitBatchEntryResult {
                    entry_id: entry.id,
                    status: RepostStatus::Failed.as_str().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    Ok(result)
}
