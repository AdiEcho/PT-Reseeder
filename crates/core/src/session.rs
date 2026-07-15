use chrono::{DateTime, NaiveDateTime, Utc};

/// Format a UTC timestamp the same way SQLite `datetime('now')` does.
pub fn format_sqlite_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Session expiry timestamp `hours` from now, SQLite-compatible UTC.
pub fn session_expiry_from_now(hours: u64) -> String {
    format_sqlite_utc(Utc::now() + chrono::Duration::hours(hours as i64))
}

/// Whether a stored session expiry string is already past.
///
/// Accepts both SQLite datetime (`YYYY-MM-DD HH:MM:SS`) and RFC3339 for
/// backwards compatibility with sessions written before the format fix.
pub fn is_session_expired(expires_at: &str) -> bool {
    let now = Utc::now();

    if let Ok(dt) = DateTime::parse_from_rfc3339(expires_at) {
        return dt.with_timezone(&Utc) <= now;
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S") {
        return naive.and_utc() <= now;
    }

    // Last resort: lexicographic compare against SQLite-compatible "now".
    expires_at <= format_sqlite_utc(now).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_format_has_no_timezone_suffix() {
        let s = format_sqlite_utc(Utc::now());
        assert!(!s.contains('T'));
        assert!(!s.contains('+'));
        assert!(!s.contains('Z'));
        assert_eq!(s.len(), 19);
    }

    #[test]
    fn expired_sqlite_datetime_detected() {
        assert!(is_session_expired("2000-01-01 00:00:00"));
    }

    #[test]
    fn future_sqlite_datetime_not_expired() {
        assert!(!is_session_expired("2099-01-01 00:00:00"));
    }

    #[test]
    fn expired_rfc3339_detected() {
        assert!(is_session_expired("2000-01-01T00:00:00+00:00"));
    }
}
