use crate::db::models::Session;
use crate::db::repo::Repository;
use crate::error::CoreError;
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

/// Extract the session token out of `Cookie:` header values.
///
/// Takes an iterator because a request may legitimately carry more than one
/// `Cookie` header — RFC 6265 has browsers send one, but reverse proxies and
/// non-browser clients do append their own, and the token can be in any of them.
///
/// Values are unquoted and percent-decoded, matching what `CookieJar` did before
/// this logic moved into core: a proxy that normalises the cookie to
/// `name="value"` form must still authenticate.
pub fn token_from_cookie_headers<'a>(headers: impl IntoIterator<Item = &'a str>) -> Option<String> {
    headers
        .into_iter()
        .flat_map(|header| header.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE_NAME)
        .map(|(_, value)| decode_cookie_value(value))
}

/// Strip surrounding double quotes and percent-decode a cookie value.
fn decode_cookie_value(value: &str) -> String {
    let unquoted = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);

    let bytes = unquoted.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // A session token is hex, so any non-UTF-8 result cannot be a real token;
    // falling back to the raw text lets `hash_token` reject it as malformed.
    String::from_utf8(out).unwrap_or_else(|_| unquoted.to_string())
}

/// Outcome of resolving a session cookie against the database.
///
/// Callers map this onto their own transport-level error (HTTP status, an empty
/// "who am I" answer, …); the resolution logic itself stays transport-agnostic
/// so the server middleware and the Leptos server functions can share it.
#[derive(Debug)]
pub enum SessionOutcome {
    /// A live, unexpired session.
    Valid(Session),
    /// No cookie, a malformed token, or no matching row. Also returned for an
    /// expired session, *after* the stale row has been deleted.
    Unauthenticated,
    /// The database lookup itself failed. Distinct from `Unauthenticated`
    /// because a transient DB fault must not read as "logged out".
    Failed(CoreError),
}

/// Resolve the `Cookie:` headers of a request into a session.
///
/// Expired sessions are always deleted before returning `Unauthenticated`. That
/// used to differ per call site — the WebSocket path left stale rows behind
/// while the other four cleaned up (AC-2.3).
pub async fn resolve_session<'a>(
    repo: &Repository,
    cookie_headers: impl IntoIterator<Item = &'a str>,
) -> SessionOutcome {
    let Some(raw_token) = token_from_cookie_headers(cookie_headers) else {
        return SessionOutcome::Unauthenticated;
    };
    let Some(token_hash) = hash_token(&raw_token) else {
        return SessionOutcome::Unauthenticated;
    };
    let session = match repo.find_session_by_hash(&token_hash).await {
        Ok(Some(session)) => session,
        Ok(None) => return SessionOutcome::Unauthenticated,
        Err(e) => return SessionOutcome::Failed(e),
    };
    if is_session_expired(&session.expires_at) {
        // Best-effort cleanup: a failed delete must not turn an expired session
        // into a server error.
        let _ = repo.delete_session(session.id).await;
        return SessionOutcome::Unauthenticated;
    }
    SessionOutcome::Valid(session)
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
