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

/// Name of the session cookie. Single source of truth for both the server
/// middleware and the Leptos server functions — changing this value logs out
/// every existing session.
pub const SESSION_COOKIE_NAME: &str = "pt_reseeder_session";

/// Generate a new session token. Returns `(raw_token_hex, token_hash_bytes)`.
pub fn generate_session_token() -> (String, Vec<u8>) {
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let hash = Sha256::digest(raw);
    (hex::encode(raw), hash.to_vec())
}

/// Hash a raw token hex string to get the token hash bytes for DB lookup.
///
/// The hash is taken over the *decoded* bytes, not the hex text. Hashing the
/// hex string instead would invalidate every stored session.
pub fn hash_token(raw_hex: &str) -> Option<Vec<u8>> {
    use sha2::{Digest, Sha256};

    let raw_bytes = hex::decode(raw_hex).ok()?;
    let hash = Sha256::digest(&raw_bytes);
    Some(hash.to_vec())
}

/// Extract the session token value out of a raw `Cookie:` header.
pub fn token_from_cookie_header(cookie_header: &str) -> Option<&str> {
    cookie_header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE_NAME).then_some(value))
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
