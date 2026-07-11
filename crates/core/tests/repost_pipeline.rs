//! Integration tests for the repost (转种) four-stage pipeline:
//!   Extract → Adapt → Review → Submit

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use pt_reseeder_core::db::init_db;
use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::error::CoreError;
use pt_reseeder_core::repost::adapter::adapt_torrent_info;
use pt_reseeder_core::repost::extractor::extract_torrent_info;
use pt_reseeder_core::repost::models::{
    AdapterMapping, CategoryMapping, CodecMapping, ResolutionMapping, ReviewAction, SourceMapping,
};
use pt_reseeder_core::repost::review::{retry_entry, review_entry};
use pt_reseeder_core::repost::submitter::{submit_entry, submit_torrent};
use pt_reseeder_core::site::models::{AdaptedTorrentInfo, RawTorrentInfo, SiteId};
use pt_reseeder_core::site::rate_limiter::SiteRateLimiter;
use pt_reseeder_core::site::registry::{AdapterHandle, SiteRegistry};
use pt_reseeder_core::site::traits::{RepostCapable, SiteCapability, SiteCore};

// ---------------------------------------------------------------------------
// Mock site adapter
// ---------------------------------------------------------------------------

/// A mock site that implements both SiteCore and RepostCapable for testing.
struct MockSite {
    name: String,
    /// The RawTorrentInfo returned by extract_torrent_detail.
    raw_info: RawTorrentInfo,
    /// The torrent ID string returned by submit_torrent on success.
    submit_result: String,
    submitted: Arc<Mutex<Vec<AdaptedTorrentInfo>>>,
}

impl SiteCore for MockSite {
    fn name(&self) -> &str {
        &self.name
    }
    fn base_url(&self) -> &str {
        "https://mock.example.com"
    }
    fn capabilities(&self) -> HashSet<SiteCapability> {
        let mut caps = HashSet::new();
        caps.insert(SiteCapability::Repost);
        caps
    }
}

#[async_trait]
impl RepostCapable for MockSite {
    async fn extract_torrent_detail(&self, _torrent_id: &str) -> Result<RawTorrentInfo, CoreError> {
        Ok(self.raw_info.clone())
    }
    async fn submit_torrent(&self, info: &AdaptedTorrentInfo) -> Result<String, CoreError> {
        self.submitted.lock().unwrap().push(info.clone());
        Ok(self.submit_result.clone())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a sample RawTorrentInfo for test purposes.
fn sample_raw_info() -> RawTorrentInfo {
    RawTorrentInfo {
        name: "Movie.2024.1080p.BluRay.x265 @HDSky".to_string(),
        small_descr: "A great movie in 1080p".to_string(),
        descr: "[quote=user]review[/quote] [img=800,600]https://img.example.com/pic.jpg[/img] [hide]secret[/hide]".to_string(),
        imdb_url: Some("https://www.imdb.com/title/tt1234567".to_string()),
        douban_url: Some("https://movie.douban.com/subject/1234567".to_string()),
        mediainfo: Some("General\nFormat: Matroska".to_string()),
        images: vec!["https://img.example.com/screenshot1.jpg".to_string()],
        torrent_type: "movie".to_string(),
        region: "US".to_string(),
        resolution: "1080p".to_string(),
        video_codec: "H.265".to_string(),
        audio_codec: "DTS-HD".to_string(),
        medium: "Blu-ray".to_string(),
        source_site: "hdsky".to_string(),
        source_url: "https://hdsky.me/details.php?id=12345".to_string(),
        torrent_file_data: None,
    }
}

fn sample_adapter_mapping() -> AdapterMapping {
    AdapterMapping {
        site_name: "target_site".to_string(),
        categories: vec![CategoryMapping {
            torrent_type: "movie".to_string(),
            aliases: vec!["movies".to_string(), "电影".to_string()],
            category_id: 401,
        }],
        codecs: vec![CodecMapping {
            codec: "H.265".to_string(),
            aliases: vec!["HEVC".to_string(), "x265".to_string()],
            codec_id: 2,
        }],
        resolutions: vec![ResolutionMapping {
            resolution: "1080p".to_string(),
            aliases: vec![],
            resolution_id: 2,
        }],
        sources: vec![SourceMapping {
            medium: "Blu-ray".to_string(),
            aliases: vec!["BluRay".to_string()],
            source_id: 1,
        }],
        description_template: None,
    }
}

/// Create a SiteRegistry with a mock source and target site.
fn build_registry(raw_info: RawTorrentInfo) -> (SiteRegistry, Arc<Mutex<Vec<AdaptedTorrentInfo>>>) {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let source_site: Arc<MockSite> = Arc::new(MockSite {
        name: "MockSource".to_string(),
        raw_info,
        submit_result: String::new(),
        submitted: Arc::new(Mutex::new(Vec::new())),
    });
    let target_site: Arc<MockSite> = Arc::new(MockSite {
        name: "MockTarget".to_string(),
        raw_info: sample_raw_info(), // not used for target
        submit_result: "new-torrent-99".to_string(),
        submitted: Arc::clone(&submitted),
    });

    let mut registry = SiteRegistry::new();

    // Source site (id = 1)
    registry.register(
        SiteId(1),
        AdapterHandle {
            core: source_site.clone() as Arc<dyn SiteCore>,
            reseed: None,
            repost: Some(source_site as Arc<dyn RepostCapable>),
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    // Target site (id = 2)
    registry.register(
        SiteId(2),
        AdapterHandle {
            core: target_site.clone() as Arc<dyn SiteCore>,
            reseed: None,
            repost: Some(target_site as Arc<dyn RepostCapable>),
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    (registry, submitted)
}

/// Set up an in-memory SQLite database with all migrations applied.
async fn setup_repo() -> Repository {
    let pool = init_db("sqlite::memory:").await.unwrap();
    let repo = Repository::new(pool);
    // Create placeholder site rows so foreign keys are satisfied.
    repo.create_site(
        "MockSource",
        "https://source.example.com",
        None,
        "mock",
        "cookie",
    )
    .await
    .unwrap();
    repo.create_site(
        "MockTarget",
        "https://target.example.com",
        None,
        "mock",
        "cookie",
    )
    .await
    .unwrap();
    repo
}

// ===========================================================================
// Test 1: Extractor gets raw info from source site via RepostCapable
// ===========================================================================

#[tokio::test]
async fn test_extractor_gets_raw_info() {
    let raw = sample_raw_info();
    let (registry, _) = build_registry(raw.clone());

    let extracted = extract_torrent_info(&registry, SiteId(1), "12345")
        .await
        .unwrap();

    assert_eq!(extracted.name, raw.name);
    assert_eq!(extracted.torrent_type, "movie");
    assert_eq!(extracted.resolution, "1080p");
    assert_eq!(extracted.video_codec, "H.265");
    assert_eq!(extracted.medium, "Blu-ray");
    assert_eq!(extracted.source_site, "hdsky");
    assert_eq!(
        extracted.imdb_url,
        Some("https://www.imdb.com/title/tt1234567".to_string())
    );
    assert_eq!(extracted.images.len(), 1);
}

// ===========================================================================
// Test 2: Adapter transforms metadata (title cleaning, BBCode, field mapping)
// ===========================================================================

#[tokio::test]
async fn test_adapter_transforms_metadata() {
    let raw = sample_raw_info();
    let mapping = sample_adapter_mapping();

    let adapted = adapt_torrent_info(&raw, "ourbits", Some(&mapping)).unwrap();

    // Title: watermark "@HDSky" should be stripped
    assert_eq!(adapted.name, "Movie.2024.1080p.BluRay.x265");
    assert!(!adapted.name.contains("@HDSky"));

    // BBCode: [quote=user] -> [quote], [img=800,600] -> [img], [hide]...[/hide] removed
    assert!(!adapted.descr.contains("[quote=user]"));
    assert!(adapted.descr.contains("[quote]"));
    assert!(!adapted.descr.contains("[img=800,600]"));
    assert!(adapted.descr.contains("[img]"));
    assert!(!adapted.descr.contains("[hide]"));
    assert!(!adapted.descr.contains("[/hide]"));
    // Content inside [hide] should be kept
    assert!(adapted.descr.contains("secret"));

    // Field IDs should be mapped
    assert_eq!(adapted.category_id, Some(401));
    assert_eq!(adapted.codec_id, Some(2));
    assert_eq!(adapted.resolution_id, Some(2));
    assert_eq!(adapted.source_id, Some(1));

    // Passthrough fields
    assert_eq!(adapted.small_descr, raw.small_descr);
    assert_eq!(adapted.imdb_url, raw.imdb_url);
    assert_eq!(adapted.douban_url, raw.douban_url);
    assert_eq!(adapted.mediainfo, raw.mediainfo);
    assert_eq!(adapted.images, raw.images);
    assert_eq!(adapted.target_site, "ourbits");
}

#[tokio::test]
async fn test_adapter_mediainfo_to_code_for_unsupported_site() {
    let mut raw = sample_raw_info();
    raw.descr = "[mediainfo]codec info[/mediainfo]".to_string();

    let adapted = adapt_torrent_info(&raw, "chdbits", None).unwrap();
    assert!(adapted.descr.contains("[code]"));
    assert!(adapted.descr.contains("[/code]"));
    assert!(!adapted.descr.contains("[mediainfo]"));
}

#[tokio::test]
async fn test_adapter_mediainfo_kept_for_supporting_site() {
    let mut raw = sample_raw_info();
    raw.descr = "[mediainfo]codec info[/mediainfo]".to_string();

    let adapted = adapt_torrent_info(&raw, "mteam", None).unwrap();
    assert!(adapted.descr.contains("[mediainfo]"));
    assert!(adapted.descr.contains("[/mediainfo]"));
}

#[tokio::test]
async fn test_adapter_default_mappings_without_custom_mapping() {
    let raw = sample_raw_info();

    let adapted = adapt_torrent_info(&raw, "mteam", None).unwrap();
    // Default mappings should still resolve common types
    assert_eq!(adapted.category_id, Some(401)); // movie -> 401
    assert_eq!(adapted.codec_id, Some(2)); // H.265 -> 2
    assert_eq!(adapted.resolution_id, Some(2)); // 1080p -> 2
    assert_eq!(adapted.source_id, Some(1)); // Blu-ray -> 1
}

// ===========================================================================
// Test 3: Review state machine (DB-backed)
// ===========================================================================

#[tokio::test]
async fn test_review_state_machine() {
    let repo = setup_repo().await;
    let raw = sample_raw_info();
    let raw_json = serde_json::to_string(&raw).unwrap();

    // --- Pending -> Approved ---
    let id1 = repo
        .create_repost_entry(1, "t100", 2, &raw_json)
        .await
        .unwrap();
    let entry = review_entry(&repo, id1, &ReviewAction::Approve, Some("looks good"))
        .await
        .unwrap();
    assert_eq!(entry.status, "approved");
    assert_eq!(entry.review_notes, Some("looks good".to_string()));

    // --- Pending -> Rejected ---
    let id2 = repo
        .create_repost_entry(1, "t200", 2, &raw_json)
        .await
        .unwrap();
    let entry = review_entry(&repo, id2, &ReviewAction::Reject, Some("bad quality"))
        .await
        .unwrap();
    assert_eq!(entry.status, "rejected");

    // --- Rejected is terminal: cannot approve a rejected entry ---
    let result = review_entry(&repo, id2, &ReviewAction::Approve, None).await;
    assert!(result.is_err());

    // --- Approved cannot be re-approved (no self-transition) ---
    let result = review_entry(&repo, id1, &ReviewAction::Approve, None).await;
    assert!(result.is_err());

    // --- Simulate Failed -> Approved (retry) ---
    // First, manually set a pending entry to approved, then to failed
    let id3 = repo
        .create_repost_entry(1, "t300", 2, &raw_json)
        .await
        .unwrap();
    review_entry(&repo, id3, &ReviewAction::Approve, None)
        .await
        .unwrap();
    // Manually set to failed (simulating a submission failure)
    repo.update_repost_status(id3, "failed", Some("timeout"), None, None)
        .await
        .unwrap();
    // Retry: Failed -> Approved
    let retried = retry_entry(&repo, id3, Some("retrying")).await.unwrap();
    assert_eq!(retried.status, "approved");

    // --- Cannot retry a rejected entry (terminal state) ---
    let id4 = repo
        .create_repost_entry(1, "t400", 2, &raw_json)
        .await
        .unwrap();
    review_entry(&repo, id4, &ReviewAction::Reject, None)
        .await
        .unwrap();
    let result = retry_entry(&repo, id4, None).await; // rejected is terminal
    assert!(result.is_err());
}

// ===========================================================================
// Test 4: Submitter sends to target site via RepostCapable
// ===========================================================================

#[tokio::test]
async fn test_submitter_sends_to_target() {
    let raw = sample_raw_info();
    let (registry, _) = build_registry(raw.clone());

    let adapted = adapt_torrent_info(&raw, "MockTarget", None).unwrap();

    let result_id = submit_torrent(&registry, SiteId(2), &adapted)
        .await
        .unwrap();
    assert_eq!(result_id, "new-torrent-99");
}

#[tokio::test]
async fn test_submitter_fails_for_unknown_site() {
    let (registry, _) = build_registry(sample_raw_info());
    let adapted = adapt_torrent_info(&sample_raw_info(), "unknown", None).unwrap();

    let result = submit_torrent(&registry, SiteId(999), &adapted).await;
    assert!(result.is_err());
}

// ===========================================================================
// Test 5: Full repost pipeline end-to-end
// ===========================================================================

#[tokio::test]
async fn test_full_repost_pipeline_e2e() {
    let repo = setup_repo().await;
    let raw = sample_raw_info();
    let (registry, submitted_calls) = build_registry(raw.clone());

    // ---- Stage 1: Extract from source site ----
    let extracted = extract_torrent_info(&registry, SiteId(1), "12345")
        .await
        .unwrap();
    assert_eq!(extracted.name, raw.name);

    // ---- Stage 2: Adapt for target site ----
    let mapping = sample_adapter_mapping();
    let adapted = adapt_torrent_info(&extracted, "MockTarget", Some(&mapping)).unwrap();
    assert!(!adapted.name.contains("@HDSky"));
    assert_eq!(adapted.category_id, Some(401));

    // ---- Stage 3: Enqueue and review ----
    let raw_json = serde_json::to_string(&extracted).unwrap();
    let adapted_json = serde_json::to_string(&adapted).unwrap();

    let entry_id = repo
        .create_repost_entry(1, "12345", 2, &raw_json)
        .await
        .unwrap();

    // Verify it starts as pending
    let entry = repo.get_repost_entry(entry_id).await.unwrap().unwrap();
    assert_eq!(entry.status, "pending");

    // Approve the entry and store adapted info
    let approved = review_entry(&repo, entry_id, &ReviewAction::Approve, Some("LGTM"))
        .await
        .unwrap();
    assert_eq!(approved.status, "approved");

    // Store the adapted_info_json (needed for submit_entry)
    repo.update_repost_status(
        entry_id,
        "approved",
        Some("LGTM"),
        Some(&adapted_json),
        None,
    )
    .await
    .unwrap();

    // ---- Stage 4: Simulated submit to the in-memory mock target ----
    let submitted_entry = submit_entry(&repo, &registry, entry_id).await.unwrap();
    assert_eq!(submitted_entry.status, "submitted");
    assert!(submitted_entry.submitted_at.is_some());

    let calls = submitted_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, adapted.name);
    assert_eq!(calls[0].descr, adapted.descr);
    assert_eq!(calls[0].category_id, adapted.category_id);
    assert_eq!(calls[0].target_site, adapted.target_site);

    // Verify the final state in the database
    let final_entry = repo.get_repost_entry(entry_id).await.unwrap().unwrap();
    assert_eq!(final_entry.status, "submitted");
    assert!(final_entry.submitted_at.is_some());
}

#[tokio::test]
async fn test_submit_entry_rejects_non_approved() {
    let repo = setup_repo().await;
    let (registry, _) = build_registry(sample_raw_info());
    let raw_json = serde_json::to_string(&sample_raw_info()).unwrap();

    let entry_id = repo
        .create_repost_entry(1, "999", 2, &raw_json)
        .await
        .unwrap();

    // Entry is pending, submit should fail
    let result = submit_entry(&repo, &registry, entry_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_entries_with_status_filter() {
    let repo = setup_repo().await;
    let raw_json = serde_json::to_string(&sample_raw_info()).unwrap();

    // Create entries with different statuses
    let id1 = repo
        .create_repost_entry(1, "a", 2, &raw_json)
        .await
        .unwrap();
    let id2 = repo
        .create_repost_entry(1, "b", 2, &raw_json)
        .await
        .unwrap();
    let _id3 = repo
        .create_repost_entry(1, "c", 2, &raw_json)
        .await
        .unwrap();

    review_entry(&repo, id1, &ReviewAction::Approve, None)
        .await
        .unwrap();
    review_entry(&repo, id2, &ReviewAction::Reject, None)
        .await
        .unwrap();

    let approved = repo.list_repost_entries(Some("approved")).await.unwrap();
    assert_eq!(approved.len(), 1);
    assert_eq!(approved[0].source_torrent_id, "a");

    let rejected = repo.list_repost_entries(Some("rejected")).await.unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].source_torrent_id, "b");

    let pending = repo.list_repost_entries(Some("pending")).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].source_torrent_id, "c");

    let all = repo.list_repost_entries(None).await.unwrap();
    assert_eq!(all.len(), 3);
}
