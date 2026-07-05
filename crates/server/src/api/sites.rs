use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::{SiteRow, UserStatRecord};
use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
use pt_reseeder_core::site::definitions::load_all_definitions;
use pt_reseeder_core::site::models::UserInfoSelectors;
use pt_reseeder_core::site::probe::probe_site;
use pt_reseeder_core::site::traits::*;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateSiteRequest {
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: Option<String>,
    pub auth_type: Option<String>,
    pub cookie: Option<String>,
    pub passkey: Option<String>,
    pub token: Option<String>,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateCredentialsRequest {
    pub cookie: Option<String>,
    pub passkey: Option<String>,
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct StatsHistoryQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct SiteResponse {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: String,
    pub auth_type: String,
    pub has_cookie: bool,
    pub has_passkey: bool,
    pub has_token: bool,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
    pub probe_status: String,
    pub probe_detail_json: Option<String>,
    pub probed_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct SiteDetailResponse {
    #[serde(flatten)]
    pub site: SiteResponse,
    pub latest_stats: Option<UserStatRecord>,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: i64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn site_to_response(row: &SiteRow) -> SiteResponse {
    SiteResponse {
        id: row.id,
        name: row.name.clone(),
        url: row.url.clone(),
        api_url: row.api_url.clone(),
        adapter_type: row.adapter_type.clone(),
        auth_type: row.auth_type.clone(),
        has_cookie: row.encrypted_cookie.is_some(),
        has_passkey: row.encrypted_passkey.is_some(),
        has_token: row.encrypted_token.is_some(),
        rate_limit_interval_ms: row.rate_limit_interval_ms,
        rate_limit_burst: row.rate_limit_burst,
        probe_status: row.probe_status.clone(),
        probe_detail_json: row.probe_detail_json.clone(),
        probed_at: row.probed_at.clone(),
        enabled: row.enabled,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

/// Encrypt a plaintext credential with the vault, returning (ciphertext, nonce).
fn encrypt_credential(
    vault: &Vault,
    plaintext: &str,
) -> Result<(Vec<u8>, Vec<u8>), (StatusCode, Json<ApiError>)> {
    let (ct, nonce) = vault.encrypt(plaintext.as_bytes()).map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("encryption error: {}", e),
        )
    })?;
    Ok((ct, nonce.to_vec()))
}

/// Decrypt an encrypted credential from the DB using the vault.
fn decrypt_credential(
    vault: &Vault,
    encrypted: &[u8],
    nonce: &[u8],
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let nonce_arr: [u8; 12] = nonce.try_into().map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid nonce length in DB",
        )
    })?;
    let plaintext = vault.decrypt(encrypted, &nonce_arr).map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("decryption error: {}", e),
        )
    })?;
    String::from_utf8(plaintext).map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("credential is not valid UTF-8: {}", e),
        )
    })
}

/// Acquire the vault from app state; returns an error response when locked.
async fn acquire_vault(
    state: &AppState,
) -> Result<tokio::sync::RwLockReadGuard<'_, Option<Vault>>, (StatusCode, Json<ApiError>)> {
    let guard = state.inner.vault.read().await;
    if guard.is_none() {
        return Err(api_err(
            StatusCode::PRECONDITION_FAILED,
            "vault is locked; please log in first",
        ));
    }
    Ok(guard)
}

/// Build a NexusPhpAdapter from a SiteRow + vault-decrypted credentials.
async fn build_adapter(
    site: &SiteRow,
    vault: &Vault,
) -> Result<NexusPhpAdapter, (StatusCode, Json<ApiError>)> {
    let cookie = if let (Some(enc), Some(nonce)) = (&site.encrypted_cookie, &site.cookie_nonce) {
        Some(decrypt_credential(vault, enc, nonce)?)
    } else {
        None
    };

    let passkey = if let (Some(enc), Some(nonce)) = (&site.encrypted_passkey, &site.passkey_nonce) {
        Some(decrypt_credential(vault, enc, nonce)?)
    } else {
        None
    };

    // Try to load site definition for selectors
    let definitions = load_all_definitions(None);
    let selectors = definitions
        .get(&site.name)
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

    let adapter = NexusPhpAdapter::new(
        site.name.clone(),
        site.url.clone(),
        site.api_url.clone(),
        cookie,
        passkey,
        None, // user_id extracted at runtime
        selectors,
        100, // default batch size
    );

    Ok(adapter)
}

/// Run probe against a site and persist the result.
async fn run_probe(
    state: &AppState,
    site: &SiteRow,
    vault: &Vault,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let adapter = build_adapter(site, vault).await?;
    let adapter_arc: Arc<dyn UserInfoCapable> = Arc::new(adapter);

    let probe_result = probe_site(None, Some(&adapter_arc)).await;
    let status_str = probe_result.status_str().to_string();
    let detail_json = probe_result.to_json();

    state
        .inner
        .repo
        .update_probe_status(site.id, &status_str, Some(&detail_json))
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to update probe status: {}", e),
            )
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /sites -- create a new site, encrypt credentials, auto-probe
async fn create_site(
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> Result<(StatusCode, Json<SiteResponse>), (StatusCode, Json<ApiError>)> {
    let vault_guard = acquire_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    let adapter_type = req.adapter_type.as_deref().unwrap_or("nexusphp");
    let auth_type = req.auth_type.as_deref().unwrap_or("cookie");

    // Create site row in DB
    let site_id = state
        .inner
        .repo
        .create_site(
            &req.name,
            &req.url,
            req.api_url.as_deref(),
            adapter_type,
            auth_type,
        )
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?;

    // Encrypt and store credentials
    let mut enc_cookie: Option<Vec<u8>> = None;
    let mut cookie_nonce: Option<Vec<u8>> = None;
    let mut enc_passkey: Option<Vec<u8>> = None;
    let mut passkey_nonce: Option<Vec<u8>> = None;
    let mut enc_token: Option<Vec<u8>> = None;
    let mut token_nonce: Option<Vec<u8>> = None;

    if let Some(ref c) = req.cookie {
        let (ct, n) = encrypt_credential(vault, c)?;
        enc_cookie = Some(ct);
        cookie_nonce = Some(n);
    }
    if let Some(ref p) = req.passkey {
        let (ct, n) = encrypt_credential(vault, p)?;
        enc_passkey = Some(ct);
        passkey_nonce = Some(n);
    }
    if let Some(ref t) = req.token {
        let (ct, n) = encrypt_credential(vault, t)?;
        enc_token = Some(ct);
        token_nonce = Some(n);
    }

    state
        .inner
        .repo
        .update_site_credentials(
            site_id,
            enc_cookie.as_deref(),
            cookie_nonce.as_deref(),
            enc_passkey.as_deref(),
            passkey_nonce.as_deref(),
            enc_token.as_deref(),
            token_nonce.as_deref(),
        )
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to store credentials: {}", e),
            )
        })?;

    // Update rate limit fields if provided
    if req.rate_limit_interval_ms.is_some() || req.rate_limit_burst.is_some() {
        sqlx::query(
            "UPDATE sites SET rate_limit_interval_ms = ?, rate_limit_burst = ?, \
             updated_at = datetime('now') WHERE id = ?",
        )
        .bind(req.rate_limit_interval_ms)
        .bind(req.rate_limit_burst)
        .bind(site_id)
        .execute(&state.inner.db_pool)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to update rate limits: {}", e),
            )
        })?;
    }

    // Fetch the created row
    let site = state
        .inner
        .repo
        .get_site(site_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "site created but not found",
            )
        })?;

    // Auto-probe (best-effort, don't fail the create if probe fails)
    if let Err(e) = run_probe(&state, &site, vault).await {
        warn!("auto-probe failed for site {}: {}", site.name, e.1.error);
    }

    // Re-fetch to include probe results
    let site = state
        .inner
        .repo
        .get_site(site_id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "site created but not found",
            )
        })?;

    state.refresh_site_registry().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to refresh site registry: {}", e),
        )
    })?;

    info!("created site '{}' (id={})", site.name, site.id);
    Ok((StatusCode::CREATED, Json(site_to_response(&site))))
}

/// GET /sites -- list all sites
async fn list_sites(
    State(state): State<AppState>,
) -> Result<Json<Vec<SiteResponse>>, (StatusCode, Json<ApiError>)> {
    let sites = state.inner.repo.list_sites().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {}", e),
        )
    })?;

    let responses: Vec<SiteResponse> = sites.iter().map(site_to_response).collect();
    Ok(Json(responses))
}

/// GET /sites/:id -- get site detail with latest stats
async fn get_site(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SiteDetailResponse>, (StatusCode, Json<ApiError>)> {
    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let latest_stats = state
        .inner
        .repo
        .get_latest_stats_by_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?;

    Ok(Json(SiteDetailResponse {
        site: site_to_response(&site),
        latest_stats,
    }))
}

/// PUT /sites/:id -- update site credentials
async fn update_site(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateCredentialsRequest>,
) -> Result<Json<SiteResponse>, (StatusCode, Json<ApiError>)> {
    let vault_guard = acquire_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    // Verify site exists
    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    // Keep existing encrypted values unless new ones are provided
    let mut enc_cookie = site.encrypted_cookie.clone();
    let mut ck_nonce = site.cookie_nonce.clone();
    let mut enc_passkey = site.encrypted_passkey.clone();
    let mut pk_nonce = site.passkey_nonce.clone();
    let mut enc_token = site.encrypted_token.clone();
    let mut tk_nonce = site.token_nonce.clone();

    if let Some(ref c) = req.cookie {
        let (ct, n) = encrypt_credential(vault, c)?;
        enc_cookie = Some(ct);
        ck_nonce = Some(n);
    }
    if let Some(ref p) = req.passkey {
        let (ct, n) = encrypt_credential(vault, p)?;
        enc_passkey = Some(ct);
        pk_nonce = Some(n);
    }
    if let Some(ref t) = req.token {
        let (ct, n) = encrypt_credential(vault, t)?;
        enc_token = Some(ct);
        tk_nonce = Some(n);
    }

    state
        .inner
        .repo
        .update_site_credentials(
            id,
            enc_cookie.as_deref(),
            ck_nonce.as_deref(),
            enc_passkey.as_deref(),
            pk_nonce.as_deref(),
            enc_token.as_deref(),
            tk_nonce.as_deref(),
        )
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to update credentials: {}", e),
            )
        })?;

    let updated = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found after update"))?;

    state.refresh_site_registry().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to refresh site registry: {}", e),
        )
    })?;

    Ok(Json(site_to_response(&updated)))
}

/// DELETE /sites/:id -- delete a site
async fn delete_site(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Verify site exists
    state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    state.inner.repo.delete_site(id).await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to delete site: {}", e),
        )
    })?;

    state.refresh_site_registry().await.map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to refresh site registry: {}", e),
        )
    })?;

    info!("deleted site id={}", id);
    Ok(StatusCode::NO_CONTENT)
}

/// POST /sites/:id/probe -- manually trigger probe
async fn probe_site_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SiteResponse>, (StatusCode, Json<ApiError>)> {
    let vault_guard = acquire_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    run_probe(&state, &site, vault).await?;

    let updated = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found after probe"))?;

    Ok(Json(site_to_response(&updated)))
}

/// GET /sites/:id/stats -- get stats history
async fn get_stats_history(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<StatsHistoryQuery>,
) -> Result<Json<Vec<UserStatRecord>>, (StatusCode, Json<ApiError>)> {
    // Verify site exists
    state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let limit = query.limit.unwrap_or(50);
    let records = state
        .inner
        .repo
        .get_stats_history(id, limit)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?;

    Ok(Json(records))
}

/// POST /sites/:id/refresh-stats -- immediately fetch and store user stats
async fn refresh_stats(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UserStatRecord>, (StatusCode, Json<ApiError>)> {
    let vault_guard = acquire_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let adapter = build_adapter(&site, vault).await?;

    let user_stats = adapter.fetch_user_info().await.map_err(|e| {
        api_err(
            StatusCode::BAD_GATEWAY,
            format!("failed to fetch user info: {}", e),
        )
    })?;

    let stat_record = UserStatRecord {
        id: 0, // ignored on insert
        site_id: id,
        uploaded: user_stats.uploaded,
        downloaded: user_stats.downloaded,
        ratio: user_stats.ratio,
        bonus: user_stats.bonus,
        user_class: user_stats.user_class,
        seeding_count: user_stats.seeding_count,
        leeching_count: user_stats.leeching_count,
        seeding_size: user_stats.seeding_size,
        upload_time_seconds: user_stats.upload_time_seconds,
        fetched_at: String::new(), // DB default
    };

    state
        .inner
        .repo
        .insert_user_stats(id, &stat_record)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to store stats: {}", e),
            )
        })?;

    // Return the latest stored record
    let latest = state
        .inner
        .repo
        .get_latest_stats_by_site(id)
        .await
        .map_err(|e| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("database error: {}", e),
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "stats inserted but not found",
            )
        })?;

    Ok(Json(latest))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sites", post(create_site))
        .route("/sites", get(list_sites))
        .route("/sites/{id}", get(get_site))
        .route("/sites/{id}", put(update_site))
        .route("/sites/{id}", delete(delete_site))
        .route("/sites/{id}/probe", post(probe_site_handler))
        .route("/sites/{id}/stats", get(get_stats_history))
        .route("/sites/{id}/refresh-stats", post(refresh_stats))
}
