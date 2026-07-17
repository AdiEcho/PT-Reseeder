//! Integration tests for the reseed (辅种) three-stage pipeline:
//!   Scan → Match → Add
//!
//! Uses mock implementations of `ReseedCapable` and `Downloader` traits,
//! a real SQLite in-memory database with migrations, and dynamically
//! generated `.torrent` fixture files.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha1::{Digest, Sha1};
use tokio_util::sync::CancellationToken;

use pt_reseeder_core::db::init_db;
use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::db::writer::spawn_writer;
use pt_reseeder_core::downloader::models::{AddTorrentOpts, TorrentInfo};
use pt_reseeder_core::downloader::traits::Downloader;
use pt_reseeder_core::engine::adder::{add_torrent, MatchedTorrent};
use pt_reseeder_core::engine::matcher::match_all_sites;
use pt_reseeder_core::engine::scanner::{scan_folder, ScanResult};
use pt_reseeder_core::engine::stats::ReseedStats;
use pt_reseeder_core::engine::{ReseedConfig, ReseedEngine};
use pt_reseeder_core::error::CoreError;
use pt_reseeder_core::site::models::SiteId;
use pt_reseeder_core::site::rate_limiter::SiteRateLimiter;
use pt_reseeder_core::site::registry::{AdapterHandle, SiteRegistry};
use pt_reseeder_core::site::traits::{ReseedCapable, SiteCapability, SiteCore};

// ---------------------------------------------------------------------------
// Torrent fixture builder
// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Build a minimal valid `.torrent` file (bencoded) with the given name,
/// announce URL, and a deterministic pieces field derived from `seed`.
/// Returns (raw_bytes, info_hash, pieces_hash).
fn build_torrent_bytes(name: &str, announce: &str, seed: u8) -> (Vec<u8>, String, String) {
    let pieces = [seed; 20];
    let pieces_hash = hex(&Sha1::digest(pieces));

    // Build the info dict bencode manually.
    // Keys must be in sorted order: "length", "name", "piece length", "pieces"
    let name_bytes = name.as_bytes();
    let mut info_bytes = Vec::new();
    info_bytes.extend_from_slice(b"d");
    info_bytes.extend_from_slice(format!("6:lengthi1024e").as_bytes());
    info_bytes.extend_from_slice(format!("4:name{}:{}", name_bytes.len(), name).as_bytes());
    info_bytes.extend_from_slice(b"12:piece lengthi16384e");
    info_bytes.extend_from_slice(format!("6:pieces{}:", pieces.len()).as_bytes());
    info_bytes.extend_from_slice(&pieces);
    info_bytes.extend_from_slice(b"e");

    let info_hash = hex(&Sha1::digest(&info_bytes));

    // Build the full torrent dict: "announce" + "info"
    let announce_bytes = announce.as_bytes();
    let mut torrent = Vec::new();
    torrent.extend_from_slice(b"d");
    torrent
        .extend_from_slice(format!("8:announce{}:{}", announce_bytes.len(), announce).as_bytes());
    torrent.extend_from_slice(format!("4:info{}:", info_bytes.len()).as_bytes());
    // Oops — info is a dict, not a byte string. We embed the raw dict bytes directly.
    // Actually bencode dicts are values, so we just concatenate:
    // "4:info" followed by the info dict bytes (which start with 'd' and end with 'e').
    // Let me redo: the outer dict key "info" maps to the info dict VALUE.
    torrent.clear();
    torrent.extend_from_slice(b"d");
    torrent
        .extend_from_slice(format!("8:announce{}:{}", announce_bytes.len(), announce).as_bytes());
    torrent.extend_from_slice(b"4:info");
    torrent.extend_from_slice(&info_bytes);
    torrent.extend_from_slice(b"e");

    (torrent, info_hash, pieces_hash)
}

/// Write torrent bytes to a temp directory and return the directory path.
fn write_torrent_fixtures(
    torrents: &[(&str, &str, u8)],
) -> (tempfile::TempDir, Vec<(String, String)>) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let mut meta = Vec::new();
    for &(name, announce, seed) in torrents {
        let (bytes, info_hash, pieces_hash) = build_torrent_bytes(name, announce, seed);
        let file_path = dir.path().join(format!("{}.torrent", name));
        std::fs::write(&file_path, &bytes).expect("write torrent file");
        meta.push((info_hash, pieces_hash));
    }
    (dir, meta)
}

// ---------------------------------------------------------------------------
// Mock Downloader
// ---------------------------------------------------------------------------

struct MockDownloader {
    existing_hashes: Mutex<HashSet<String>>,
    added_torrents: Mutex<Vec<AddTorrentOpts>>,
    resumed: Mutex<Vec<String>>,
}

impl MockDownloader {
    fn new() -> Self {
        Self {
            existing_hashes: Mutex::new(HashSet::new()),
            added_torrents: Mutex::new(Vec::new()),
            resumed: Mutex::new(Vec::new()),
        }
    }

    fn with_existing(hashes: Vec<String>) -> Self {
        Self {
            existing_hashes: Mutex::new(hashes.into_iter().collect()),
            added_torrents: Mutex::new(Vec::new()),
            resumed: Mutex::new(Vec::new()),
        }
    }

    fn added_count(&self) -> usize {
        self.added_torrents.lock().unwrap().len()
    }

    fn resumed_count(&self) -> usize {
        self.resumed.lock().unwrap().len()
    }
}

#[async_trait]
impl Downloader for MockDownloader {
    async fn connect(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
    async fn test_connection(&self) -> Result<bool, CoreError> {
        Ok(true)
    }
    async fn get_torrent_info(&self, _info_hash: &str) -> Result<Option<TorrentInfo>, CoreError> {
        Ok(None)
    }
    async fn get_all_info_hashes(&self) -> Result<HashSet<String>, CoreError> {
        Ok(self.existing_hashes.lock().unwrap().clone())
    }
    async fn add_torrent(&self, opts: AddTorrentOpts) -> Result<bool, CoreError> {
        self.added_torrents.lock().unwrap().push(opts);
        Ok(true)
    }
    async fn resume_torrent(&self, info_hash: &str) -> Result<bool, CoreError> {
        self.resumed.lock().unwrap().push(info_hash.to_string());
        Ok(true)
    }
    async fn pause_torrent(&self, _info_hash: &str) -> Result<bool, CoreError> {
        Ok(true)
    }
    async fn export_torrent(&self, _info_hash: &str) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(None)
    }
    async fn close(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock ReseedCapable site
// ---------------------------------------------------------------------------

struct MockReseedSite {
    name: String,
    base_url: String,
    /// (pieces_hash, torrent_id) pairs this site "knows about".
    known_matches: Vec<(String, i64)>,
}

impl SiteCore for MockReseedSite {
    fn name(&self) -> &str {
        &self.name
    }
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn capabilities(&self) -> HashSet<SiteCapability> {
        let mut caps = HashSet::new();
        caps.insert(SiteCapability::Reseed);
        caps
    }
}

#[async_trait]
impl ReseedCapable for MockReseedSite {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        let matches: Vec<(String, i64)> = self
            .known_matches
            .iter()
            .filter(|(ph, _)| hashes.contains(ph))
            .cloned()
            .collect();
        Ok(matches)
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        format!("{}/download/{}", self.base_url, torrent_id)
    }

    fn batch_size(&self) -> usize {
        1000
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn setup_db() -> (
    tempfile::TempDir,
    Repository,
    pt_reseeder_core::db::writer::DbWriterHandle,
) {
    let db_dir = tempfile::tempdir().expect("create database temp dir");
    let database_url = format!("sqlite://{}", db_dir.path().join("test.db").display());
    let pool = init_db(&database_url).await.unwrap();
    let repo = Repository::new(pool);
    // Create a placeholder site row so foreign keys are satisfied.
    repo.create_site(
        "MockSite",
        "https://mocksite.example.com",
        None,
        "mock",
        "cookie",
    )
    .await
    .unwrap();
    let db_writer = spawn_writer(&database_url, 100).unwrap();
    (db_dir, repo, db_writer)
}

fn build_registry_with_site(
    site_id: SiteId,
    name: &str,
    base_url: &str,
    known_matches: Vec<(String, i64)>,
) -> SiteRegistry {
    let site = Arc::new(MockReseedSite {
        name: name.to_string(),
        base_url: base_url.to_string(),
        known_matches,
    });

    let mut registry = SiteRegistry::new();
    registry.register(
        site_id,
        AdapterHandle {
            core: site.clone() as Arc<dyn SiteCore>,
            reseed: Some(site as Arc<dyn ReseedCapable>),
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );
    registry
}

// ===========================================================================
// Test 1: Scanner finds and parses torrent files from a folder
// ===========================================================================

#[tokio::test]
async fn test_scanner_finds_torrents() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let dest_client = MockDownloader::new();
    let stats = ReseedStats::new();
    let cancel = CancellationToken::new();

    // Create fixture torrents
    let (dir, meta) = write_torrent_fixtures(&[
        ("movie1", "http://tracker1.example.com/announce", 0xAA),
        ("movie2", "http://tracker2.example.com/announce", 0xBB),
    ]);

    let result = scan_folder(dir.path(), &repo, &db_writer, &dest_client, &stats, &cancel)
        .await
        .unwrap();

    // Verify scan results
    assert_eq!(result.torrents.len(), 2, "should find 2 torrents");
    assert_eq!(
        result.pieces_groups.len(),
        2,
        "should have 2 pieces_hash groups"
    );

    // Verify info_hashes and pieces_hashes match expectations
    for (info_hash, pieces_hash) in &meta {
        assert!(
            result.torrents.contains_key(info_hash),
            "should contain info_hash {}",
            info_hash
        );
        assert!(
            result.pieces_groups.contains_key(pieces_hash),
            "should contain pieces_hash {}",
            pieces_hash
        );
    }

    // Verify stats
    assert_eq!(stats.scanned.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn test_scanner_skips_cached_torrents_on_rescan() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let dest_client = MockDownloader::new();
    let cancel = CancellationToken::new();

    let (dir, _meta) =
        write_torrent_fixtures(&[("cached1", "http://tracker.example.com/announce", 0xCC)]);

    // First scan: all torrents are new
    let stats1 = ReseedStats::new();
    let result1 = scan_folder(
        dir.path(),
        &repo,
        &db_writer,
        &dest_client,
        &stats1,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(result1.torrents.len(), 1);
    // Flush so the pieces_cache entry is written
    db_writer.flush().await.unwrap();

    // Second scan: torrents are already cached
    let stats2 = ReseedStats::new();
    let result2 = scan_folder(
        dir.path(),
        &repo,
        &db_writer,
        &dest_client,
        &stats2,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(result2.torrents.len(), 1, "cached torrents still in result");
    assert_eq!(
        stats2.cached_skip.load(Ordering::Relaxed),
        1,
        "should count 1 cached skip"
    );
}

#[tokio::test]
async fn test_scanner_empty_folder() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let dest_client = MockDownloader::new();
    let stats = ReseedStats::new();
    let cancel = CancellationToken::new();

    let dir = tempfile::tempdir().expect("create temp dir");

    let result = scan_folder(dir.path(), &repo, &db_writer, &dest_client, &stats, &cancel)
        .await
        .unwrap();

    assert!(result.torrents.is_empty());
    assert!(result.pieces_groups.is_empty());
    assert_eq!(stats.scanned.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_scanner_captures_dest_hashes() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let dest_client = MockDownloader::with_existing(vec![
        "existing_hash_1".to_string(),
        "existing_hash_2".to_string(),
    ]);
    let stats = ReseedStats::new();
    let cancel = CancellationToken::new();

    let (dir, _meta) =
        write_torrent_fixtures(&[("t1", "http://tracker.example.com/announce", 0x01)]);

    let result = scan_folder(dir.path(), &repo, &db_writer, &dest_client, &stats, &cancel)
        .await
        .unwrap();

    assert_eq!(result.dest_hashes.len(), 2);
    assert!(result.dest_hashes.contains("existing_hash_1"));
    assert!(result.dest_hashes.contains("existing_hash_2"));
}

// ===========================================================================
// Test 2: Matcher queries site and returns matched torrents
// ===========================================================================

#[tokio::test]
async fn test_matcher_queries_site() {
    let (_db_dir, repo, _db_writer) = setup_db().await;
    let cancel = CancellationToken::new();
    let stats = ReseedStats::new();

    // Build fixture data
    let (_torrent_bytes, info_hash, pieces_hash) =
        build_torrent_bytes("matchme", "http://other-tracker.example.com/announce", 0xDD);

    // Site knows about this pieces_hash with torrent_id=42
    let site_id = SiteId(1);
    let registry = build_registry_with_site(
        site_id,
        "MockSite",
        "https://mocksite.example.com",
        vec![(pieces_hash.clone(), 42)],
    );

    // Build a ScanResult as would come from the scanner
    let mut torrents = std::collections::HashMap::new();
    torrents.insert(
        info_hash.clone(),
        pt_reseeder_core::torrent::models::TorrentMeta {
            info_hash: info_hash.clone(),
            pieces_hash: pieces_hash.clone(),
            name: "matchme".to_string(),
            total_size: 1024,
            files: vec![],
            announce: Some("http://other-tracker.example.com/announce".to_string()),
            announce_list: vec![],
            piece_length: 16384,
            pieces_count: 1,
        },
    );
    let mut pieces_groups = std::collections::HashMap::new();
    pieces_groups.insert(pieces_hash.clone(), vec![info_hash.clone()]);

    let scan = ScanResult {
        torrents,
        pieces_groups,
        dest_hashes: HashSet::new(),
    };

    let matched = match_all_sites(
        &scan,
        &registry,
        &[site_id],
        &repo,
        "/downloads",
        true,
        Some("reseed"),
        &stats,
        &cancel,
    )
    .await
    .unwrap();

    assert_eq!(matched.len(), 1, "should find 1 match");
    assert_eq!(matched[0].pieces_hash, pieces_hash);
    assert_eq!(matched[0].site_id, site_id);
    assert_eq!(matched[0].torrent_id, Some(42));
    assert_eq!(
        matched[0].download_url,
        "https://mocksite.example.com/download/42"
    );
    assert_eq!(matched[0].save_path, "/downloads");
    assert!(matched[0].skip_hash_check);
    assert_eq!(matched[0].tag, Some("reseed".to_string()));
    assert_eq!(stats.matched.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_matcher_no_matches() {
    let (_db_dir, repo, _db_writer) = setup_db().await;
    let cancel = CancellationToken::new();
    let stats = ReseedStats::new();

    let site_id = SiteId(1);
    // Site knows nothing
    let registry =
        build_registry_with_site(site_id, "MockSite", "https://mocksite.example.com", vec![]);

    let (_, _info_hash, pieces_hash) =
        build_torrent_bytes("nomatch", "http://other.example.com/announce", 0xEE);

    let mut torrents = std::collections::HashMap::new();
    torrents.insert(
        _info_hash.clone(),
        pt_reseeder_core::torrent::models::TorrentMeta {
            info_hash: _info_hash.clone(),
            pieces_hash: pieces_hash.clone(),
            name: "nomatch".to_string(),
            total_size: 1024,
            files: vec![],
            announce: Some("http://other.example.com/announce".to_string()),
            announce_list: vec![],
            piece_length: 16384,
            pieces_count: 1,
        },
    );
    let mut pieces_groups = std::collections::HashMap::new();
    pieces_groups.insert(pieces_hash.clone(), vec![_info_hash.clone()]);

    let scan = ScanResult {
        torrents,
        pieces_groups,
        dest_hashes: HashSet::new(),
    };

    let matched = match_all_sites(
        &scan,
        &registry,
        &[site_id],
        &repo,
        "/downloads",
        false,
        None,
        &stats,
        &cancel,
    )
    .await
    .unwrap();

    assert!(matched.is_empty(), "should find no matches");
    assert_eq!(stats.matched.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_matcher_empty_scan() {
    let (_db_dir, repo, _db_writer) = setup_db().await;
    let cancel = CancellationToken::new();
    let stats = ReseedStats::new();

    let site_id = SiteId(1);
    let registry =
        build_registry_with_site(site_id, "MockSite", "https://mocksite.example.com", vec![]);

    let scan = ScanResult {
        torrents: std::collections::HashMap::new(),
        pieces_groups: std::collections::HashMap::new(),
        dest_hashes: HashSet::new(),
    };

    let matched = match_all_sites(
        &scan,
        &registry,
        &[site_id],
        &repo,
        "/downloads",
        false,
        None,
        &stats,
        &cancel,
    )
    .await
    .unwrap();

    assert!(matched.is_empty());
}

// ===========================================================================
// Test 3: Adder downloads torrent and adds to destination downloader
// ===========================================================================

#[tokio::test]
async fn test_adder_downloads_and_adds() {
    let (_db_dir, _repo, db_writer) = setup_db().await;
    let stats = ReseedStats::new();

    // Build a torrent that the "site" will serve
    let (torrent_bytes, info_hash, pieces_hash) =
        build_torrent_bytes("addme", "http://tracker.example.com/announce", 0xFF);

    // Start a wiremock-like HTTP server using a simple approach:
    // Use a local server to serve the torrent bytes
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let download_url = format!("http://127.0.0.1:{}/download/100", addr.port());

    let torrent_bytes_clone = torrent_bytes.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-bittorrent\r\nConnection: close\r\n\r\n",
                    torrent_bytes_clone.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(&torrent_bytes_clone).await;
                let _ = stream.flush().await;
            }
        }
    });

    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let dest_client = MockDownloader::new();
    let dest_hashes = Arc::new(tokio::sync::Mutex::new(HashSet::new()));

    let matched = MatchedTorrent {
        pieces_hash: pieces_hash.clone(),
        site_id: SiteId(1),
        torrent_id: Some(100),
        download_url,
        save_path: "/downloads".to_string(),
        skip_hash_check: true,
        tag: Some("test-tag".to_string()),
    };

    let http_client = reqwest::Client::new();
    let added = add_torrent(
        &matched,
        &http_client,
        &dest_client,
        &dest_hashes,
        true, // auto_start
        &db_writer,
        &stats,
    )
    .await
    .unwrap();

    assert!(added, "torrent should be added successfully");
    assert_eq!(dest_client.added_count(), 1);
    assert_eq!(
        dest_client.resumed_count(),
        1,
        "should resume when auto_start=true"
    );
    assert_eq!(stats.added.load(Ordering::Relaxed), 1);
    assert!(
        dest_hashes.lock().await.contains(&info_hash),
        "info_hash should be added to dest_hashes for dedup"
    );
}

#[tokio::test]
async fn test_adder_skips_existing_hash() {
    let (_db_dir, _repo, db_writer) = setup_db().await;
    let stats = ReseedStats::new();

    let (torrent_bytes, info_hash, pieces_hash) =
        build_torrent_bytes("existing", "http://tracker.example.com/announce", 0xAB);

    // Serve torrent
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let download_url = format!("http://127.0.0.1:{}/download/200", addr.port());

    let torrent_bytes_clone = torrent_bytes.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-bittorrent\r\nConnection: close\r\n\r\n",
                    torrent_bytes_clone.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(&torrent_bytes_clone).await;
                let _ = stream.flush().await;
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let dest_client = MockDownloader::new();
    // Pre-populate dest_hashes with the info_hash — simulates "already exists"
    let mut seed = HashSet::new();
    seed.insert(info_hash.clone());
    let dest_hashes = Arc::new(tokio::sync::Mutex::new(seed));

    let matched = MatchedTorrent {
        pieces_hash,
        site_id: SiteId(1),
        torrent_id: Some(200),
        download_url,
        save_path: "/downloads".to_string(),
        skip_hash_check: false,
        tag: None,
    };

    let http_client = reqwest::Client::new();
    let added = add_torrent(
        &matched,
        &http_client,
        &dest_client,
        &dest_hashes,
        false,
        &db_writer,
        &stats,
    )
    .await
    .unwrap();

    assert!(!added, "should skip existing torrent");
    assert_eq!(dest_client.added_count(), 0);
    assert_eq!(stats.skipped_exists.load(Ordering::Relaxed), 1);
}

// ===========================================================================
// Test 4: Full pipeline end-to-end with mock site + mock downloader + real scanner
// ===========================================================================

#[tokio::test]
async fn test_full_pipeline_e2e() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let cancel = CancellationToken::new();

    // Create fixture torrents
    let (dir, meta) = write_torrent_fixtures(&[
        (
            "e2e_movie1",
            "http://other-tracker.example.com/announce",
            0x11,
        ),
        (
            "e2e_movie2",
            "http://other-tracker.example.com/announce",
            0x22,
        ),
        (
            "e2e_movie3",
            "http://other-tracker.example.com/announce",
            0x33,
        ),
    ]);

    let pieces_hash_1 = meta[0].1.clone();
    let pieces_hash_2 = meta[1].1.clone();
    // movie3 won't be known by the site

    // --- Set up mock site that knows about movie1 and movie2 ---
    let site_id = SiteId(1);
    let site = Arc::new(MockReseedSite {
        name: "TestSite".to_string(),
        base_url: "https://testsite.example.com".to_string(),
        known_matches: vec![(pieces_hash_1.clone(), 1001), (pieces_hash_2.clone(), 1002)],
    });

    let mut registry = SiteRegistry::new();
    registry.register(
        site_id,
        AdapterHandle {
            core: site.clone() as Arc<dyn SiteCore>,
            reseed: Some(site as Arc<dyn ReseedCapable>),
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    // --- Set up HTTP servers for torrent downloads ---
    // We need to serve the torrent bytes at the URLs the site will produce.
    // The mock site produces URLs like: https://testsite.example.com/download/{id}
    // We'll redirect those to local HTTP servers.

    // Build torrent bytes that the "site" will serve when downloading
    let (torrent_bytes_1, _info_hash_1, _) = build_torrent_bytes(
        "e2e_movie1",
        "http://other-tracker.example.com/announce",
        0x11,
    );
    let (torrent_bytes_2, _info_hash_2, _) = build_torrent_bytes(
        "e2e_movie2",
        "http://other-tracker.example.com/announce",
        0x22,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();

    let tb1 = torrent_bytes_1.clone();
    let tb2 = torrent_bytes_2.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                // Read the request to determine which torrent to serve
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);

                let body = if request.contains("/download/1001") {
                    &tb1
                } else {
                    &tb2
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-bittorrent\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(body).await;
                let _ = stream.flush().await;
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // --- Override the site to use local URLs ---
    // We need to re-create the site with URLs pointing to our local server.
    let site_local = Arc::new(MockReseedSiteLocalUrls {
        name: "TestSite".to_string(),
        base_url: format!("http://127.0.0.1:{}", port),
        known_matches: vec![(pieces_hash_1.clone(), 1001), (pieces_hash_2.clone(), 1002)],
    });

    let mut registry = SiteRegistry::new();
    registry.register(
        site_id,
        AdapterHandle {
            core: site_local.clone() as Arc<dyn SiteCore>,
            reseed: Some(site_local as Arc<dyn ReseedCapable>),
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    // --- Build the mock destination downloader ---
    let dest_client = Arc::new(MockDownloader::new());

    // --- Run the full pipeline ---
    let config = ReseedConfig {
        scan_folders: vec![dir.path().to_path_buf()],
        source_downloaders: vec![],
        target_site_ids: vec![site_id],
        default_save_path: "/downloads".to_string(),
        skip_hash_check: true,
        auto_start: true,
        tag: Some("e2e-test".to_string()),
        jackett_config: None,
    };

    let engine = ReseedEngine::new(Arc::new(registry), repo.clone(), db_writer.clone(), cancel);

    let (snapshot, preview) = engine
        .run_sync(config, dest_client.clone(), false)
        .await
        .unwrap();
    assert!(preview.is_none());

    // --- Verify results ---
    // 3 torrents scanned
    assert_eq!(snapshot.scanned, 3, "should scan 3 torrents");
    // 2 matched (movie1 and movie2)
    assert_eq!(snapshot.matched, 2, "should match 2 torrents");
    // 2 added to destination
    assert_eq!(snapshot.added, 2, "should add 2 torrents");
    // movie3 was not matched, so no failure
    assert_eq!(snapshot.failed, 0, "no failures expected");

    // Verify the mock downloader received 2 torrents
    assert_eq!(dest_client.added_count(), 2);
    // With auto_start=true, both should be resumed
    assert_eq!(dest_client.resumed_count(), 2);
}

/// Dry-run runs scan+match but never adds to destination or writes reseed history.
#[tokio::test]
async fn test_pipeline_dry_run_skips_add_and_history() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let cancel = CancellationToken::new();

    let (dir, metas) = write_torrent_fixtures(&[
        ("movie1", "http://tracker.example.com/announce", 0x11),
        ("movie2", "http://tracker.example.com/announce", 0x22),
        ("movie3", "http://tracker.example.com/announce", 0x33),
    ]);
    let pieces_hash_1 = metas[0].1.clone();
    let pieces_hash_2 = metas[1].1.clone();

    let site_id = SiteId(1);
    let site = Arc::new(MockReseedSite {
        name: "TestSite".to_string(),
        base_url: "https://example.test".to_string(),
        known_matches: vec![(pieces_hash_1.clone(), 1001), (pieces_hash_2.clone(), 1002)],
    });

    let mut registry = SiteRegistry::new();
    registry.register(
        site_id,
        AdapterHandle {
            core: site.clone() as Arc<dyn SiteCore>,
            reseed: Some(site as Arc<dyn ReseedCapable>),
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    let dest_client = Arc::new(MockDownloader::new());
    let config = ReseedConfig {
        scan_folders: vec![dir.path().to_path_buf()],
        source_downloaders: vec![],
        target_site_ids: vec![site_id],
        default_save_path: "/downloads".to_string(),
        skip_hash_check: true,
        auto_start: true,
        tag: Some("dry-run-test".to_string()),
        jackett_config: None,
    };

    let engine = ReseedEngine::new(Arc::new(registry), repo.clone(), db_writer.clone(), cancel);
    let (snapshot, preview) = engine
        .run_sync(config, dest_client.clone(), true)
        .await
        .unwrap();

    assert_eq!(snapshot.scanned, 3, "should still scan torrents");
    assert_eq!(snapshot.matched, 2, "should still match torrents");
    assert_eq!(snapshot.added, 0, "dry-run must not add");
    assert_eq!(dest_client.added_count(), 0);
    assert_eq!(dest_client.resumed_count(), 0);

    let preview = preview.expect("dry-run should return preview");
    assert_eq!(preview.would_add_count, 2);
    assert_eq!(preview.items.len(), 2);
    assert!(preview
        .items
        .iter()
        .any(|item| item.pieces_hash == pieces_hash_1));
    assert!(preview
        .items
        .iter()
        .any(|item| item.pieces_hash == pieces_hash_2));

    // No reseed history rows should be written under dry-run.
    let h1 = repo
        .find_reseed_history(&pieces_hash_1, site_id.0)
        .await
        .unwrap();
    let h2 = repo
        .find_reseed_history(&pieces_hash_2, site_id.0)
        .await
        .unwrap();
    assert!(h1.is_empty(), "dry-run must not write reseed_history");
    assert!(h2.is_empty(), "dry-run must not write reseed_history");
}

/// Same as MockReseedSite but uses local HTTP URLs for downloading.
struct MockReseedSiteLocalUrls {
    name: String,
    base_url: String,
    known_matches: Vec<(String, i64)>,
}

impl SiteCore for MockReseedSiteLocalUrls {
    fn name(&self) -> &str {
        &self.name
    }
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn capabilities(&self) -> HashSet<SiteCapability> {
        let mut caps = HashSet::new();
        caps.insert(SiteCapability::Reseed);
        caps
    }
}

#[async_trait]
impl ReseedCapable for MockReseedSiteLocalUrls {
    async fn query_pieces_hash(&self, hashes: &[String]) -> Result<Vec<(String, i64)>, CoreError> {
        let matches: Vec<(String, i64)> = self
            .known_matches
            .iter()
            .filter(|(ph, _)| hashes.contains(ph))
            .cloned()
            .collect();
        Ok(matches)
    }

    fn build_download_url(&self, torrent_id: i64) -> String {
        format!("{}/download/{}", self.base_url, torrent_id)
    }

    fn batch_size(&self) -> usize {
        1000
    }
}

// ===========================================================================
// Test 5: Pipeline with no matches completes gracefully
// ===========================================================================

#[tokio::test]
async fn test_pipeline_no_matches_completes() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let cancel = CancellationToken::new();

    let (dir, _) =
        write_torrent_fixtures(&[("lonely", "http://tracker.example.com/announce", 0x99)]);

    // Site knows nothing
    let site_id = SiteId(1);
    let site_local = Arc::new(MockReseedSiteLocalUrls {
        name: "Empty".to_string(),
        base_url: "https://empty.example.com".to_string(),
        known_matches: vec![],
    });

    let mut registry = SiteRegistry::new();
    registry.register(
        site_id,
        AdapterHandle {
            core: site_local.clone() as Arc<dyn SiteCore>,
            reseed: Some(site_local as Arc<dyn ReseedCapable>),
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1, 100)),
        },
    );

    let dest_client = Arc::new(MockDownloader::new());

    let config = ReseedConfig {
        scan_folders: vec![dir.path().to_path_buf()],
        source_downloaders: vec![],
        target_site_ids: vec![site_id],
        default_save_path: "/downloads".to_string(),
        skip_hash_check: false,
        auto_start: false,
        tag: None,
        jackett_config: None,
    };

    let engine = ReseedEngine::new(Arc::new(registry), repo, db_writer, cancel);

    let (snapshot, preview) = engine
        .run_sync(config, dest_client.clone(), false)
        .await
        .unwrap();
    assert!(preview.is_none());

    assert_eq!(snapshot.scanned, 1);
    assert_eq!(snapshot.matched, 0);
    assert_eq!(snapshot.added, 0);
    assert_eq!(dest_client.added_count(), 0);
}

// ===========================================================================
// Test 6: Cancellation is respected
// ===========================================================================

#[tokio::test]
async fn test_scanner_respects_cancellation() {
    let (_db_dir, repo, db_writer) = setup_db().await;
    let dest_client = MockDownloader::new();
    let stats = ReseedStats::new();
    let cancel = CancellationToken::new();

    let (dir, _) =
        write_torrent_fixtures(&[("cancel1", "http://tracker.example.com/announce", 0x01)]);

    // Cancel before scanning
    cancel.cancel();

    let result = scan_folder(dir.path(), &repo, &db_writer, &dest_client, &stats, &cancel).await;

    assert!(result.is_err(), "should return error when cancelled");
}
