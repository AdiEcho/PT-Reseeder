use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::session::{
    generate_session_token, resolve_session, session_expiry_from_now, SessionOutcome,
    SESSION_COOKIE_NAME,
};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub username: String,
}

#[derive(Serialize)]
pub struct HasUserResponse {
    pub has_user: bool,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

/// Borrow every `Cookie:` header value for session resolution.
fn cookie_headers(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(axum::http::header::COOKIE)
        .into_iter()
        .filter_map(|v| v.to_str().ok())
}

/// Format a Set-Cookie header value for a new session token.
fn format_session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict",
        SESSION_COOKIE_NAME, token
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Format a Set-Cookie header value that removes the session cookie.
fn format_removal_cookie(secure: bool) -> String {
    let mut cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        SESSION_COOKIE_NAME
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Create a session row and return the raw token for the cookie.
async fn create_session(
    repo: &Repository,
    user_id: i64,
    ttl_hours: u64,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let (raw_token, token_hash) = generate_session_token();
    let expires_at = session_expiry_from_now(ttl_hours);
    repo.create_session(user_id, &token_hash, &expires_at)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = repo.update_last_login(user_id).await;
    Ok(raw_token)
}

/// Refresh the site registry, logging but not propagating errors.
async fn refresh_site_registry_best_effort(state: &AppState) {
    if let Err(e) = state.refresh_site_registry().await {
        tracing::warn!(%e, "failed to refresh site registry after auth");
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<(StatusCode, HeaderMap, Json<()>), (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;

    let user = repo
        .find_user_by_username(&body.username)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    let vault = Vault::unlock(
        &body.password,
        &user.kdf_salt,
        &user.wrapped_dek,
        &user.dek_nonce,
        &user.password_hash,
    )
    .map_err(|_| api_err(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    *state.inner.vault.write().await = Some(vault);
    let _ = repo.update_last_login(user.id).await;

    let raw_token = create_session(
        repo,
        user.id,
        state.inner.config.session_ttl_hours,
    )
    .await?;

    refresh_site_registry_best_effort(&state).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        format_session_cookie(&raw_token, state.inner.config.cookie_secure)
            .parse()
            .unwrap(),
    );
    Ok((StatusCode::OK, headers, Json(())))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<()>), (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;

    if let SessionOutcome::Valid(session) = resolve_session(repo, cookie_headers(&headers)).await {
        let _ = repo.delete_session(session.id).await;
    }

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        SET_COOKIE,
        format_removal_cookie(state.inner.config.cookie_secure)
            .parse()
            .unwrap(),
    );
    Ok((StatusCode::OK, resp_headers, Json(())))
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Option<UserInfo>>, (StatusCode, Json<ApiError>)> {
    // If vault is not unlocked, no user can be authenticated.
    if state.inner.vault.read().await.is_none() {
        return Ok(Json(None));
    }

    let repo = &state.inner.repo;
    let session = match resolve_session(repo, cookie_headers(&headers)).await {
        SessionOutcome::Valid(session) => session,
        SessionOutcome::Unauthenticated => return Ok(Json(None)),
        SessionOutcome::Failed(e) => {
            return Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    };

    let user = sqlx::query_as::<_, pt_reseeder_core::db::models::User>(
        "SELECT * FROM users WHERE id = ?",
    )
    .bind(session.user_id)
    .fetch_optional(&state.inner.db_pool)
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(user.map(|u| UserInfo {
        username: u.username,
    })))
}

async fn has_user(
    State(state): State<AppState>,
) -> Result<Json<HasUserResponse>, (StatusCode, Json<ApiError>)> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.inner.db_pool)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(HasUserResponse {
        has_user: count.0 > 0,
    }))
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, HeaderMap, Json<()>), (StatusCode, Json<ApiError>)> {
    let repo = &state.inner.repo;

    let existing_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.inner.db_pool)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing_count.0 > 0 {
        return Err(api_err(StatusCode::CONFLICT, "A user already exists"));
    }

    let (vault, reg) =
        Vault::create(&body.password).map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("crypto error: {e}")))?;

    let user_id = repo
        .create_user(
            &body.username,
            &reg.password_hash,
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
        )
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    *state.inner.vault.write().await = Some(vault);

    let raw_token = create_session(
        repo,
        user_id,
        state.inner.config.session_ttl_hours,
    )
    .await?;

    refresh_site_registry_best_effort(&state).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        format_session_cookie(&raw_token, state.inner.config.cookie_secure)
            .parse()
            .unwrap(),
    );
    Ok((StatusCode::CREATED, headers, Json(())))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Public auth routes — no session required.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/me", get(me))
        .route("/auth/has-user", get(has_user))
}

/// Auth routes that require a live session.
pub fn authed_router() -> Router<AppState> {
    Router::new().route("/auth/logout", post(logout))
}
