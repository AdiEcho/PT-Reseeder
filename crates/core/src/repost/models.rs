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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repost_status_as_str_returns_lowercase_string() {
        assert_eq!(RepostStatus::Pending.as_str(), "pending");
        assert_eq!(RepostStatus::Approved.as_str(), "approved");
        assert_eq!(RepostStatus::Rejected.as_str(), "rejected");
        assert_eq!(RepostStatus::Submitted.as_str(), "submitted");
        assert_eq!(RepostStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn repost_status_from_str_parses_valid_strings() {
        assert_eq!(RepostStatus::from_str("pending"), Some(RepostStatus::Pending));
        assert_eq!(RepostStatus::from_str("approved"), Some(RepostStatus::Approved));
        assert_eq!(RepostStatus::from_str("rejected"), Some(RepostStatus::Rejected));
        assert_eq!(RepostStatus::from_str("submitted"), Some(RepostStatus::Submitted));
        assert_eq!(RepostStatus::from_str("failed"), Some(RepostStatus::Failed));
    }

    #[test]
    fn repost_status_from_str_returns_none_for_unknown() {
        assert_eq!(RepostStatus::from_str("unknown"), None);
        assert_eq!(RepostStatus::from_str(""), None);
        assert_eq!(RepostStatus::from_str("PENDING"), None);
    }

    #[test]
    fn pending_can_transition_to_approved() {
        assert!(RepostStatus::Pending.can_transition_to(&RepostStatus::Approved));
    }

    #[test]
    fn pending_can_transition_to_rejected() {
        assert!(RepostStatus::Pending.can_transition_to(&RepostStatus::Rejected));
    }

    #[test]
    fn pending_cannot_transition_to_submitted() {
        assert!(!RepostStatus::Pending.can_transition_to(&RepostStatus::Submitted));
    }

    #[test]
    fn pending_cannot_transition_to_failed() {
        assert!(!RepostStatus::Pending.can_transition_to(&RepostStatus::Failed));
    }

    #[test]
    fn approved_can_transition_to_submitted() {
        assert!(RepostStatus::Approved.can_transition_to(&RepostStatus::Submitted));
    }

    #[test]
    fn approved_can_transition_to_failed() {
        assert!(RepostStatus::Approved.can_transition_to(&RepostStatus::Failed));
    }

    #[test]
    fn approved_cannot_transition_to_pending() {
        assert!(!RepostStatus::Approved.can_transition_to(&RepostStatus::Pending));
    }

    #[test]
    fn failed_can_transition_to_approved() {
        assert!(RepostStatus::Failed.can_transition_to(&RepostStatus::Approved));
    }

    #[test]
    fn rejected_cannot_transition_to_any() {
        assert!(!RepostStatus::Rejected.can_transition_to(&RepostStatus::Pending));
        assert!(!RepostStatus::Rejected.can_transition_to(&RepostStatus::Approved));
        assert!(!RepostStatus::Rejected.can_transition_to(&RepostStatus::Rejected));
        assert!(!RepostStatus::Rejected.can_transition_to(&RepostStatus::Submitted));
        assert!(!RepostStatus::Rejected.can_transition_to(&RepostStatus::Failed));
    }

    #[test]
    fn submitted_cannot_transition_to_any() {
        assert!(!RepostStatus::Submitted.can_transition_to(&RepostStatus::Pending));
        assert!(!RepostStatus::Submitted.can_transition_to(&RepostStatus::Approved));
        assert!(!RepostStatus::Submitted.can_transition_to(&RepostStatus::Rejected));
        assert!(!RepostStatus::Submitted.can_transition_to(&RepostStatus::Submitted));
        assert!(!RepostStatus::Submitted.can_transition_to(&RepostStatus::Failed));
    }

    #[test]
    fn review_action_serializes_to_json_and_back() {
        let approve = ReviewAction::Approve;
        let json = serde_json::to_string(&approve).unwrap();
        assert_eq!(json, r#""approve""#);
        let deserialized: ReviewAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ReviewAction::Approve));

        let reject = ReviewAction::Reject;
        let json = serde_json::to_string(&reject).unwrap();
        assert_eq!(json, r#""reject""#);
        let deserialized: ReviewAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ReviewAction::Reject));
    }

    #[test]
    fn repost_request_serializes_to_json_and_back() {
        let request = RepostRequest {
            source_site_id: 1,
            source_torrent_id: "12345".to_string(),
            target_site_id: 2,
        };
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RepostRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source_site_id, 1);
        assert_eq!(deserialized.source_torrent_id, "12345");
        assert_eq!(deserialized.target_site_id, 2);
    }

    #[test]
    fn category_mapping_deserializes_with_empty_aliases() {
        let json = r#"{"torrent_type": "movie", "category_id": 42}"#;
        let mapping: CategoryMapping = serde_json::from_str(json).unwrap();
        assert_eq!(mapping.torrent_type, "movie");
        assert_eq!(mapping.category_id, 42);
        assert!(mapping.aliases.is_empty());
    }

    #[test]
    fn adapter_mapping_deserializes_with_minimal_fields() {
        let json = r#"{"site_name": "example"}"#;
        let mapping: AdapterMapping = serde_json::from_str(json).unwrap();
        assert_eq!(mapping.site_name, "example");
        assert!(mapping.categories.is_empty());
        assert!(mapping.codecs.is_empty());
        assert!(mapping.resolutions.is_empty());
        assert!(mapping.sources.is_empty());
        assert!(mapping.description_template.is_none());
    }

    #[test]
    fn repost_status_clone_preserves_equality() {
        let statuses = vec![
            RepostStatus::Pending,
            RepostStatus::Approved,
            RepostStatus::Rejected,
            RepostStatus::Submitted,
            RepostStatus::Failed,
        ];
        for status in &statuses {
            let cloned = status.clone();
            assert_eq!(status, &cloned);
        }
    }
}
