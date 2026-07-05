use serde::{Deserialize, Serialize};

/// Status of a repost queue entry (state machine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepostStatus {
    Pending,
    Approved,
    Rejected,
    Submitted,
    Failed,
}

impl RepostStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Submitted => "submitted",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "submitted" => Some(Self::Submitted),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Check whether a transition from the current status to `target` is valid.
    pub fn can_transition_to(&self, target: &Self) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Approved)
                | (Self::Pending, Self::Rejected)
                | (Self::Approved, Self::Submitted)
                | (Self::Approved, Self::Failed)
                | (Self::Failed, Self::Approved)
        )
    }
}

/// Action taken during review.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAction {
    Approve,
    Reject,
}

/// Request to extract and enqueue a torrent for reposting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepostRequest {
    pub source_site_id: i64,
    pub source_torrent_id: String,
    pub target_site_id: i64,
}

/// Category mapping entry: maps a torrent_type string to a site-specific browsecat ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMapping {
    pub torrent_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category_id: i64,
}

/// Codec mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecMapping {
    pub codec: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub codec_id: i64,
}

/// Resolution mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionMapping {
    pub resolution: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub resolution_id: i64,
}

/// Source/medium mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMapping {
    pub medium: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub source_id: i64,
}

/// Full adapter mapping configuration for a target site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMapping {
    pub site_name: String,
    #[serde(default)]
    pub categories: Vec<CategoryMapping>,
    #[serde(default)]
    pub codecs: Vec<CodecMapping>,
    #[serde(default)]
    pub resolutions: Vec<ResolutionMapping>,
    #[serde(default)]
    pub sources: Vec<SourceMapping>,
    #[serde(default)]
    pub description_template: Option<String>,
}
