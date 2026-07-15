use std::collections::HashSet;

use crate::db::repo::Repository;
use crate::error::CoreError;
use crate::site::models::SiteId;

/// Three-layer filter logic for the reseed engine.
///
/// Filter 1: Tracker pre-filter — if any cached variant of this pieces_hash
///   has an announce URL matching the target site, skip (already seeding there).
///
/// Filter 2: History pre-filter — if reseed_history shows a successful entry
///   for this pieces_hash + target site, skip.
///
/// Filter 3: Info-hash dedup at add time — if the destination downloader
///   already has this info_hash, skip and cache it.

/// Filter 1: Check if any cached torrent with this pieces_hash already has
/// an announce URL belonging to the target site.
pub fn filter_by_tracker(cached_announce_urls: &HashSet<String>, site_base_url: &str) -> bool {
    // Extract domain from site_base_url for matching
    let site_domain = extract_domain(site_base_url);
    for url in cached_announce_urls {
        let announce_domain = extract_domain(url);
        if announce_domain == site_domain {
            return true; // Already tracked on this site
        }
    }
    false
}

/// Filter 2: Check reseed_history for a prior successful add for this
/// pieces_hash + site_id, but only skip if that successful info_hash is still
/// present in the current source scan or destination downloader.
pub async fn filter_by_history(
    repo: &Repository,
    pieces_hash: &str,
    site_id: SiteId,
    present_info_hashes: &HashSet<String>,
) -> Result<bool, CoreError> {
    let history = repo.find_reseed_history(pieces_hash, site_id.0).await?;
    Ok(history.iter().any(|h| {
        h.status == "success"
            && h.info_hash
                .as_deref()
                .is_some_and(|info_hash| present_info_hashes.contains(info_hash))
    }))
}

/// Filter 3: Check if the destination downloader already has this info_hash.
pub fn filter_by_existing_hash(dest_hashes: &HashSet<String>, info_hash: &str) -> bool {
    dest_hashes.contains(info_hash)
}

/// Extract the domain portion from a URL for comparison.
pub fn extract_domain(url: &str) -> String {
    // Strip protocol
    let without_proto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    // Take everything before the first '/' or ':'
    without_proto
        .split(|c| c == '/' || c == ':')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://hdsky.me/announce.php"), "hdsky.me");
        assert_eq!(
            extract_domain("http://tracker.mteam.cc:8080/announce"),
            "tracker.mteam.cc"
        );
        assert_eq!(extract_domain("hdsky.me"), "hdsky.me");
    }

    #[test]
    fn test_filter_by_tracker() {
        let mut urls = HashSet::new();
        urls.insert("https://hdsky.me/announce.php?passkey=abc".to_string());
        urls.insert("https://tracker.mteam.cc/announce".to_string());

        assert!(filter_by_tracker(&urls, "https://hdsky.me"));
        assert!(filter_by_tracker(&urls, "https://tracker.mteam.cc/browse"));
        assert!(!filter_by_tracker(&urls, "https://ourbits.club"));
    }

    #[test]
    fn test_filter_by_existing_hash() {
        let mut hashes = HashSet::new();
        hashes.insert("abc123".to_string());
        assert!(filter_by_existing_hash(&hashes, "abc123"));
        assert!(!filter_by_existing_hash(&hashes, "def456"));
    }

    #[test]
    fn test_extract_domain_with_port() {
        assert_eq!(
            extract_domain("http://tracker.example.com:8080/announce"),
            "tracker.example.com"
        );
    }

    #[test]
    fn test_extract_domain_no_protocol() {
        assert_eq!(
            extract_domain("tracker.example.com/announce"),
            "tracker.example.com"
        );
    }

    #[test]
    fn test_extract_domain_empty_string() {
        assert_eq!(extract_domain(""), "");
    }

    #[test]
    fn test_extract_domain_lowercases_output() {
        // extract_domain lowercases the result; protocol prefix stripping is case-sensitive
        assert_eq!(extract_domain("https://HDSky.ME/announce"), "hdsky.me");
    }

    #[test]
    fn test_filter_by_tracker_empty_urls() {
        let urls: HashSet<String> = HashSet::new();
        assert!(!filter_by_tracker(&urls, "https://hdsky.me"));
    }

    #[test]
    fn test_filter_by_tracker_matches_when_lowercase_protocol() {
        let mut urls = HashSet::new();
        urls.insert("https://HDSKY.me/announce".to_string());
        assert!(filter_by_tracker(&urls, "https://hdsky.me"));
    }

    #[test]
    fn test_filter_by_existing_hash_empty_set() {
        let hashes: HashSet<String> = HashSet::new();
        assert!(!filter_by_existing_hash(&hashes, "abc123"));
    }

    #[test]
    fn test_filter_by_existing_hash_case_sensitive() {
        let mut hashes = HashSet::new();
        hashes.insert("ABC123".to_string());
        assert!(!filter_by_existing_hash(&hashes, "abc123"));
    }
}
