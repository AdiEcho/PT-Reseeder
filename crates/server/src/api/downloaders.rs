use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::DownloaderRow;
use pt_reseeder_core::downloader::qbittorrent::QBittorrentClient;
use pt_reseeder_core::downloader::traits::Downloader;
use pt_reseeder_core::downloader::transmission::TransmissionClient;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderInfo {
    pub id: i64,
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    pub role: String,
    pub auto_start: bool,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateDownloaderRequest {
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDownloaderRequest {
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Deserialize)]
pub struct AutoStartRequest {
    pub auto_start: bool,
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

/// Decrypt a stored credential field.
fn decrypt_optional(
    vault: &Vault,
    encrypted: &Option<Vec<u8>>,
    nonce: &Option<Vec<u8>>,
) -> Result<Option<String>, (StatusCode, Json<ApiError>)> {
    let (Some(encrypted), Some(nonce)) = (encrypted.as_ref(), nonce.as_ref()) else {
        return Ok(None);
    };
    let nonce_arr: [u8; 12] = nonce
        .as_slice()
        .try_into()
        .map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "invalid credential nonce"))?;
    let plaintext = vault
        .decrypt(encrypted, &nonce_arr)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("decryption error: {e}")))?;
    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("credential is not UTF-8: {e}")))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_downloaders(State(state): State<AppState>) -> ApiResult<Vec<DownloaderInfo>> {
    let rows = state
        .inner
        .repo
        .list_downloaders()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        rows.into_iter()
            .map(|d| DownloaderInfo {
                id: d.id,
                name: d.name,
                dl_type: d.dl_type,
                host: d.host,
                port: d.port,
                role: d.role,
                auto_start: d.auto_start.unwrap_or(false),
                enabled: d.enabled,
            })
            .collect(),
    ))
}

async fn create_downloader(
    State(state): State<AppState>,
    Json(req): Json<CreateDownloaderRequest>,
) -> ApiResult<DownloaderInfo> {
    // Input validation
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "名称不能为空"));
    }
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "主机地址不能为空"));
    }
    if !(1..=65535).contains(&req.port) {
        return Err(api_err(StatusCode::BAD_REQUEST, "端口必须在 1-65535 范围内"));
    }
    if !matches!(req.dl_type.as_str(), "qbittorrent" | "transmission") {
        return Err(api_err(StatusCode::BAD_REQUEST, "不支持的下载器类型"));
    }
    if !matches!(req.role.as_str(), "source" | "destination" | "both") {
        return Err(api_err(StatusCode::BAD_REQUEST, "无效的用途选项"));
    }

    let (encrypted_username, username_nonce, encrypted_password, password_nonce) = {
        let vault_guard = state.inner.vault.read().await;
        if let Some(vault) = vault_guard.as_ref() {
            let (enc_user, user_nonce) = encrypt_optional(vault, &req.username)?;
            let (enc_pass, pass_nonce) = encrypt_optional(vault, &req.password)?;
            (enc_user, user_nonce, enc_pass, pass_nonce)
        } else {
            (None, None, None, None)
        }
    };

    let row = DownloaderRow {
        id: 0,
        name,
        dl_type: req.dl_type,
        host,
        port: req.port,
        encrypted_username,
        username_nonce,
        encrypted_password,
        password_nonce,
        role: req.role,
        torrent_dir: None,
        default_save_path: None,
        skip_hash_check: Some(true),
        auto_start: Some(req.auto_start),
        tag: Some("PT-Reseeder".into()),
        enabled: true,
        created_at: String::new(),
    };

    let id = state
        .inner
        .repo
        .create_downloader(&row)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Return the newly created downloader
    let created = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "downloader created but not found"))?;

    Ok(Json(DownloaderInfo {
        id: created.id,
        name: created.name,
        dl_type: created.dl_type,
        host: created.host,
        port: created.port,
        role: created.role,
        auto_start: created.auto_start.unwrap_or(false),
        enabled: created.enabled,
    }))
}

async fn update_downloader(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateDownloaderRequest>,
) -> ApiResult<DownloaderInfo> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "名称不能为空"));
    }
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "主机地址不能为空"));
    }
    if !(1..=65535).contains(&req.port) {
        return Err(api_err(StatusCode::BAD_REQUEST, "端口必须在 1-65535 范围内"));
    }
    if !matches!(req.dl_type.as_str(), "qbittorrent" | "transmission") {
        return Err(api_err(StatusCode::BAD_REQUEST, "不支持的下载器类型"));
    }
    if !matches!(req.role.as_str(), "source" | "destination" | "both") {
        return Err(api_err(StatusCode::BAD_REQUEST, "无效的用途选项"));
    }

    let mut row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "下载器不存在"))?;

    row.name = name;
    row.dl_type = req.dl_type;
    row.host = host;
    row.port = req.port;
    row.role = req.role;
    row.auto_start = Some(req.auto_start);

    // Update credentials if provided (empty means keep existing)
    if !req.username.trim().is_empty() || !req.password.trim().is_empty() {
        let vault_guard = state.inner.vault.read().await;
        if let Some(vault) = vault_guard.as_ref() {
            if !req.username.trim().is_empty() {
                let (enc, nonce) = encrypt_optional(vault, &req.username)?;
                row.encrypted_username = enc;
                row.username_nonce = nonce;
            }
            if !req.password.trim().is_empty() {
                let (enc, nonce) = encrypt_optional(vault, &req.password)?;
                row.encrypted_password = enc;
                row.password_nonce = nonce;
            }
        }
    }

    state
        .inner
        .repo
        .update_downloader(&row)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(DownloaderInfo {
        id: row.id,
        name: row.name,
        dl_type: row.dl_type,
        host: row.host,
        port: row.port,
        role: row.role,
        auto_start: row.auto_start.unwrap_or(false),
        enabled: row.enabled,
    }))
}

async fn delete_downloader_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .inner
        .repo
        .delete_downloader(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_downloader_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<String> {
    let row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "downloader not found"))?;

    let vault_guard = state.inner.vault.read().await;
    let vault = vault_guard
        .as_ref()
        .ok_or_else(|| api_err(StatusCode::FORBIDDEN, "凭证已锁定，请重新登录"))?;

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
                .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("连接失败：{e}")))?;
            let version = client.get_version().await.ok();
            let torrent_count = client.get_torrent_count().await.ok();
            Ok(Json(format!(
                "连接成功{}{}",
                version.map(|v| format!("，版本：{v}")).unwrap_or_default(),
                torrent_count
                    .map(|c| format!("，种子数：{c}"))
                    .unwrap_or_default()
            )))
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
                .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("连接失败：{e}")))?;
            let version = client.get_version().await.ok();
            let torrent_count = client.get_all_info_hashes().await.ok().map(|h| h.len());
            Ok(Json(format!(
                "连接成功{}{}",
                version.map(|v| format!("，版本：{v}")).unwrap_or_default(),
                torrent_count
                    .map(|c| format!("，种子数：{c}"))
                    .unwrap_or_default()
            )))
        }
        other => Err(api_err(
            StatusCode::BAD_REQUEST,
            format!("不支持的下载器类型：{other}"),
        )),
    }
}

async fn toggle_auto_start(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<AutoStartRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let mut row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "下载器不存在"))?;

    row.auto_start = Some(req.auto_start);
    state
        .inner
        .repo
        .update_downloader(&row)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloaders", get(list_downloaders).post(create_downloader))
        .route(
            "/downloaders/{id}",
            put(update_downloader).delete(delete_downloader_handler),
        )
        .route("/downloaders/{id}/test", post(test_downloader_handler))
        .route("/downloaders/{id}/auto-start", patch(toggle_auto_start))
}
