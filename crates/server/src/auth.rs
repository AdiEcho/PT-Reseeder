use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use sha2::{Sha256, Digest};
use crate::state::AppState;

pub const SESSION_COOKIE_NAME: &str = "pt_reseeder_session";

/// Generate a new session token. Returns (raw_token_hex, token_hash_bytes).
pub fn generate_session_token() -> (String, Vec<u8>) {
    use rand::RngCore;
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let hash = Sha256::digest(&raw);
    (hex::encode(raw), hash.to_vec())
}

/// Hash a raw token hex string to get the token hash bytes for DB lookup.
pub fn hash_token(raw_hex: &str) -> Option<Vec<u8>> {
    let raw_bytes = hex::decode(raw_hex).ok()?;
    let hash = Sha256::digest(&raw_bytes);
    Some(hash.to_vec())
}

/// Build a session cookie with the given token value.
pub fn build_session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(false) // Set to true in production behind TLS
        .build()
}

/// Build a removal cookie to clear the session.
pub fn build_removal_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(time::Duration::ZERO)
        .build()
}

/// Auth middleware: checks session cookie, validates, injects user_id into request extensions.
pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let cookie = jar
        .get(SESSION_COOKIE_NAME)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_hash = hash_token(cookie.value()).ok_or(StatusCode::UNAUTHORIZED)?;

    let session = state
        .inner
        .repo
        .find_session_by_hash(&token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check expiry: expires_at is stored as an ISO 8601 / SQLite datetime string
    let now = chrono::Utc::now().to_rfc3339();
    if session.expires_at < now {
        // Session expired, clean it up
        let _ = state.inner.repo.delete_session(session.id).await;
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Inject user_id into request extensions for downstream handlers
    request.extensions_mut().insert(AuthenticatedUser {
        user_id: session.user_id,
    });

    Ok(next.run(request).await)
}

/// Extension type inserted by require_auth middleware.
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub user_id: i64,
}

/// CSRF middleware: all non-GET/HEAD/OPTIONS requests to /api/* must have X-PT-Reseeder: 1
pub async fn csrf_check(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    if path.starts_with("/api/")
        && !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS)
    {
        if request.headers().get("X-PT-Reseeder").map(|v| v == "1") != Some(true) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(next.run(request).await)
}
