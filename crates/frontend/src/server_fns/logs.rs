// Logs: DTOs, line-parsing helpers and server functions for the log viewer.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFileInfo {
    pub filename: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPage {
    pub entries: Vec<LogEntry>,
    pub total_lines: usize,
    pub page: usize,
    pub page_size: usize,
}

#[cfg(feature = "ssr")]
fn resolve_log_dir(context: &ServerFnContext) -> std::path::PathBuf {
    // Always read from the runtime directory used by the file appender.
    // Settings `log_dir` does not reconfigure the appender after process start.
    context.log_dir.clone()
}

#[server]
pub async fn get_log_files() -> Result<Vec<LogFileInfo>, ServerFnError> {
    let context = server_context()?;
    let log_dir_path = resolve_log_dir(&context);

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
    Ok(files)
}

pub(crate) fn log_entry_matches_task_id(entry: &LogEntry, task_id: i64) -> bool {
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
///
/// An empty page would make the viewer render its empty state for a non-empty
/// log. During SSR that also diverges from what the client renders for the same
/// page, and the differing DOM shapes abort hydration.
#[cfg(feature = "ssr")]
fn clamp_page(page: usize, total_lines: usize, page_size: usize) -> usize {
    let page_size = page_size.max(1);
    let total_pages = total_lines.div_ceil(page_size).max(1);
    page.max(1).min(total_pages)
}

#[server]
pub async fn get_logs(
    filename: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
    level: Option<String>,
    keyword: Option<String>,
    task_id: Option<i64>,
) -> Result<LogPage, ServerFnError> {
    let context = server_context()?;
    let log_dir_path = resolve_log_dir(&context);
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(100).min(500);

    // Find the log file to read
    let file_path = if let Some(ref name) = filename {
        // Sanitize: prevent directory traversal
        let sanitized = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ServerFnError::new("无效的文件名"))?;
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
        // No historical file yet (fresh install / logging not writing files).
        return Ok(LogPage {
            entries: Vec::new(),
            total_lines: 0,
            page,
            page_size,
        });
    };

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| ServerFnError::new(format!("读取日志文件失败：{e}")))?;

    let level_filter = level.as_deref().unwrap_or("").to_uppercase();
    let keyword_filter = keyword.unwrap_or_default();

    // Parse lines into LogEntry
    let mut entries: Vec<LogEntry> = Vec::new();
    for raw_line in content.lines() {
        let entry = parse_log_line(raw_line);

        // Level filter
        if !level_filter.is_empty() && !entry.level.eq_ignore_ascii_case(&level_filter) {
            continue;
        }

        // Task filter
        if task_id.is_some_and(|id| !log_entry_matches_task_id(&entry, id)) {
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

    // Clamp the requested page into the range that actually exists, so an
    // out-of-range page never renders as "no logs" for a non-empty file.
    let page = clamp_page(page, total_lines, page_size);

    // Reverse so newest entries come first, then paginate
    entries.reverse();
    let start = (page - 1) * page_size;
    let page_entries: Vec<LogEntry> = entries.into_iter().skip(start).take(page_size).collect();

    Ok(LogPage {
        entries: page_entries,
        total_lines,
        page,
        page_size,
    })
}

#[cfg(feature = "ssr")]
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

#[cfg(all(test, feature = "ssr"))]
mod log_tests {
    use super::{clamp_page, log_entry_matches_task_id, parse_log_line, LogEntry};

    fn entry(message: &str) -> LogEntry {
        LogEntry {
            timestamp: String::new(),
            level: "INFO".into(),
            target: "test".into(),
            message: message.into(),
        }
    }

    #[test]
    fn task_log_filter_matches_exact_id() {
        assert!(log_entry_matches_task_id(
            &entry("task completed task_id=42 matched=1"),
            42,
        ));
        assert!(!log_entry_matches_task_id(
            &entry("task completed task_id=420 matched=1"),
            42,
        ));
        assert!(!log_entry_matches_task_id(&entry("task completed"), 42));
        assert!(log_entry_matches_task_id(
            &entry("downloading torrent for reseed task_id=42 site_id=1"),
            42,
        ));
    }

    #[test]
    fn task_log_filter_handles_formatter_punctuation() {
        assert!(log_entry_matches_task_id(&entry("task_id=42,"), 42));
        assert!(log_entry_matches_task_id(&entry("task_id=42}"), 42));
        assert!(!log_entry_matches_task_id(&entry("task_id=42x"), 42));
    }

    #[test]
    fn parsed_log_entry_preserves_task_field() {
        let entry =
            parse_log_line("2026-07-16T12:00:00.000Z INFO scheduler task completed task_id=42");
        assert!(log_entry_matches_task_id(&entry, 42));
    }

    #[test]
    fn clamps_page_past_the_end_to_the_last_real_page() {
        // 123 lines at 100/page spans 2 pages; page 3+ must not return empty.
        assert_eq!(clamp_page(3, 123, 100), 2);
        assert_eq!(clamp_page(99, 123, 100), 2);
        // Pages inside the range are left alone.
        assert_eq!(clamp_page(1, 123, 100), 1);
        assert_eq!(clamp_page(2, 123, 100), 2);
    }

    #[test]
    fn clamps_page_for_short_and_empty_logs() {
        // A single partial page stays page 1 whatever was asked for.
        assert_eq!(clamp_page(5, 40, 100), 1);
        // An exact multiple does not gain a trailing empty page.
        assert_eq!(clamp_page(2, 100, 100), 1);
        // An empty log still reports page 1 rather than page 0.
        assert_eq!(clamp_page(1, 0, 100), 1);
        assert_eq!(clamp_page(7, 0, 100), 1);
        // Page 0 and a zero page_size are coerced into valid values.
        assert_eq!(clamp_page(0, 123, 100), 1);
        assert_eq!(clamp_page(3, 123, 0), 3);
    }
}
