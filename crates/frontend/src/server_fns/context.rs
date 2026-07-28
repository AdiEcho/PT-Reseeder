// Shared server-side context and cross-domain SSR helpers.
//
// `ServerFnContext` is the request context injected by the server crate; the
// helpers below cover session cookies, credential encryption, pool access and
// site-registry refresh, and are used by every other domain file.

/// Boxed async callback returning `Result<(), String>`.
///
/// The registry-refresh and task-runtime hooks are injected by the server crate
/// as async closures; this alias names the shared `Arc<dyn Fn -> Pin<Box<Future>>>`
/// shape so the field declarations stay readable.
#[cfg(feature = "ssr")]
pub type AsyncHook = std::sync::Arc<
    dyn Fn() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>,
        > + Send
        + Sync,
>;

/// Same as [`AsyncHook`] but taking a task id.
#[cfg(feature = "ssr")]
pub type AsyncTaskHook = std::sync::Arc<
    dyn Fn(
            i64,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>,
        > + Send
        + Sync,
>;

/// Encrypted credential pair: `(ciphertext, nonce)`, both `None` when the
/// plaintext was blank.
#[cfg(feature = "ssr")]
pub type EncryptedField = (Option<Vec<u8>>, Option<Vec<u8>>);

#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct ServerFnContext {
    pub pool: sqlx::SqlitePool,
    pub vault: std::sync::Arc<tokio::sync::RwLock<Option<pt_reseeder_core::crypto::Vault>>>,
    pub session_ttl_hours: u64,
    pub cookie_secure: bool,
    pub data_dir: std::path::PathBuf,
    /// Runtime log directory used by the process file appender.
    pub log_dir: std::path::PathBuf,
    pub site_registry: std::sync::Arc<
        tokio::sync::RwLock<std::sync::Arc<pt_reseeder_core::site::registry::SiteRegistry>>,
    >,
    pub refresh_site_registry: AsyncHook,
    pub fetch_seeding_size: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub trigger_task_execution: std::sync::Arc<dyn Fn(i64, bool) + Send + Sync>,
    pub reconfigure_task_runtime: AsyncTaskHook,
    pub remove_task_runtime: AsyncTaskHook,
    pub authenticated_user_id: Option<i64>,
}

// Cookie name and token hashing live in core so the server middleware and these
// server functions cannot drift apart (a mismatch would silently log everyone
// out). The cookie *builders* below stay local: they return `axum_extra` types
// that core has no dependency on.
#[cfg(feature = "ssr")]
use pt_reseeder_core::session::{generate_session_token, hash_token, SESSION_COOKIE_NAME};

#[cfg(feature = "ssr")]
fn build_session_cookie(
    token: String,
    secure: bool,
) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};

    Cookie::build((SESSION_COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
        .build()
}

#[cfg(feature = "ssr")]
fn build_removal_cookie(secure: bool) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};

    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure)
        .max_age(time::Duration::ZERO)
        .build()
}

#[cfg(feature = "ssr")]
fn append_set_cookie(
    cookie: &axum_extra::extract::cookie::Cookie<'static>,
) -> Result<(), ServerFnError> {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos::prelude::expect_context;

    let value = HeaderValue::from_str(&cookie.to_string())
        .map_err(|e| ServerFnError::new(format!("invalid cookie header: {e}")))?;
    expect_context::<leptos_axum::ResponseOptions>().append_header(SET_COOKIE, value);
    Ok(())
}

#[cfg(feature = "ssr")]
fn server_context() -> Result<ServerFnContext, ServerFnError> {
    use leptos::prelude::use_context;

    use_context::<ServerFnContext>()
        .ok_or_else(|| ServerFnError::new("missing server function context"))
}

#[cfg(feature = "ssr")]
async fn auth_register(username: String, password: String) -> Result<(), ServerFnError> {
    use pt_reseeder_core::crypto::Vault;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let existing_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&context.pool)
        .await?;
    if existing_count.0 > 0 {
        return Err(ServerFnError::new("A user already exists"));
    }

    let (vault, reg) =
        Vault::create(&password).map_err(|e| ServerFnError::new(format!("crypto error: {e}")))?;
    let user_id = repo
        .create_user(
            &username,
            &reg.password_hash,
            &reg.kdf_salt,
            &reg.wrapped_dek,
            &reg.dek_nonce,
        )
        .await?;
    *context.vault.write().await = Some(vault);
    create_session_cookie(
        &repo,
        user_id,
        context.session_ttl_hours,
        context.cookie_secure,
    )
    .await?;
    refresh_site_registry_best_effort(&context).await;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn auth_login(username: String, password: String) -> Result<(), ServerFnError> {
    use pt_reseeder_core::crypto::Vault;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let user = repo
        .find_user_by_username(&username)
        .await?
        .ok_or_else(|| ServerFnError::new("Invalid username or password"))?;
    let vault = Vault::unlock(
        &password,
        &user.kdf_salt,
        &user.wrapped_dek,
        &user.dek_nonce,
        &user.password_hash,
    )
    .map_err(|_| ServerFnError::new("Invalid username or password"))?;
    *context.vault.write().await = Some(vault);
    let _ = repo.update_last_login(user.id).await;
    create_session_cookie(
        &repo,
        user.id,
        context.session_ttl_hours,
        context.cookie_secure,
    )
    .await?;
    refresh_site_registry_best_effort(&context).await;
    Ok(())
}

#[cfg(feature = "ssr")]
fn encrypt_optional(
    vault: &pt_reseeder_core::crypto::Vault,
    value: &str,
) -> Result<EncryptedField, ServerFnError> {
    if value.trim().is_empty() {
        return Ok((None, None));
    }
    let (ciphertext, nonce) = vault
        .encrypt(value.as_bytes())
        .map_err(|e| ServerFnError::new(format!("encryption error: {e}")))?;
    Ok((Some(ciphertext), Some(nonce.to_vec())))
}

#[cfg(feature = "ssr")]
async fn create_session_cookie(
    repo: &pt_reseeder_core::db::repo::Repository,
    user_id: i64,
    ttl_hours: u64,
    cookie_secure: bool,
) -> Result<(), ServerFnError> {
    let (raw_token, token_hash) = generate_session_token();
    let expires_at = pt_reseeder_core::session::session_expiry_from_now(ttl_hours);
    repo.create_session(user_id, &token_hash, &expires_at)
        .await?;
    let _ = repo.update_last_login(user_id).await;
    append_set_cookie(&build_session_cookie(raw_token, cookie_secure))
}

#[cfg(feature = "ssr")]
async fn refresh_site_registry_best_effort(context: &ServerFnContext) {
    if let Err(error) = (context.refresh_site_registry)().await {
        eprintln!("failed to refresh site registry: {error}");
    }
}

#[cfg(feature = "ssr")]
fn server_pool() -> Result<sqlx::SqlitePool, ServerFnError> {
    Ok(server_context()?.pool)
}
