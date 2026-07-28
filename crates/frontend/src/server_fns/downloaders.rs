// Downloaders: DTOs, CRUD server functions and connectivity tests.

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

#[server]
pub async fn get_downloaders() -> Result<Vec<DownloaderInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let rows = Repository::new(server_pool()?)
        .list_downloaders()
        .await?;
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
        .await?;
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
        .map_err(Into::into)
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
        .await?
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
                .await?;
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
                .await?;
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
