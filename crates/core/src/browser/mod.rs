// Headless browser module for Docker/headless repost autofill.
//
// Gated behind the `headless-browser` feature so the chromiumoxide
// dependency is fully optional.

#[cfg(feature = "headless-browser")]
pub mod headless;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::site::models::AdaptedTorrentInfo;

/// Result of an autofill operation (mirrors the desktop CustomEvent detail).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutofillResult {
    pub entry_id: i64,
    pub success: bool,
    pub filled: Vec<String>,
    pub skipped: Vec<String>,
    pub message: String,
}

/// Trait abstracting repost autofill capability.
///
/// Desktop (Tauri WebView) and headless (chromiumoxide) implementations both
/// satisfy this trait so the server layer can dispatch generically.
#[async_trait]
pub trait RepostAutoFiller: Send + Sync {
    /// Open the target site's upload page for a given repost queue entry.
    async fn open_upload_page(&self, site_url: &str, entry_id: i64) -> Result<(), CoreError>;

    /// Inject autofill JS into the currently-open upload page.
    async fn inject_autofill(
        &self,
        entry_id: i64,
        adapted_info: &AdaptedTorrentInfo,
    ) -> Result<AutofillResult, CoreError>;

    /// Whether this autofiller backend is available in the current runtime.
    fn is_available(&self) -> bool;
}
