use leptos::prelude::*;
use serde::{Deserialize, Serialize};

pub const FETCH_SEEDING_SIZE_CONFIG_KEY: &str = "fetch_seeding_size";

#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct ServerFnContext {
    pub pool: sqlx::SqlitePool,
    pub vault: std::sync::Arc<tokio::sync::RwLock<Option<pt_reseeder_core::crypto::Vault>>>,
    pub session_ttl_hours: u64,
    pub cookie_secure: bool,
    pub data_dir: std::path::PathBuf,
    pub site_registry: std::sync::Arc<
        tokio::sync::RwLock<std::sync::Arc<pt_reseeder_core::site::registry::SiteRegistry>>,
    >,
    pub refresh_site_registry: std::sync::Arc<
        dyn Fn() -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'static>,
            > + Send
            + Sync,
    >,
    pub fetch_seeding_size: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub authenticated_user_id: Option<i64>,
}

#[cfg(feature = "ssr")]
const SESSION_COOKIE_NAME: &str = "pt_reseeder_session";

#[cfg(feature = "ssr")]
fn generate_session_token() -> (String, Vec<u8>) {
    use rand::RngCore;
    use sha2::{Digest, Sha256};

    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let hash = Sha256::digest(raw);
    (hex::encode(raw), hash.to_vec())
}

#[cfg(feature = "ssr")]
fn hash_token(raw_hex: &str) -> Option<Vec<u8>> {
    use sha2::{Digest, Sha256};

    let raw_bytes = hex::decode(raw_hex).ok()?;
    Some(Sha256::digest(raw_bytes).to_vec())
}

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
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
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
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
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
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
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
) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), ServerFnError> {
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
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let _ = repo.update_last_login(user_id).await;
    append_set_cookie(&build_session_cookie(raw_token, cookie_secure))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardOverview {
    pub running_tasks: i64,
    pub today_success: i64,
    pub today_failed: i64,
    pub total_sites: i64,
    pub tracked_torrents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteReseedStats {
    pub site_id: i64,
    pub site_name: String,
    pub matched: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub success_rate: f64,
    /// Circuit breaker status: "ok" | "tripped" | "unknown"
    pub breaker_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub date: String,
    pub succeeded: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteUserInfo {
    pub site_id: i64,
    pub site_name: String,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub ratio: Option<f64>,
    pub bonus: Option<f64>,
    pub user_class: Option<String>,
    pub seeding_count: Option<i64>,
    pub leeching_count: Option<i64>,
    pub seeding_size: Option<i64>,
    pub upload_time_seconds: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoAggregate {
    pub total_uploaded: i64,
    pub total_downloaded: i64,
    pub total_seeding: i64,
    pub total_bonus: f64,
    pub site_count: i64,
    pub sites: Vec<SiteUserInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub overview: DashboardOverview,
    pub site_stats: Vec<SiteReseedStats>,
    pub trend: Vec<TrendPoint>,
    pub user_info: UserInfoAggregate,
}

#[server]
pub async fn has_user() -> Result<bool, ServerFnError> {
    let pool = server_pool()?;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(count.0 > 0)
}

#[server]
pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
    auth_login(username, password).await
}

#[server]
pub async fn register(username: String, password: String) -> Result<(), ServerFnError> {
    auth_register(username, password).await
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    use axum_extra::extract::cookie::CookieJar;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let jar: CookieJar = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    if let Some(cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(token_hash) = hash_token(cookie.value()) {
            let repo = Repository::new(context.pool.clone());
            if let Some(session) = repo
                .find_session_by_hash(&token_hash)
                .await
                .map_err(|e| ServerFnError::new(format!("{e}")))?
            {
                let _ = repo.delete_session(session.id).await;
            }
        }
    }
    append_set_cookie(&build_removal_cookie(context.cookie_secure))?;
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    use axum_extra::extract::cookie::CookieJar;
    use pt_reseeder_core::db::models::User;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    if context.vault.read().await.is_none() {
        return Ok(None);
    }

    let jar: CookieJar = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let Some(cookie) = jar.get(SESSION_COOKIE_NAME) else {
        return Ok(None);
    };
    let Some(token_hash) = hash_token(cookie.value()) else {
        return Ok(None);
    };

    let pool = server_pool()?;
    let repo = Repository::new(pool.clone());
    let Some(session) = repo
        .find_session_by_hash(&token_hash)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
    else {
        return Ok(None);
    };
    if pt_reseeder_core::session::is_session_expired(&session.expires_at) {
        let _ = repo.delete_session(session.id).await;
        return Ok(None);
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(session.user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(user.map(|user| UserInfo {
        username: user.username,
    }))
}

// ── Repost types & server functions ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepostEntry {
    pub id: i64,
    pub source_site_name: String,
    pub source_torrent_id: String,
    pub target_site_name: String,
    pub status: String,
    pub review_notes: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDefinitionInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteInfo {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: String,
    pub auth_type: String,
    pub probe_status: String,
    pub probe_detail_json: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDetailData {
    pub site: SiteInfo,
    pub user_stats: Option<SiteUserInfo>,
    pub probe_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateSiteResult {
    pub status: String,
    pub message: String,
    pub detail_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderInfo {
    pub id: i64,
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    pub role: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderPairInfo {
    pub id: i64,
    pub name: String,
    pub source_id: i64,
    pub destination_id: i64,
    pub source_name: String,
    pub destination_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub id: i64,
    pub path: String,
    pub scan_mode: String,
    pub downloader_id: Option<i64>,
    pub enabled: bool,
    pub last_scanned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: i64,
    pub name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub cron_expression: Option<String>,
    pub status: String,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogInfo {
    pub status: String,
    pub matched_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub duration_ms: Option<i64>,
    pub created_at: String,
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

#[server]
pub async fn get_sites() -> Result<Vec<SiteInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let sites = repo
        .list_sites()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(sites
        .into_iter()
        .map(|s| SiteInfo {
            id: s.id,
            name: s.name,
            url: s.url,
            api_url: s.api_url,
            adapter_type: s.adapter_type,
            auth_type: s.auth_type,
            probe_status: s.probe_status,
            probe_detail_json: s.probe_detail_json,
            enabled: s.enabled,
        })
        .collect())
}

#[server]
pub async fn get_site_definitions() -> Result<Vec<SiteDefinitionInfo>, ServerFnError> {
    use pt_reseeder_core::site::definitions::load_all_definitions;

    let context = server_context()?;
    let definitions = load_all_definitions(Some(&context.data_dir));
    let mut results: Vec<SiteDefinitionInfo> = definitions
        .into_values()
        .map(|def| SiteDefinitionInfo {
            id: def.site.id,
            name: def.site.name,
            url: def.site.url,
            api_url: def.site.api_url,
            adapter: def.site.adapter,
        })
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

#[server]
pub async fn update_site_url(
    id: i64,
    url: String,
    api_url: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(ServerFnError::new("URL 不能为空"));
    }

    let repo = Repository::new(server_pool()?);
    let api_url_opt = if api_url.trim().is_empty() {
        None
    } else {
        Some(api_url.trim().to_string())
    };
    repo.update_site_url(id, &url, api_url_opt.as_deref())
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    refresh_site_registry_best_effort(&server_context()?).await;
    get_site_info(id).await
}

#[server]
pub async fn update_site(
    id: i64,
    url: String,
    api_url: String,
    cookie: String,
    passkey: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(ServerFnError::new("URL 不能为空"));
    }

    let context = server_context()?;
    let vault = context
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| ServerFnError::new("凭证已锁定，请重新登录后再操作"))?;
    let repo = Repository::new(context.pool.clone());

    // Update URL
    let api_url_opt = if api_url.trim().is_empty() {
        None
    } else {
        Some(api_url.trim().to_string())
    };
    repo.update_site_url(id, &url, api_url_opt.as_deref())
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    // Update credentials (only if user provided new values; empty means keep existing)
    let cookie_trimmed = cookie.trim().to_string();
    let passkey_trimmed = passkey.trim().to_string();
    if !cookie_trimmed.is_empty() || !passkey_trimmed.is_empty() {
        // Load existing credentials to preserve unchanged fields
        let site_row = repo
            .get_site(id)
            .await
            .map_err(|e| ServerFnError::new(format!("{e}")))?
            .ok_or_else(|| ServerFnError::new("站点不存在"))?;

        let (encrypted_cookie, cookie_nonce) = if !cookie_trimmed.is_empty() {
            encrypt_optional(&vault, &cookie_trimmed)?
        } else {
            (
                site_row.encrypted_cookie.clone(),
                site_row.cookie_nonce.clone(),
            )
        };
        let (encrypted_passkey, passkey_nonce) = if !passkey_trimmed.is_empty() {
            encrypt_optional(&vault, &passkey_trimmed)?
        } else {
            (
                site_row.encrypted_passkey.clone(),
                site_row.passkey_nonce.clone(),
            )
        };

        repo.update_site_credentials(
            id,
            encrypted_cookie.as_deref(),
            cookie_nonce.as_deref(),
            encrypted_passkey.as_deref(),
            passkey_nonce.as_deref(),
            site_row.encrypted_token.as_deref(),
            site_row.token_nonce.as_deref(),
        )
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    }

    refresh_site_registry_best_effort(&context).await;
    get_site_info(id).await
}

#[server]
pub async fn create_site(
    name: String,
    url: String,
    api_url: String,
    adapter_type: String,
    auth_type: String,
    cookie: String,
    passkey: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let vault = context
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| ServerFnError::new("凭证已锁定，请重新登录后再创建站点"))?;
    let repo = Repository::new(context.pool.clone());
    let adapter = adapter_type.to_ascii_lowercase();
    if !matches!(
        adapter.as_str(),
        "nexusphp" | "mteam" | "unit3d" | "gazelle" | "zhuque"
    ) {
        return Err(ServerFnError::new(format!(
            "不支持的站点架构：{adapter_type}"
        )));
    }
    let id = match repo
        .create_site(
            &name,
            &url,
            (!api_url.trim().is_empty()).then_some(api_url.as_str()),
            &adapter,
            &auth_type,
        )
        .await
    {
        Ok(id) => id,
        Err(error) => return Err(ServerFnError::new(format!("{error}"))),
    };
    let credential_result = async {
        let (encrypted_cookie, cookie_nonce) = encrypt_optional(&vault, &cookie)?;
        let (encrypted_passkey, passkey_nonce) = encrypt_optional(&vault, &passkey)?;
        repo.update_site_credentials(
            id,
            encrypted_cookie.as_deref(),
            cookie_nonce.as_deref(),
            encrypted_passkey.as_deref(),
            passkey_nonce.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
    }
    .await;
    if let Err(error) = credential_result {
        let _ = repo.delete_site(id).await;
        return Err(error);
    }
    refresh_site_registry_best_effort(&context).await;

    // 后台抓取一次用户数据，不阻塞站点创建流程
    {
        let pool = context.pool.clone();
        let site_registry = context.site_registry.clone();
        let site_id = id;
        tokio::spawn(async move {
            use pt_reseeder_core::db::models::UserStatRecord;
            use pt_reseeder_core::db::repo::Repository;
            use pt_reseeder_core::site::models::SiteId;

            let registry = site_registry.read().await.clone();
            let handle = registry.get(&SiteId::from(site_id));
            let user_info_cap = handle.and_then(|h| h.user_info.as_ref());
            if let Some(ui) = user_info_cap {
                match ui.fetch_user_info().await {
                    Ok(stats) => {
                        let repo = Repository::new(pool);
                        let record = UserStatRecord {
                            id: 0,
                            site_id,
                            uploaded: stats.uploaded,
                            downloaded: stats.downloaded,
                            ratio: stats.ratio,
                            bonus: stats.bonus,
                            user_class: stats.user_class,
                            seeding_count: stats.seeding_count,
                            leeching_count: stats.leeching_count,
                            seeding_size: stats.seeding_size,
                            upload_time_seconds: stats.upload_time_seconds,
                            fetched_at: String::new(),
                        };
                        if let Err(e) = repo.insert_user_stats(site_id, &record).await {
                            eprintln!("创建站点后自动抓取用户数据写入失败: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("创建站点后自动抓取用户数据失败: {e}");
                    }
                }
            }
        });
    }

    get_site_info(id).await
}

#[server]
pub async fn validate_site(
    name: String,
    url: String,
    api_url: String,
    adapter_type: String,
    cookie: String,
    passkey: String,
) -> Result<ValidateSiteResult, ServerFnError> {
    use pt_reseeder_core::site::adapters::gazelle::GazelleAdapter;
    use pt_reseeder_core::site::adapters::mteam::MTeamAdapter;
    use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
    use pt_reseeder_core::site::adapters::unit3d::Unit3dAdapter;
    use pt_reseeder_core::site::adapters::zhuque::ZhuqueAdapter;
    use pt_reseeder_core::site::definitions::load_all_definitions;
    use pt_reseeder_core::site::models::UserInfoSelectors;
    use pt_reseeder_core::site::probe::probe_site as run_site_probe;
    use pt_reseeder_core::site::traits::UserInfoCapable;
    use std::sync::Arc;

    let context = server_context()?;
    let adapter = adapter_type.to_ascii_lowercase();
    let api_url_opt = (!api_url.trim().is_empty()).then_some(api_url);
    let cookie_opt = (!cookie.trim().is_empty()).then_some(cookie);
    let passkey_opt = (!passkey.trim().is_empty()).then_some(passkey);

    let definitions = load_all_definitions(Some(&context.data_dir));
    let selectors = definitions
        .get(&name)
        .and_then(|def| def.user_info.clone())
        .unwrap_or_else(|| UserInfoSelectors {
            profile_url_template: None,
            uid_selector: None,
            uploaded_selector: None,
            downloaded_selector: None,
            ratio_selector: None,
            bonus_selector: None,
            user_class_selector: None,
            seeding_count_selector: None,
            leeching_count_selector: None,
            seeding_size_selector: None,
            upload_time_selector: None,
        });

    let fetch_seeding_size = context
        .fetch_seeding_size
        .load(std::sync::atomic::Ordering::Relaxed);
    let user_info: Arc<dyn UserInfoCapable> = match adapter.as_str() {
        "nexusphp" => Arc::new(
            NexusPhpAdapter::new(
                name,
                url,
                api_url_opt,
                cookie_opt,
                passkey_opt,
                None,
                selectors,
                100,
            )
            .with_fetch_seeding_size(fetch_seeding_size),
        ),
        "mteam" => Arc::new(MTeamAdapter::new(name, url, None, passkey_opt, 100)),
        "unit3d" => Arc::new(Unit3dAdapter::new(name, url, None, passkey_opt, 100)),
        "gazelle" => Arc::new(GazelleAdapter::new(name, url, cookie_opt, passkey_opt, 100)),
        "zhuque" => Arc::new(ZhuqueAdapter::new(
            name,
            url,
            None,
            passkey_opt,
            cookie_opt,
            100,
        )),
        other => {
            return Ok(ValidateSiteResult {
                status: "failed".to_string(),
                message: format!("不支持的站点架构：{other}"),
                detail_json: None,
            });
        }
    };

    let probe = run_site_probe(None, Some(&user_info)).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();
    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    })
}

#[server]
pub async fn delete_site(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    Repository::new(context.pool.clone())
        .delete_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    refresh_site_registry_best_effort(&context).await;
    Ok(())
}

#[server]
pub async fn probe_site(id: i64) -> Result<ValidateSiteResult, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::site::probe::probe_site as run_site_probe;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let site = repo
        .get_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .ok_or_else(|| ServerFnError::new("site not found"))?;

    let registry = context.site_registry.read().await.clone();
    let handle = registry
        .get(&pt_reseeder_core::site::models::SiteId::from(site.id))
        .cloned()
        .ok_or_else(|| ServerFnError::new("站点适配器未注册，请确认凭证已解锁且站点架构受支持"))?;
    let probe = run_site_probe(handle.reseed.as_ref(), handle.user_info.as_ref()).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();
    repo.update_probe_status(id, &status, Some(&detail))
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    })
}

#[server]
pub async fn get_site_detail(id: i64) -> Result<SiteDetailData, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let site = repo
        .get_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .ok_or_else(|| ServerFnError::new("site not found"))?;
    let user_stats = repo
        .get_latest_stats_by_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .map(|s| SiteUserInfo {
            site_id: s.site_id,
            site_name: site.name.clone(),
            uploaded: s.uploaded,
            downloaded: s.downloaded,
            ratio: s.ratio,
            bonus: s.bonus,
            user_class: s.user_class,
            seeding_count: s.seeding_count,
            leeching_count: s.leeching_count,
            seeding_size: s.seeding_size,
            upload_time_seconds: s.upload_time_seconds,
            fetched_at: s.fetched_at,
        });
    Ok(SiteDetailData {
        probe_detail: site.probe_detail_json.clone(),
        site: SiteInfo {
            id: site.id,
            name: site.name,
            url: site.url,
            api_url: site.api_url,
            adapter_type: site.adapter_type,
            auth_type: site.auth_type,
            probe_status: site.probe_status,
            probe_detail_json: site.probe_detail_json,
            enabled: site.enabled,
        },
        user_stats,
    })
}

#[server]
pub async fn refresh_site_stats(id: i64) -> Result<(), ServerFnError> {
    let _ = get_site_detail(id).await?;
    Ok(())
}

#[server]
async fn get_site_info(id: i64) -> Result<SiteInfo, ServerFnError> {
    Ok(get_site_detail(id).await?.site)
}

#[server]
pub async fn get_downloaders() -> Result<Vec<DownloaderInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .list_downloaders()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|d| DownloaderInfo {
            id: d.id,
            name: d.name,
            dl_type: d.dl_type,
            host: d.host,
            port: d.port,
            role: d.role,
            enabled: d.enabled,
        })
        .collect())
}

#[server]
pub async fn create_downloader(
    name: String,
    dl_type: String,
    host: String,
    port: i64,
    username: String,
    password: String,
    role: String,
) -> Result<DownloaderInfo, ServerFnError> {
    use pt_reseeder_core::db::models::DownloaderRow;
    use pt_reseeder_core::db::repo::Repository;

    // --- 输入验证 ---
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("名称不能为空"));
    }
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err(ServerFnError::new("主机地址不能为空"));
    }
    if !(1..=65535).contains(&port) {
        return Err(ServerFnError::new("端口必须在 1-65535 范围内"));
    }
    if !matches!(dl_type.as_str(), "qbittorrent" | "transmission") {
        return Err(ServerFnError::new("不支持的下载器类型"));
    }
    if !matches!(role.as_str(), "source" | "destination" | "both") {
        return Err(ServerFnError::new("无效的用途选项"));
    }

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let (encrypted_username, username_nonce, encrypted_password, password_nonce) = {
        let vault_guard = context.vault.read().await;
        if let Some(vault) = vault_guard.as_ref() {
            let (enc_user, user_nonce) = encrypt_optional(vault, &username)?;
            let (enc_pass, pass_nonce) = encrypt_optional(vault, &password)?;
            (enc_user, user_nonce, enc_pass, pass_nonce)
        } else {
            (None, None, None, None)
        }
    };
    let row = DownloaderRow {
        id: 0,
        name,
        dl_type,
        host,
        port,
        encrypted_username,
        username_nonce,
        encrypted_password,
        password_nonce,
        role,
        torrent_dir: None,
        default_save_path: None,
        skip_hash_check: Some(true),
        auto_start: Some(true),
        tag: Some("PT-Reseeder".into()),
        enabled: true,
        created_at: String::new(),
    };
    let id = repo
        .create_downloader(&row)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    get_downloaders()
        .await?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| ServerFnError::new("downloader created but not found"))
}

/// 在创建前测试下载器连接（不保存到数据库）
#[server]
pub async fn test_downloader_connection(
    dl_type: String,
    host: String,
    port: i64,
    username: String,
    password: String,
) -> Result<String, ServerFnError> {
    use pt_reseeder_core::downloader::qbittorrent::QBittorrentClient;
    use pt_reseeder_core::downloader::traits::Downloader;
    use pt_reseeder_core::downloader::transmission::TransmissionClient;

    if host.trim().is_empty() {
        return Err(ServerFnError::new("主机地址不能为空"));
    }
    if !(1..=65535).contains(&port) {
        return Err(ServerFnError::new("端口必须在 1-65535 范围内"));
    }

    match dl_type.as_str() {
        "qbittorrent" => {
            let mut client = QBittorrentClient::new(host.trim(), port as u16, &username, &password);
            client
                .connect()
                .await
                .map_err(|e| ServerFnError::new(format!("连接失败：{e}")))?;
            let version = client.get_version().await.ok();
            Ok(format!(
                "连接成功{}",
                version.map(|v| format!("，版本：{v}")).unwrap_or_default(),
            ))
        }
        "transmission" => {
            let mut client = TransmissionClient::new(
                host.trim(),
                port as u16,
                if username.is_empty() {
                    None
                } else {
                    Some(username.as_str())
                },
                if password.is_empty() {
                    None
                } else {
                    Some(password.as_str())
                },
            );
            client
                .connect()
                .await
                .map_err(|e| ServerFnError::new(format!("连接失败：{e}")))?;
            let version = client.get_version().await.ok();
            Ok(format!(
                "连接成功{}",
                version.map(|v| format!("，版本：{v}")).unwrap_or_default(),
            ))
        }
        other => Err(ServerFnError::new(format!("不支持的下载器类型：{other}"))),
    }
}

#[server]
pub async fn delete_downloader(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_downloader(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[cfg(feature = "ssr")]
fn decrypt_optional(
    vault: &pt_reseeder_core::crypto::Vault,
    encrypted: &Option<Vec<u8>>,
    nonce: &Option<Vec<u8>>,
) -> Result<Option<String>, ServerFnError> {
    let (Some(encrypted), Some(nonce)) = (encrypted.as_ref(), nonce.as_ref()) else {
        return Ok(None);
    };
    let nonce: [u8; 12] = nonce
        .as_slice()
        .try_into()
        .map_err(|_| ServerFnError::new("invalid credential nonce"))?;
    let plaintext = vault
        .decrypt(encrypted, &nonce)
        .map_err(|e| ServerFnError::new(format!("decryption error: {e}")))?;
    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|e| ServerFnError::new(format!("credential is not UTF-8: {e}")))
}

#[server]
pub async fn test_downloader(id: i64) -> Result<String, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::downloader::qbittorrent::QBittorrentClient;
    use pt_reseeder_core::downloader::traits::Downloader;
    use pt_reseeder_core::downloader::transmission::TransmissionClient;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let row = repo
        .get_downloader(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .ok_or_else(|| ServerFnError::new("downloader not found"))?;
    let vault_guard = context.vault.read().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| ServerFnError::new("vault is locked; please log in first"))?;
    let username = decrypt_optional(vault, &row.encrypted_username, &row.username_nonce)?;
    let password = decrypt_optional(vault, &row.encrypted_password, &row.password_nonce)?;

    match row.dl_type.as_str() {
        "qbittorrent" => {
            let mut client = QBittorrentClient::new(
                &row.host,
                row.port as u16,
                username.as_deref().unwrap_or(""),
                password.as_deref().unwrap_or(""),
            );
            client
                .connect()
                .await
                .map_err(|e| ServerFnError::new(format!("{e}")))?;
            let version = client.get_version().await.ok();
            let torrent_count = client.get_torrent_count().await.ok();
            Ok(format!(
                "Connection successful{}{}",
                version
                    .map(|v| format!("; version: {v}"))
                    .unwrap_or_default(),
                torrent_count
                    .map(|c| format!("; torrents: {c}"))
                    .unwrap_or_default()
            ))
        }
        "transmission" => {
            let mut client = TransmissionClient::new(
                &row.host,
                row.port as u16,
                username.as_deref(),
                password.as_deref(),
            );
            client
                .connect()
                .await
                .map_err(|e| ServerFnError::new(format!("{e}")))?;
            let version = client.get_version().await.ok();
            let torrent_count = client.get_all_info_hashes().await.ok().map(|h| h.len());
            Ok(format!(
                "Connection successful{}{}",
                version
                    .map(|v| format!("; version: {v}"))
                    .unwrap_or_default(),
                torrent_count
                    .map(|c| format!("; torrents: {c}"))
                    .unwrap_or_default()
            ))
        }
        other => Err(ServerFnError::new(format!(
            "unsupported downloader type: {other}"
        ))),
    }
}

#[server]
pub async fn get_downloader_pairs() -> Result<Vec<DownloaderPairInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use std::collections::HashMap;

    let repo = Repository::new(server_pool()?);
    let downloaders = repo
        .list_downloaders()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let names: HashMap<i64, String> = downloaders.into_iter().map(|d| (d.id, d.name)).collect();
    let pairs = repo
        .list_downloader_pairs()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(pairs
        .into_iter()
        .map(|p| DownloaderPairInfo {
            id: p.id,
            name: p.name,
            source_id: p.source_id,
            destination_id: p.destination_id,
            source_name: names
                .get(&p.source_id)
                .cloned()
                .unwrap_or_else(|| p.source_id.to_string()),
            destination_name: names
                .get(&p.destination_id)
                .cloned()
                .unwrap_or_else(|| p.destination_id.to_string()),
        })
        .collect())
}

#[server]
pub async fn create_downloader_pair(
    name: String,
    source_id: i64,
    destination_id: i64,
) -> Result<DownloaderPairInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let id = Repository::new(server_pool()?)
        .create_downloader_pair(&name, source_id, destination_id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    get_downloader_pairs()
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ServerFnError::new("downloader pair created but not found"))
}

#[server]
pub async fn delete_downloader_pair(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_downloader_pair(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn get_folders() -> Result<Vec<FolderInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .list_folders()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|f| FolderInfo {
            id: f.id,
            path: f.path,
            scan_mode: f.scan_mode,
            downloader_id: f.downloader_id,
            enabled: f.enabled,
            last_scanned_at: f.last_scanned_at,
        })
        .collect())
}

#[server]
pub async fn create_folder(
    path: String,
    scan_mode: String,
    downloader_id: Option<i64>,
) -> Result<FolderInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let id = repo
        .create_folder(&path, &scan_mode, downloader_id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    get_folders()
        .await?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| ServerFnError::new("folder created but not found"))
}

#[server]
pub async fn delete_folder(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_folder(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn get_tasks() -> Result<Vec<TaskInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .list_tasks()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|t| TaskInfo {
            id: t.id,
            name: t.name,
            task_type: t.task_type,
            trigger_type: t.trigger_type,
            cron_expression: t.cron_expression,
            status: t.status,
            last_run_at: t.last_run_at,
            next_run_at: t.next_run_at,
            run_count: t.run_count.unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn create_task(
    name: String,
    task_type: String,
    trigger_type: String,
    cron_expression: Option<String>,
) -> Result<TaskInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let id = repo
        .create_task(
            &name,
            &task_type,
            &trigger_type,
            cron_expression.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    get_tasks()
        .await?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| ServerFnError::new("task created but not found"))
}

#[server]
pub async fn delete_task(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_task(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn trigger_task(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .update_task_status(id, "running")
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn get_task_logs(id: i64) -> Result<Vec<TaskLogInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .get_task_logs(id, 50)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|l| TaskLogInfo {
            status: l.status,
            matched_count: l.matched_count.unwrap_or_default(),
            succeeded_count: l.succeeded_count.unwrap_or_default(),
            failed_count: l.failed_count.unwrap_or_default(),
            duration_ms: l.duration_ms,
            created_at: l.created_at,
        })
        .collect())
}

#[server]
pub async fn get_repost_queue(
    status_filter: Option<String>,
) -> Result<Vec<RepostEntry>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let sites: std::collections::HashMap<i64, String> = repo
        .list_sites()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    let entries = repo
        .list_repost_entries(status_filter.as_deref())
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(entries
        .into_iter()
        .map(|e| RepostEntry {
            id: e.id,
            source_site_name: sites
                .get(&e.source_site_id)
                .cloned()
                .unwrap_or_else(|| e.source_site_id.to_string()),
            source_torrent_id: e.source_torrent_id,
            target_site_name: sites
                .get(&e.target_site_id)
                .cloned()
                .unwrap_or_else(|| e.target_site_id.to_string()),
            status: e.status,
            review_notes: e.review_notes,
            submitted_at: e.submitted_at,
            created_at: e.created_at,
        })
        .collect())
}

#[server]
pub async fn review_repost(
    id: i64,
    action: String,
    notes: Option<String>,
) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::repost::models::ReviewAction;
    use pt_reseeder_core::repost::review;

    let action = match action.as_str() {
        "approve" | "approved" => ReviewAction::Approve,
        "reject" | "rejected" => ReviewAction::Reject,
        other => {
            return Err(ServerFnError::new(format!(
                "unknown review action: {other}"
            )))
        }
    };
    let repo = Repository::new(server_pool()?);
    review::review_entry(&repo, id, &action, notes.as_deref())
        .await
        .map(|_| ())
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn submit_repost(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::repost::submitter;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let registry = context.site_registry.read().await.clone();
    submitter::submit_entry(&repo, registry.as_ref(), id)
        .await
        .map(|_| ())
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn delete_repost(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    Repository::new(server_pool()?)
        .delete_repost_entry(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
}

#[server]
pub async fn get_app_config() -> Result<Vec<ConfigEntry>, ServerFnError> {
    let pool = server_pool()?;
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT key, value, updated_at FROM app_config ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|(key, value, updated_at)| ConfigEntry {
            key,
            value,
            updated_at,
        })
        .collect())
}

#[server]
pub async fn update_app_config(key: String, value: String) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let normalized_value = if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => "true".to_string(),
            "false" | "0" => "false".to_string(),
            _ => return Err(ServerFnError::new("做种大小开关的值必须为 true 或 false")),
        }
    } else {
        value
    };

    Repository::new(context.pool.clone())
        .set_config(&key, &normalized_value)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        context.fetch_seeding_size.store(
            normalized_value == "true",
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    Ok(())
}

// ── Logs ────────────────────────────────────────────────────────────────

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

#[server]
pub async fn get_log_files() -> Result<Vec<LogFileInfo>, ServerFnError> {
    let context = server_context()?;
    let log_dir = &context.data_dir;
    // Use config log_dir from app_config table, fallback to "logs"
    let log_dir_path = {
        let repo = pt_reseeder_core::db::repo::Repository::new(context.pool.clone());
        repo.get_config("log_dir")
            .await
            .ok()
            .flatten()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("logs"))
    };
    let _ = log_dir; // suppress unused

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

#[server]
pub async fn get_logs(
    filename: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
    level: Option<String>,
    keyword: Option<String>,
) -> Result<LogPage, ServerFnError> {
    let context = server_context()?;
    let log_dir_path = {
        let repo = pt_reseeder_core::db::repo::Repository::new(context.pool.clone());
        repo.get_config("log_dir")
            .await
            .ok()
            .flatten()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("logs"))
    };

    // Find the log file to read
    let file_path = if let Some(ref name) = filename {
        // Sanitize: prevent directory traversal
        let sanitized = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ServerFnError::new("无效的文件名"))?;
        log_dir_path.join(sanitized)
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
        latest.ok_or_else(|| ServerFnError::new("没有找到日志文件"))?
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
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(100).min(500);

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

// ── Dashboard ───────────────────────────────────────────────────────────

#[server]
pub async fn get_dashboard_data(days: i64) -> Result<DashboardData, ServerFnError> {
    use pt_reseeder_core::stats::reseed::ReseedStatsService;
    use pt_reseeder_core::stats::user_info::UserInfoService;

    let pool: sqlx::SqlitePool = expect_context();

    let reseed_svc = ReseedStatsService::new(pool.clone());
    let user_svc = UserInfoService::new(pool);

    let overview = reseed_svc
        .get_overview()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let site_stats = reseed_svc
        .get_site_reseed_stats()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let trend = reseed_svc
        .get_trend(days)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    let user_info = user_svc
        .get_aggregated_user_info()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    Ok(DashboardData {
        overview: DashboardOverview {
            running_tasks: overview.running_tasks,
            today_success: overview.today_success,
            today_failed: overview.today_failed,
            total_sites: overview.total_sites,
            tracked_torrents: overview.tracked_torrents,
        },
        site_stats: site_stats
            .into_iter()
            .map(|s| SiteReseedStats {
                site_id: s.site_id,
                site_name: s.site_name,
                matched: s.matched,
                succeeded: s.succeeded,
                failed: s.failed,
                skipped: s.skipped,
                success_rate: s.success_rate,
                breaker_status: s.breaker_status,
            })
            .collect(),
        trend: trend
            .into_iter()
            .map(|t| TrendPoint {
                date: t.date,
                succeeded: t.succeeded,
                failed: t.failed,
            })
            .collect(),
        user_info: UserInfoAggregate {
            total_uploaded: user_info.total_uploaded,
            total_downloaded: user_info.total_downloaded,
            total_seeding: user_info.total_seeding,
            total_bonus: user_info.total_bonus,
            site_count: user_info.site_count,
            sites: user_info
                .sites
                .into_iter()
                .map(|s| SiteUserInfo {
                    site_id: s.site_id,
                    site_name: s.site_name,
                    uploaded: s.uploaded,
                    downloaded: s.downloaded,
                    ratio: s.ratio,
                    bonus: s.bonus,
                    user_class: s.user_class,
                    seeding_count: s.seeding_count,
                    leeching_count: s.leeching_count,
                    seeding_size: s.seeding_size,
                    upload_time_seconds: s.upload_time_seconds,
                    fetched_at: s.fetched_at,
                })
                .collect(),
        },
    })
}
