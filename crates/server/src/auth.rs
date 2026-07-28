use crate::state::AppState;
use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

// Single source of truth lives in core so the Leptos server functions (which
// cannot depend on this crate) share the exact same cookie name and hashing.
pub use pt_reseeder_core::session::{generate_session_token, hash_token, SESSION_COOKIE_NAME};

/// Build a session cookie with the given token value.
///
/// Stays here rather than in core: the return type is an `axum_extra` cookie,
/// and core deliberately has no HTTP framework dependency.
pub fn build_session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
        .build()
}

/// Build a removal cookie to clear the session.
pub fn build_removal_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
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

    if pt_reseeder_core::session::is_session_expired(&session.expires_at) {
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
        && request.headers().get("X-PT-Reseeder").map(|v| v == "1") != Some(true)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}
