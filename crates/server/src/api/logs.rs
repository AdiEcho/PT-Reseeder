use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct LogFileInfo {
    pub filename: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct LogPage {
    pub entries: Vec<LogEntry>,
    pub total_lines: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GetLogsParams {
    pub filename: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub level: Option<String>,
    pub keyword: Option<String>,
    pub task_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

fn resolve_log_dir(state: &AppState) -> std::path::PathBuf {
    state.inner.config.log_dir.clone()
}

fn parse_log_line(line: &str) -> LogEntry {
    // Format: "2026-07-13T12:34:56.789Z INFO target message..."
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() >= 4 {
        LogEntry {
            timestamp: parts[0].to_string(),
            level: parts[1].to_string(),
            target: parts[2].to_string(),
            message: parts[3].to_string(),
        }
    } else {
        LogEntry {
            timestamp: String::new(),
            level: String::new(),
            target: String::new(),
            message: line.to_string(),
        }
    }
}

fn log_entry_matches_task_id(entry: &LogEntry, task_id: i64) -> bool {
    entry.message.split_whitespace().any(|field| {
        field
            .strip_prefix("task_id=")
            .map(|value| value.trim_matches(|c| matches!(c, '"' | ',' | ';' | ')' | ']' | '}')))
            .and_then(|value| value.parse::<i64>().ok())
            == Some(task_id)
    })
}

/// Clamps a requested page number into the range that `total_lines` actually
/// spans, so a caller asking past the end gets the last real page instead of an
/// empty one.
fn clamp_page(page: usize, total_lines: usize, page_size: usize) -> usize {
    let page_size = page_size.max(1);
    let total_pages = total_lines.div_ceil(page_size).max(1);
    page.max(1).min(total_pages)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /logs/files
async fn get_log_files(
    State(state): State<AppState>,
) -> Result<Json<Vec<LogFileInfo>>, (StatusCode, Json<ApiError>)> {
    let log_dir_path = resolve_log_dir(&state);

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&log_dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("pt-reseeder") {
                        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        files.push(LogFileInfo {
                            filename: name.to_string(),
                            size,
                        });
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(Json(files))
}

/// GET /logs
async fn get_logs(
    State(state): State<AppState>,
    Query(params): Query<GetLogsParams>,
) -> Result<Json<LogPage>, (StatusCode, Json<ApiError>)> {
    let log_dir_path = resolve_log_dir(&state);
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    // Find the log file to read
    let file_path = if let Some(ref name) = params.filename {
        // Sanitize: prevent directory traversal
        let sanitized = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "无效的文件名"))?;
        Some(log_dir_path.join(sanitized))
    } else {
        // Find the most recent log file
        let mut latest: Option<std::path::PathBuf> = None;
        if let Ok(entries) = std::fs::read_dir(&log_dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("pt-reseeder") {
                            match &latest {
                                None => latest = Some(path),
                                Some(prev) => {
                                    if path.file_name() > prev.file_name() {
                                        latest = Some(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        latest
    };

    let Some(file_path) = file_path else {
        return Ok(Json(LogPage {
            entries: Vec::new(),
            total_lines: 0,
            page,
            page_size,
        }));
    };

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("读取日志文件失败：{e}")))?;

    let level_filter = params.level.as_deref().unwrap_or("").to_uppercase();
    let keyword_filter = params.keyword.unwrap_or_default();

    // Parse lines into LogEntry
    let mut entries: Vec<LogEntry> = Vec::new();
    for raw_line in content.lines() {
        let entry = parse_log_line(raw_line);

        // Level filter
        if !level_filter.is_empty() && !entry.level.eq_ignore_ascii_case(&level_filter) {
            continue;
        }

        // Task filter
        if params
            .task_id
            .is_some_and(|id| !log_entry_matches_task_id(&entry, id))
        {
            continue;
        }

        // Keyword filter
        if !keyword_filter.is_empty()
            && !entry.message.contains(&keyword_filter)
            && !entry.target.contains(&keyword_filter)
        {
            continue;
        }

        entries.push(entry);
    }

    let total_lines = entries.len();

    // Clamp the requested page into the range that actually exists.
    let page = clamp_page(page, total_lines, page_size);

    // Reverse so newest entries come first, then paginate
    entries.reverse();
    let start = (page - 1) * page_size;
    let page_entries: Vec<LogEntry> = entries.into_iter().skip(start).take(page_size).collect();

    Ok(Json(LogPage {
        entries: page_entries,
        total_lines,
        page,
        page_size,
    }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/logs/files", get(get_log_files))
        .route("/logs", get(get_logs))
}
