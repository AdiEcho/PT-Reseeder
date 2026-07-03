use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::site::traits::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub overall_status: ProbeStatus,
    pub api_reachable: Option<FieldProbeResult>,
    pub user_info_fields: Vec<FieldProbeResult>,
    pub passkey_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProbeStatus {
    Ok,
    Partial,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldProbeResult {
    pub field_name: String,
    pub success: bool,
    pub value_preview: Option<String>,
    pub error: Option<String>,
}

impl fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProbeStatus::Ok => "ok",
            ProbeStatus::Partial => "partial",
            ProbeStatus::Failed => "failed",
            ProbeStatus::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for ProbeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ok" => Ok(ProbeStatus::Ok),
            "partial" => Ok(ProbeStatus::Partial),
            "failed" => Ok(ProbeStatus::Failed),
            "unknown" => Ok(ProbeStatus::Unknown),
            other => Err(format!("unknown probe status: {}", other)),
        }
    }
}

impl ProbeResult {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn status_str(&self) -> &str {
        match &self.overall_status {
            ProbeStatus::Ok => "ok",
            ProbeStatus::Partial => "partial",
            ProbeStatus::Failed => "failed",
            ProbeStatus::Unknown => "unknown",
        }
    }
}

pub fn format_bytes_preview(bytes: i64) -> String {
    const TB: f64 = 1_099_511_627_776.0;
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= TB {
        format!("{:.2} TB", bytes_f / TB)
    } else if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.2} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.2} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub async fn probe_site(
    reseed: Option<&Arc<dyn ReseedCapable>>,
    user_info: Option<&Arc<dyn UserInfoCapable>>,
) -> ProbeResult {
    let mut result = ProbeResult {
        overall_status: ProbeStatus::Unknown,
        api_reachable: None,
        user_info_fields: Vec::new(),
        passkey_available: None,
    };

    let mut any_passed = false;
    let mut any_failed = false;
    let mut any_tested = false;

    // Test reseed capability (API reachability)
    if let Some(reseed_cap) = reseed {
        any_tested = true;
        let dummy_hashes = vec!["0000000000000000000000000000000000000000".to_string()];
        match reseed_cap.query_pieces_hash(&dummy_hashes).await {
            Ok(_) => {
                debug!("API reachable for site {}", reseed_cap.name());
                result.api_reachable = Some(FieldProbeResult {
                    field_name: "api_reachable".to_string(),
                    success: true,
                    value_preview: None,
                    error: None,
                });
                any_passed = true;
            }
            Err(e) => {
                warn!("API unreachable for site {}: {}", reseed_cap.name(), e);
                result.api_reachable = Some(FieldProbeResult {
                    field_name: "api_reachable".to_string(),
                    success: false,
                    value_preview: None,
                    error: Some(e.to_string()),
                });
                any_failed = true;
            }
        }
    }

    // Test user info capability
    if let Some(user_info_cap) = user_info {
        any_tested = true;

        match user_info_cap.fetch_user_info().await {
            Ok(stats) => {
                debug!("User info fetched for site {}", user_info_cap.name());

                let fields: Vec<(&str, Option<String>)> = vec![
                    (
                        "uploaded",
                        stats.uploaded.map(|v| format_bytes_preview(v)),
                    ),
                    (
                        "downloaded",
                        stats.downloaded.map(|v| format_bytes_preview(v)),
                    ),
                    ("ratio", stats.ratio.map(|v| format!("{:.3}", v))),
                    ("bonus", stats.bonus.map(|v| format!("{:.1}", v))),
                    ("user_class", stats.user_class.clone()),
                    (
                        "seeding_count",
                        stats.seeding_count.map(|v| v.to_string()),
                    ),
                    (
                        "leeching_count",
                        stats.leeching_count.map(|v| v.to_string()),
                    ),
                    (
                        "seeding_size",
                        stats.seeding_size.map(|v| format_bytes_preview(v)),
                    ),
                    (
                        "upload_time_seconds",
                        stats.upload_time_seconds.map(|v| format!("{}s", v)),
                    ),
                ];

                for (field_name, value) in fields {
                    match value {
                        Some(preview) => {
                            result.user_info_fields.push(FieldProbeResult {
                                field_name: field_name.to_string(),
                                success: true,
                                value_preview: Some(preview),
                                error: None,
                            });
                            any_passed = true;
                        }
                        None => {
                            result.user_info_fields.push(FieldProbeResult {
                                field_name: field_name.to_string(),
                                success: false,
                                value_preview: None,
                                error: Some("field not parsed".to_string()),
                            });
                            any_failed = true;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    "User info fetch failed for site {}: {}",
                    user_info_cap.name(),
                    e
                );
                // Mark all fields as failed
                let field_names = [
                    "uploaded",
                    "downloaded",
                    "ratio",
                    "bonus",
                    "user_class",
                    "seeding_count",
                    "leeching_count",
                    "seeding_size",
                    "upload_time_seconds",
                ];
                for field_name in &field_names {
                    result.user_info_fields.push(FieldProbeResult {
                        field_name: field_name.to_string(),
                        success: false,
                        value_preview: None,
                        error: Some(e.to_string()),
                    });
                }
                any_failed = true;
            }
        }

        // Test passkey
        match user_info_cap.fetch_passkey().await {
            Ok(passkey) => {
                let available = passkey.is_some();
                result.passkey_available = Some(available);
                if available {
                    any_passed = true;
                } else {
                    any_failed = true;
                }
            }
            Err(e) => {
                warn!(
                    "Passkey fetch failed for site {}: {}",
                    user_info_cap.name(),
                    e
                );
                result.passkey_available = Some(false);
                any_failed = true;
            }
        }
    }

    // Determine overall status
    result.overall_status = if !any_tested {
        ProbeStatus::Unknown
    } else if any_passed && !any_failed {
        ProbeStatus::Ok
    } else if any_passed {
        ProbeStatus::Partial
    } else {
        ProbeStatus::Failed
    };

    result
}
