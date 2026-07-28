use crate::state::AppState;
use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use pt_reseeder_core::db::models::Session;
use pt_reseeder_core::session::{resolve_session, SessionOutcome};

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

/// HTTP-layer wrapper around [`resolve_session`]: pull the cookie out of the
/// request headers and map the outcome onto status codes.
///
/// `Failed` maps to 500, not 401 — a transient database fault must not look like
/// a logged-out user, or the frontend would clear the session and bounce to the
/// login page.
pub async fn resolve_session_from_headers(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Session, StatusCode> {
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok());
    match resolve_session(&state.inner.repo, cookie_header).await {
        SessionOutcome::Valid(session) => Ok(session),
        SessionOutcome::Unauthenticated => Err(StatusCode::UNAUTHORIZED),
        SessionOutcome::Failed(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Auth middleware for the REST subtree: rejects a request without a live
/// session.
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    resolve_session_from_headers(&state, request.headers()).await?;

    Ok(next.run(request).await)
}

/// CSRF middleware: all non-GET/HEAD/OPTIONS requests to the /api subtree must
/// carry `X-PT-Reseeder: 1`.
///
/// There is deliberately no `/api/` prefix test. This layer is applied to the
/// `/api` subtree via `nest`, and `nest` strips the prefix before inner layers
/// run — so `request.uri().path()` here reads `/repost/queue/1/review`, never
/// `/api/repost/…`. The old `path.starts_with("/api/")` guard was therefore
/// always false and the whole check silently never fired; the 401s that made it
/// look alive came from `require_auth` further in.
pub async fn csrf_check(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();

    if !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS)
        && request.headers().get("X-PT-Reseeder").map(|v| v == "1") != Some(true)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}
