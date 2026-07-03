use axum::{
    Router,
    routing::{delete, get, post},
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::state::AppState;
use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::{DownloaderRow, DownloaderPairRow};
use pt_reseeder_core::downloader::qbittorrent::QBittorrentClient;
use pt_reseeder_core::downloader::transmission::TransmissionClient;
use pt_reseeder_core::downloader::traits::Downloader;

// --- Request / Response types ---

#[derive(Deserialize)]
pub struct CreateDownloaderRequest {
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub torrent_dir: Option<String>,
    pub default_save_path: Option<String>,
    pub skip_hash_check: Option<bool>,
    pub auto_start: Option<bool>,
    pub tag: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateDownloaderRequest {
    pub name: Option<String>,
    pub dl_type: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub torrent_dir: Option<String>,
    pub default_save_path: Option<String>,
    pub skip_hash_check: Option<bool>,
    pub auto_start: Option<bool>,
    pub tag: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Serialize)]
pub struct DownloaderResponse {
    pub id: i64,
    pub name: String,
    pub dl_type: String,
    pub host: String,
    pub port: i64,
    pub has_credentials: bool,
    pub role: String,
    pub torrent_dir: Option<String>,
    pub default_save_path: Option<String>,
    pub skip_hash_check: bool,
    pub auto_start: bool,
    pub tag: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct TestConnectionResponse {
    pub success: bool,
    pub message: String,
    pub version: Option<String>,
}

#[derive(Deserialize)]
pub struct CreatePairRequest {
    pub name: String,
    pub source_id: i64,
    pub destination_id: i64,
}

#[derive(Serialize)]
pub struct DownloaderPairResponse {
    pub id: i64,
    pub name: String,
    pub source_id: i64,
    pub destination_id: i64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

// --- Helpers ---

fn row_to_response(row: &DownloaderRow) -> DownloaderResponse {
    DownloaderResponse {
        id: row.id,
        name: row.name.clone(),
        dl_type: row.dl_type.clone(),
        host: row.host.clone(),
        port: row.port,
        has_credentials: row.encrypted_username.is_some() || row.encrypted_password.is_some(),
        role: row.role.clone(),
        torrent_dir: row.torrent_dir.clone(),
        default_save_path: row.default_save_path.clone(),
        skip_hash_check: row.skip_hash_check.unwrap_or(false),
        auto_start: row.auto_start.unwrap_or(true),
        tag: row.tag.clone(),
        enabled: row.enabled,
        created_at: row.created_at.clone(),
    }
}

fn pair_row_to_response(row: &DownloaderPairRow) -> DownloaderPairResponse {
    DownloaderPairResponse {
        id: row.id,
        name: row.name.clone(),
        source_id: row.source_id,
        destination_id: row.destination_id,
        created_at: row.created_at.clone(),
    }
}

/// Decrypt credentials from a DownloaderRow and build a boxed Downloader client.
async fn build_downloader(
    row: &DownloaderRow,
    vault: &Vault,
) -> Result<Box<dyn Downloader>, (StatusCode, Json<ApiError>)> {
    let username = if let (Some(enc), Some(nonce)) = (&row.encrypted_username, &row.username_nonce) {
        let nonce_arr: [u8; 12] = nonce.as_slice().try_into().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "invalid username nonce".to_string(),
                }),
            )
        })?;
        let decrypted = vault.decrypt(enc, &nonce_arr).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("failed to decrypt username: {}", e),
                }),
            )
        })?;
        Some(String::from_utf8(decrypted).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("invalid UTF-8 in username: {}", e),
                }),
            )
        })?)
    } else {
        None
    };

    let password = if let (Some(enc), Some(nonce)) = (&row.encrypted_password, &row.password_nonce) {
        let nonce_arr: [u8; 12] = nonce.as_slice().try_into().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "invalid password nonce".to_string(),
                }),
            )
        })?;
        let decrypted = vault.decrypt(enc, &nonce_arr).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("failed to decrypt password: {}", e),
                }),
            )
        })?;
        Some(String::from_utf8(decrypted).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("invalid UTF-8 in password: {}", e),
                }),
            )
        })?)
    } else {
        None
    };

    match row.dl_type.as_str() {
        "qbittorrent" => {
            let client = QBittorrentClient::new(
                &row.host,
                row.port as u16,
                username.as_deref().unwrap_or(""),
                password.as_deref().unwrap_or(""),
            );
            Ok(Box::new(client))
        }
        "transmission" => {
            let client = TransmissionClient::new(
                &row.host,
                row.port as u16,
                username.as_deref(),
                password.as_deref(),
            );
            Ok(Box::new(client))
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("unsupported downloader type: {}", other),
            }),
        )),
    }
}

/// Get a read-locked vault reference, returning an error if the vault is not unlocked.
async fn get_vault(state: &AppState) -> Result<tokio::sync::RwLockReadGuard<'_, Option<Vault>>, (StatusCode, Json<ApiError>)> {
    let guard = state.inner.vault.read().await;
    if guard.is_none() {
        return Err((
            StatusCode::PRECONDITION_FAILED,
            Json(ApiError {
                error: "vault is locked, please login first".to_string(),
            }),
        ));
    }
    Ok(guard)
}

// --- Handlers ---

/// POST /downloaders
async fn create_downloader(
    State(state): State<AppState>,
    Json(req): Json<CreateDownloaderRequest>,
) -> Result<(StatusCode, Json<DownloaderResponse>), (StatusCode, Json<ApiError>)> {
    // Validate dl_type
    if req.dl_type != "qbittorrent" && req.dl_type != "transmission" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("unsupported dl_type: {}, must be 'qbittorrent' or 'transmission'", req.dl_type),
            }),
        ));
    }

    let vault_guard = get_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    // Encrypt credentials
    let (encrypted_username, username_nonce) = if let Some(ref u) = req.username {
        let (enc, nonce) = vault.encrypt(u.as_bytes()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("failed to encrypt username: {}", e),
                }),
            )
        })?;
        (Some(enc), Some(nonce.to_vec()))
    } else {
        (None, None)
    };

    let (encrypted_password, password_nonce) = if let Some(ref p) = req.password {
        let (enc, nonce) = vault.encrypt(p.as_bytes()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("failed to encrypt password: {}", e),
                }),
            )
        })?;
        (Some(enc), Some(nonce.to_vec()))
    } else {
        (None, None)
    };

    drop(vault_guard);

    let role = req.role.unwrap_or_else(|| "both".to_string());

    let row = DownloaderRow {
        id: 0, // will be set by DB
        name: req.name,
        dl_type: req.dl_type,
        host: req.host,
        port: req.port,
        encrypted_username,
        username_nonce,
        encrypted_password,
        password_nonce,
        role,
        torrent_dir: req.torrent_dir,
        default_save_path: req.default_save_path,
        skip_hash_check: req.skip_hash_check,
        auto_start: req.auto_start,
        tag: req.tag,
        enabled: true,
        created_at: String::new(), // will be set by DB
    };

    let id = state.inner.repo.create_downloader(&row).await.map_err(|e| {
        error!("failed to create downloader: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("database error: {}", e),
            }),
        )
    })?;

    // Fetch back to get DB-generated fields
    let created = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "failed to fetch created downloader".to_string(),
                }),
            )
        })?;

    debug!(id = id, name = %created.name, "downloader created");
    Ok((StatusCode::CREATED, Json(row_to_response(&created))))
}

/// GET /downloaders
async fn list_downloaders(
    State(state): State<AppState>,
) -> Result<Json<Vec<DownloaderResponse>>, (StatusCode, Json<ApiError>)> {
    let rows = state.inner.repo.list_downloaders().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("database error: {}", e),
            }),
        )
    })?;

    Ok(Json(rows.iter().map(row_to_response).collect()))
}

/// GET /downloaders/:id
async fn get_downloader(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DownloaderResponse>, (StatusCode, Json<ApiError>)> {
    let row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("downloader {} not found", id),
                }),
            )
        })?;

    Ok(Json(row_to_response(&row)))
}

/// PUT /downloaders/:id
async fn update_downloader(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateDownloaderRequest>,
) -> Result<Json<DownloaderResponse>, (StatusCode, Json<ApiError>)> {
    let existing = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("downloader {} not found", id),
                }),
            )
        })?;

    // Validate dl_type if provided
    if let Some(ref dt) = req.dl_type {
        if dt != "qbittorrent" && dt != "transmission" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: format!("unsupported dl_type: {}", dt),
                }),
            ));
        }
    }

    // Re-encrypt credentials if they changed
    let (encrypted_username, username_nonce) = if req.username.is_some() {
        let vault_guard = get_vault(&state).await?;
        let vault = vault_guard.as_ref().unwrap();
        if let Some(ref u) = req.username {
            let (enc, nonce) = vault.encrypt(u.as_bytes()).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("failed to encrypt username: {}", e),
                    }),
                )
            })?;
            (Some(enc), Some(nonce.to_vec()))
        } else {
            (existing.encrypted_username.clone(), existing.username_nonce.clone())
        }
    } else {
        (existing.encrypted_username.clone(), existing.username_nonce.clone())
    };

    let (encrypted_password, password_nonce) = if req.password.is_some() {
        let vault_guard = get_vault(&state).await?;
        let vault = vault_guard.as_ref().unwrap();
        if let Some(ref p) = req.password {
            let (enc, nonce) = vault.encrypt(p.as_bytes()).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("failed to encrypt password: {}", e),
                    }),
                )
            })?;
            (Some(enc), Some(nonce.to_vec()))
        } else {
            (existing.encrypted_password.clone(), existing.password_nonce.clone())
        }
    } else {
        (existing.encrypted_password.clone(), existing.password_nonce.clone())
    };

    let updated_row = DownloaderRow {
        id,
        name: req.name.unwrap_or(existing.name),
        dl_type: req.dl_type.unwrap_or(existing.dl_type),
        host: req.host.unwrap_or(existing.host),
        port: req.port.unwrap_or(existing.port),
        encrypted_username,
        username_nonce,
        encrypted_password,
        password_nonce,
        role: req.role.unwrap_or(existing.role),
        torrent_dir: req.torrent_dir.or(existing.torrent_dir),
        default_save_path: req.default_save_path.or(existing.default_save_path),
        skip_hash_check: req.skip_hash_check.or(existing.skip_hash_check),
        auto_start: req.auto_start.or(existing.auto_start),
        tag: req.tag.or(existing.tag),
        enabled: req.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
    };

    state
        .inner
        .repo
        .update_downloader(&updated_row)
        .await
        .map_err(|e| {
            error!("failed to update downloader: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    debug!(id = id, "downloader updated");
    Ok(Json(row_to_response(&updated_row)))
}

/// DELETE /downloaders/:id
async fn delete_downloader(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Verify it exists
    state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("downloader {} not found", id),
                }),
            )
        })?;

    state
        .inner
        .repo
        .delete_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    debug!(id = id, "downloader deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /downloaders/:id/test
async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TestConnectionResponse>, (StatusCode, Json<ApiError>)> {
    let row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("downloader {} not found", id),
                }),
            )
        })?;

    let vault_guard = get_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    let mut client = build_downloader(&row, vault).await?;
    drop(vault_guard);

    // Connect first
    if let Err(e) = client.connect().await {
        return Ok(Json(TestConnectionResponse {
            success: false,
            message: format!("connection failed: {}", e),
            version: None,
        }));
    }

    // Test connection
    match client.test_connection().await {
        Ok(true) => {
            // Try to get version for the response
            let version = match row.dl_type.as_str() {
                "qbittorrent" => {
                    // We know it's connected, try to get version
                    // The test_connection already validated it works
                    None // version is checked internally
                }
                _ => None,
            };
            Ok(Json(TestConnectionResponse {
                success: true,
                message: "connection successful".to_string(),
                version,
            }))
        }
        Ok(false) => Ok(Json(TestConnectionResponse {
            success: false,
            message: "connection test returned false".to_string(),
            version: None,
        })),
        Err(e) => Ok(Json(TestConnectionResponse {
            success: false,
            message: format!("connection test failed: {}", e),
            version: None,
        })),
    }
}

/// GET /downloaders/:id/torrents
async fn list_torrents(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let row = state
        .inner
        .repo
        .get_downloader(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("downloader {} not found", id),
                }),
            )
        })?;

    let vault_guard = get_vault(&state).await?;
    let vault = vault_guard.as_ref().unwrap();

    let mut client = build_downloader(&row, vault).await?;
    drop(vault_guard);

    client.connect().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: format!("failed to connect to downloader: {}", e),
            }),
        )
    })?;

    let hashes = client.get_all_info_hashes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: format!("failed to get torrents: {}", e),
            }),
        )
    })?;

    Ok(Json(hashes.into_iter().collect()))
}

/// POST /downloader-pairs
async fn create_pair(
    State(state): State<AppState>,
    Json(req): Json<CreatePairRequest>,
) -> Result<(StatusCode, Json<DownloaderPairResponse>), (StatusCode, Json<ApiError>)> {
    // Validate that both downloaders exist
    state
        .inner
        .repo
        .get_downloader(req.source_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: format!("source downloader {} not found", req.source_id),
                }),
            )
        })?;

    state
        .inner
        .repo
        .get_downloader(req.destination_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: format!("destination downloader {} not found", req.destination_id),
                }),
            )
        })?;

    let id = state
        .inner
        .repo
        .create_downloader_pair(&req.name, req.source_id, req.destination_id)
        .await
        .map_err(|e| {
            error!("failed to create downloader pair: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    debug!(id = id, name = %req.name, "downloader pair created");
    Ok((
        StatusCode::CREATED,
        Json(DownloaderPairResponse {
            id,
            name: req.name,
            source_id: req.source_id,
            destination_id: req.destination_id,
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
    ))
}

/// GET /downloader-pairs
async fn list_pairs(
    State(state): State<AppState>,
) -> Result<Json<Vec<DownloaderPairResponse>>, (StatusCode, Json<ApiError>)> {
    let rows = state
        .inner
        .repo
        .list_downloader_pairs()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    Ok(Json(rows.iter().map(pair_row_to_response).collect()))
}

/// DELETE /downloader-pairs/:id
async fn delete_pair(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    state
        .inner
        .repo
        .delete_downloader_pair(id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("database error: {}", e),
                }),
            )
        })?;

    debug!(id = id, "downloader pair deleted");
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloaders", post(create_downloader).get(list_downloaders))
        .route(
            "/downloaders/{id}",
            get(get_downloader)
                .put(update_downloader)
                .delete(delete_downloader),
        )
        .route("/downloaders/{id}/test", post(test_connection))
        .route("/downloaders/{id}/torrents", get(list_torrents))
        .route("/downloader-pairs", post(create_pair).get(list_pairs))
        .route("/downloader-pairs/{id}", delete(delete_pair))
}
