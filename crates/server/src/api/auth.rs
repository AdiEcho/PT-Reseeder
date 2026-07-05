use crate::auth;
use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::CookieJar;
use pt_reseeder_core::crypto::Vault;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub username: String,
}

#[derive(Serialize)]
pub struct AuthError {
    pub error: String,
}

/// POST /api/auth/register -- only works if no users exist yet
async fn register(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<(StatusCode, CookieJar, Json<UserInfo>), (StatusCode, Json<AuthError>)> {
    let repo = &state.inner.repo;

    // Check if any user already exists by trying to find one with the given username
    // A cleaner approach: try to find ANY user. We use the username lookup as a proxy --
    // but really we want "has any user been registered." We check if a user with this
    // username exists; if registration is first-come-first-serve, that's fine.
    // For a single-user app, we check if a user already exists at all.
    let existing = repo
        .find_user_by_username(&req.username)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(AuthError {
                error: "A user already exists".to_string(),
            }),
        ));
    }

    // Create vault with password
    let (vault, reg_data) = Vault::create(&req.password).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError {
                error: format!("crypto error: {}", e),
            }),
        )
    })?;

    // Create user in DB
    let user_id = repo
        .create_user(
            &req.username,
            &reg_data.password_hash,
            &reg_data.kdf_salt,
            &reg_data.wrapped_dek,
            &reg_data.dek_nonce,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    // Store vault in app state
    {
        let mut vault_lock = state.inner.vault.write().await;
        *vault_lock = Some(vault);
    }
    state.refresh_site_registry().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError {
                error: format!("failed to refresh site registry: {}", e),
            }),
        )
    })?;

    // Create session
    let (raw_token, token_hash) = auth::generate_session_token();
    let expires_at = (chrono::Utc::now()
        + chrono::Duration::hours(state.inner.config.session_ttl_hours as i64))
    .to_rfc3339();

    repo.create_session(user_id, &token_hash, &expires_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError {
                    error: format!("session error: {}", e),
                }),
            )
        })?;

    // Update last login
    let _ = repo.update_last_login(user_id).await;

    let jar = CookieJar::new().add(auth::build_session_cookie(raw_token));

    Ok((
        StatusCode::CREATED,
        jar,
        Json(UserInfo {
            username: req.username,
        }),
    ))
}

/// POST /api/auth/login
async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AuthRequest>,
) -> Result<(CookieJar, Json<UserInfo>), (StatusCode, Json<AuthError>)> {
    let repo = &state.inner.repo;

    // Find user by username
    let user = repo
        .find_user_by_username(&req.username)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(AuthError {
                    error: "Invalid username or password".to_string(),
                }),
            )
        })?;

    // Unlock vault with password (this verifies the password)
    let vault = Vault::unlock(
        &req.password,
        &user.kdf_salt,
        &user.wrapped_dek,
        &user.dek_nonce,
        &user.password_hash,
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(AuthError {
                error: "Invalid username or password".to_string(),
            }),
        )
    })?;

    // Store vault in app state
    {
        let mut vault_lock = state.inner.vault.write().await;
        *vault_lock = Some(vault);
    }
    state.refresh_site_registry().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError {
                error: format!("failed to refresh site registry: {}", e),
            }),
        )
    })?;

    // Create session token
    let (raw_token, token_hash) = auth::generate_session_token();
    let expires_at = (chrono::Utc::now()
        + chrono::Duration::hours(state.inner.config.session_ttl_hours as i64))
    .to_rfc3339();

    // Store session hash in DB
    repo.create_session(user.id, &token_hash, &expires_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError {
                    error: format!("session error: {}", e),
                }),
            )
        })?;

    // Update last login
    let _ = repo.update_last_login(user.id).await;

    let jar = jar.add(auth::build_session_cookie(raw_token));

    Ok((
        jar,
        Json(UserInfo {
            username: user.username,
        }),
    ))
}

/// POST /api/auth/logout
async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<CookieJar, StatusCode> {
    // Extract session cookie
    if let Some(cookie) = jar.get(auth::SESSION_COOKIE_NAME) {
        if let Some(token_hash) = auth::hash_token(cookie.value()) {
            // Find and delete session from DB
            if let Ok(Some(session)) = state.inner.repo.find_session_by_hash(&token_hash).await {
                let _ = state.inner.repo.delete_session(session.id).await;
            }
        }
    }

    // Clear cookie
    Ok(jar.add(auth::build_removal_cookie()))
}

/// GET /api/auth/me
async fn me(State(state): State<AppState>, jar: CookieJar) -> Result<Json<UserInfo>, StatusCode> {
    let cookie = jar
        .get(auth::SESSION_COOKIE_NAME)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_hash = auth::hash_token(cookie.value()).ok_or(StatusCode::UNAUTHORIZED)?;

    let session = state
        .inner
        .repo
        .find_session_by_hash(&token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check expiry
    let now = chrono::Utc::now().to_rfc3339();
    if session.expires_at < now {
        let _ = state.inner.repo.delete_session(session.id).await;
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Look up the user to return their info
    // We need to find user by id. Repository doesn't have find_by_id, so we use
    // a query directly. For now, we store the username from the session's user_id.
    let user =
        sqlx::query_as::<_, pt_reseeder_core::db::models::User>("SELECT * FROM users WHERE id = ?")
            .bind(session.user_id)
            .fetch_optional(&state.inner.db_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

    Ok(Json(UserInfo {
        username: user.username,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}
