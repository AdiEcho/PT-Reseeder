use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::UserStatRecord;

use pt_reseeder_core::site::adapters::gazelle::GazelleAdapter;
use pt_reseeder_core::site::adapters::mteam::MTeamAdapter;
use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
use pt_reseeder_core::site::adapters::unit3d::Unit3dAdapter;
use pt_reseeder_core::site::adapters::zhuque::ZhuqueAdapter;
use pt_reseeder_core::site::definitions::load_all_definitions;
use pt_reseeder_core::site::models::SiteId;
use pt_reseeder_core::site::probe::probe_site as run_site_probe;
use pt_reseeder_core::site::traits::{ReseedCapable, UserInfoCapable};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteInfo {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: String,
    pub auth_type: String,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
    pub download_interval_ms: Option<i64>,
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
pub struct SiteDefinitionInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter: String,
    pub rate_limit_interval_ms: Option<u64>,
    pub rate_limit_burst: Option<u32>,
    pub download_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateSiteResult {
    pub status: String,
    pub message: String,
    pub detail_json: Option<String>,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub api_url: String,
    pub adapter_type: String,
    pub auth_type: String,
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub passkey: String,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
    pub download_interval_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    pub url: String,
    #[serde(default)]
    pub api_url: String,
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub passkey: String,
    pub rate_limit_interval_ms: Option<i64>,
    pub rate_limit_burst: Option<i64>,
    pub download_interval_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateSiteRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub api_url: String,
    pub adapter_type: String,
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub passkey: String,
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Encrypt a plaintext credential; returns `(None, None)` when input is blank.
fn encrypt_optional(
    vault: &Vault,
    value: &str,
) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), (StatusCode, Json<ApiError>)> {
    if value.trim().is_empty() {
        return Ok((None, None));
    }
    let (ciphertext, nonce) = vault
        .encrypt(value.as_bytes())
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("encryption error: {e}")))?;
    Ok((Some(ciphertext), Some(nonce.to_vec())))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_sites(State(state): State<AppState>) -> ApiResult<Vec<SiteInfo>> {
    let sites = state
        .inner
        .repo
        .list_sites()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        sites
            .into_iter()
            .map(|s| SiteInfo {
                id: s.id,
                name: s.name,
                url: s.url,
                api_url: s.api_url,
                adapter_type: s.adapter_type,
                auth_type: s.auth_type,
                rate_limit_interval_ms: s.rate_limit_interval_ms,
                rate_limit_burst: s.rate_limit_burst,
                download_interval_ms: s.download_interval_ms,
                probe_status: s.probe_status,
                probe_detail_json: s.probe_detail_json,
                enabled: s.enabled,
            })
            .collect(),
    ))
}

async fn create_site(
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> ApiResult<SiteInfo> {
    let vault = state
        .inner
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| api_err(StatusCode::FORBIDDEN, "凭证已锁定"))?;

    let adapter = req.adapter_type.to_ascii_lowercase();
    if !matches!(
        adapter.as_str(),
        "nexusphp" | "mteam" | "unit3d" | "gazelle" | "zhuque"
    ) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            format!("不支持的站点架构：{}", req.adapter_type),
        ));
    }

    let rate_interval = req.rate_limit_interval_ms.map(|v| v.max(1)).or(Some(5000));
    let rate_burst = req.rate_limit_burst.map(|v| v.max(1)).or(Some(1));
    let download_interval = req.download_interval_ms.map(|v| v.max(1)).or(Some(5000));

    let api_url_opt = if req.api_url.trim().is_empty() {
        None
    } else {
        Some(req.api_url.trim())
    };

    let id = state
        .inner
        .repo
        .create_site(
            &req.name,
            &req.url,
            api_url_opt,
            &adapter,
            &req.auth_type,
            rate_interval,
            rate_burst,
            download_interval,
        )
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Encrypt and store credentials
    let credential_result = async {
        let (encrypted_cookie, cookie_nonce) = encrypt_optional(&vault, &req.cookie)?;
        let (encrypted_passkey, passkey_nonce) = encrypt_optional(&vault, &req.passkey)?;
        state
            .inner
            .repo
            .update_site_credentials(
                id,
                encrypted_cookie.as_deref(),
                cookie_nonce.as_deref(),
                encrypted_passkey.as_deref(),
                passkey_nonce.as_deref(),
                None,
                None,
            )
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
    }
    .await;

    if let Err(err) = credential_result {
        let _ = state.inner.repo.delete_site(id).await;
        return Err(err);
    }

    // Refresh registry
    let _ = state.refresh_site_registry().await;

    // Background: fetch user info for newly created site
    {
        let site_registry = state.inner.site_registry.clone();
        let repo = state.inner.repo.clone();
        let site_id = id;
        tokio::spawn(async move {
            let registry = site_registry.read().await.clone();
            let handle = registry.get(&SiteId::from(site_id));
            let user_info_cap = handle.and_then(|h| h.user_info.as_ref());
            if let Some(ui) = user_info_cap {
                match ui.fetch_user_info().await {
                    Ok(stats) => {
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
                            tracing::warn!(site_id, %e, "failed to fetch user stats after site creation");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(site_id, %e, "failed to fetch user info after site creation");
                    }
                }
            }
        });
    }

    get_site_info_by_id(&state, id).await
}

async fn get_site_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<SiteDetailData> {
    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let user_stats = state
        .inner
        .repo
        .get_latest_stats_by_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
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

    Ok(Json(SiteDetailData {
        probe_detail: site.probe_detail_json.clone(),
        site: SiteInfo {
            id: site.id,
            name: site.name,
            url: site.url,
            api_url: site.api_url,
            adapter_type: site.adapter_type,
            auth_type: site.auth_type,
            rate_limit_interval_ms: site.rate_limit_interval_ms,
            rate_limit_burst: site.rate_limit_burst,
            download_interval_ms: site.download_interval_ms,
            probe_status: site.probe_status,
            probe_detail_json: site.probe_detail_json,
            enabled: site.enabled,
        },
        user_stats,
    }))
}

async fn update_site(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateSiteRequest>,
) -> ApiResult<SiteInfo> {
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "URL 不能为空"));
    }

    let vault = state
        .inner
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| api_err(StatusCode::FORBIDDEN, "凭证已锁定"))?;

    let repo = &state.inner.repo;

    // Update URL
    let api_url_opt = if req.api_url.trim().is_empty() {
        None
    } else {
        Some(req.api_url.trim().to_string())
    };
    repo.update_site_url(id, &url, api_url_opt.as_deref())
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update rate limits
    let rate_interval = req.rate_limit_interval_ms.map(|v| v.max(1)).or(Some(5000));
    let rate_burst = req.rate_limit_burst.map(|v| v.max(1)).or(Some(1));
    let download_interval = req.download_interval_ms.map(|v| v.max(1)).or(Some(5000));
    repo.update_site_rate_limits(id, rate_interval, rate_burst, download_interval)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update credentials (only if user provided new values; empty means keep existing)
    let cookie_trimmed = req.cookie.trim().to_string();
    let passkey_trimmed = req.passkey.trim().to_string();
    if !cookie_trimmed.is_empty() || !passkey_trimmed.is_empty() {
        let site_row = repo
            .get_site(id)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "站点不存在"))?;

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
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let _ = state.refresh_site_registry().await;
    get_site_info_by_id(&state, id).await
}

async fn delete_site_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .inner
        .repo
        .delete_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = state.refresh_site_registry().await;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_site_handler(
    State(state): State<AppState>,
    Path(_id): Path<i64>,
    Json(req): Json<ValidateSiteRequest>,
) -> ApiResult<ValidateSiteResult> {
    let adapter = req.adapter_type.to_ascii_lowercase();
    let api_url_opt = (!req.api_url.trim().is_empty()).then_some(req.api_url.clone());
    let cookie_opt = (!req.cookie.trim().is_empty()).then_some(req.cookie.clone());
    let passkey_opt = (!req.passkey.trim().is_empty()).then_some(req.passkey.clone());

    let definitions = load_all_definitions(Some(&state.inner.config.data_dir));
    let selectors = definitions
        .get(&req.name)
        .and_then(|def| def.user_info.clone())
        .unwrap_or_default();

    let fetch_seeding_size = state
        .inner
        .fetch_seeding_size
        .load(Ordering::Relaxed);

    type ProbeCapabilities = (
        Option<Arc<dyn ReseedCapable>>,
        Option<Arc<dyn UserInfoCapable>>,
    );

    let (reseed, user_info): ProbeCapabilities = match adapter.as_str() {
        "nexusphp" => {
            let a = Arc::new(
                NexusPhpAdapter::new(
                    req.name,
                    req.url,
                    api_url_opt,
                    cookie_opt,
                    passkey_opt,
                    None,
                    selectors,
                    100,
                )
                .with_fetch_seeding_size(fetch_seeding_size),
            );
            (Some(a.clone()), Some(a))
        }
        "mteam" => {
            let a = Arc::new(MTeamAdapter::new(req.name, req.url, None, passkey_opt, 100));
            (Some(a.clone()), Some(a))
        }
        "unit3d" => {
            let a = Arc::new(Unit3dAdapter::new(req.name, req.url, None, passkey_opt, 100));
            (Some(a.clone()), Some(a))
        }
        "gazelle" => {
            let a = Arc::new(GazelleAdapter::new(req.name, req.url, cookie_opt, passkey_opt, 100));
            (Some(a.clone()), Some(a))
        }
        "zhuque" => {
            let a = Arc::new(ZhuqueAdapter::new(
                req.name,
                req.url,
                None,
                passkey_opt,
                cookie_opt,
                100,
            ));
            (Some(a.clone()), Some(a))
        }
        other => {
            return Ok(Json(ValidateSiteResult {
                status: "failed".to_string(),
                message: format!("不支持的站点架构：{other}"),
                detail_json: None,
            }));
        }
    };

    let probe = run_site_probe(reseed.as_ref(), user_info.as_ref()).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();
    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(Json(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    }))
}

async fn probe_site_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ValidateSiteResult> {
    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let registry = state.inner.site_registry.read().await.clone();
    let handle = registry
        .get(&SiteId::from(site.id))
        .cloned()
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "站点适配器未注册，请确认凭证已解锁且站点架构受支持",
            )
        })?;

    let probe = run_site_probe(handle.reseed.as_ref(), handle.user_info.as_ref()).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();

    state
        .inner
        .repo
        .update_probe_status(id, &status, Some(&detail))
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(Json(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    }))
}

async fn refresh_stats_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Verify site exists
    state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    let registry = state.inner.site_registry.read().await.clone();
    let handle = registry.get(&SiteId::from(id)).cloned().ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "站点适配器未注册，请确认凭证已解锁且站点架构受支持",
        )
    })?;

    let repo = state.inner.repo.clone();
    let site_id = id;
    tokio::spawn(async move {
        if let Some(ui) = handle.user_info.as_ref() {
            match ui.fetch_user_info().await {
                Ok(stats) => {
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
                        tracing::warn!(site_id, %e, "failed to refresh user stats");
                    }
                }
                Err(e) => {
                    tracing::warn!(site_id, %e, "failed to refresh user info");
                }
            }
        }
    });

    Ok(StatusCode::ACCEPTED)
}

async fn list_site_definitions(State(state): State<AppState>) -> ApiResult<Vec<SiteDefinitionInfo>> {
    let definitions = load_all_definitions(Some(&state.inner.config.data_dir));
    let mut results: Vec<SiteDefinitionInfo> = definitions
        .into_values()
        .map(|def| SiteDefinitionInfo {
            id: def.site.id,
            name: def.site.name,
            url: def.site.url,
            api_url: def.site.api_url,
            adapter: def.site.adapter,
            rate_limit_interval_ms: def.site.rate_limit_interval_ms,
            rate_limit_burst: def.site.rate_limit_burst,
            download_interval_ms: def.site.download_interval_ms,
        })
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(results))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn get_site_info_by_id(
    state: &AppState,
    id: i64,
) -> ApiResult<SiteInfo> {
    let site = state
        .inner
        .repo
        .get_site(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "site not found"))?;

    Ok(Json(SiteInfo {
        id: site.id,
        name: site.name,
        url: site.url,
        api_url: site.api_url,
        adapter_type: site.adapter_type,
        auth_type: site.auth_type,
        rate_limit_interval_ms: site.rate_limit_interval_ms,
        rate_limit_burst: site.rate_limit_burst,
        download_interval_ms: site.download_interval_ms,
        probe_status: site.probe_status,
        probe_detail_json: site.probe_detail_json,
        enabled: site.enabled,
    }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sites", get(list_sites).post(create_site))
        .route("/sites/{id}", get(get_site_detail).put(update_site).delete(delete_site_handler))
        .route("/sites/{id}/validate", post(validate_site_handler))
        .route("/sites/{id}/probe", post(probe_site_handler))
        .route("/sites/{id}/refresh-stats", post(refresh_stats_handler))
        .route("/site-definitions", get(list_site_definitions))
}
