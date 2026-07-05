use tracing::info;

use crate::error::{CoreError, RepostError};
use crate::site::models::{RawTorrentInfo, SiteId};
use crate::site::registry::SiteRegistry;

/// Extract torrent details from the source site using its RepostCapable adapter.
pub async fn extract_torrent_info(
    registry: &SiteRegistry,
    source_site_id: SiteId,
    torrent_id: &str,
) -> Result<RawTorrentInfo, CoreError> {
    let handle = registry.get(&source_site_id).ok_or_else(|| {
        CoreError::Repost(RepostError::SiteNotCapable(format!(
            "source site {} not found in registry",
            source_site_id.0
        )))
    })?;

    let repost_adapter = handle.repost.as_ref().ok_or_else(|| {
        CoreError::Repost(RepostError::SiteNotCapable(format!(
            "site {} does not support repost extraction",
            handle.core.name()
        )))
    })?;

    // Respect rate limiter before making the request
    handle.rate_limiter.acquire().await?;

    info!(
        site = handle.core.name(),
        torrent_id = torrent_id,
        "extracting torrent detail for repost"
    );

    let raw_info = repost_adapter
        .extract_torrent_detail(torrent_id)
        .await
        .map_err(|e| {
            CoreError::Repost(RepostError::ExtractionFailed(format!(
                "failed to extract from site {}: {}",
                handle.core.name(),
                e
            )))
        })?;

    Ok(raw_info)
}
