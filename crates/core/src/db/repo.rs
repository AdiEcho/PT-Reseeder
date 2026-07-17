use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;

use super::models::*;
use crate::error::{CoreError, DbError};

/// Keep well under SQLite's ~999 bind-variable limit for `IN (...)` queries.
const SQLITE_IN_CHUNK: usize = 400;

#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // --- Users ---

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        kdf_salt: &[u8],
        wrapped_dek: &[u8],
        dek_nonce: &[u8],
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO users (username, password_hash, kdf_salt, wrapped_dek, dek_nonce) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(username)
        .bind(password_hash)
        .bind(kdf_salt)
        .bind(wrapped_dek)
        .bind(dek_nonce)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, CoreError> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(user)
    }

    pub async fn update_last_login(&self, user_id: i64) -> Result<(), CoreError> {
        sqlx::query("UPDATE users SET last_login_at = datetime('now') WHERE id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn update_password_crypto(
        &self,
        user_id: i64,
        password_hash: &str,
        kdf_salt: &[u8],
        wrapped_dek: &[u8],
        dek_nonce: &[u8],
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE users SET password_hash = ?, kdf_salt = ?, wrapped_dek = ?, dek_nonce = ? \
             WHERE id = ?",
        )
        .bind(password_hash)
        .bind(kdf_salt)
        .bind(wrapped_dek)
        .bind(dek_nonce)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Sessions ---

    pub async fn create_session(
        &self,
        user_id: i64,
        token_hash: &[u8],
        expires_at: &str,
    ) -> Result<i64, CoreError> {
        let result =
            sqlx::query("INSERT INTO sessions (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(token_hash)
                .bind(expires_at)
                .execute(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn find_session_by_hash(
        &self,
        token_hash: &[u8],
    ) -> Result<Option<Session>, CoreError> {
        let session = sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(session)
    }

    pub async fn delete_session(&self, session_id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, CoreError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < datetime('now')")
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(result.rows_affected())
    }

    // --- App Config ---

    pub async fn get_config(&self, key: &str) -> Result<Option<String>, CoreError> {
        let row = sqlx::query_as::<_, AppConfigEntry>("SELECT * FROM app_config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(row.map(|r| r.value))
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, datetime('now')) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Sites ---

    pub async fn create_site(
        &self,
        name: &str,
        url: &str,
        api_url: Option<&str>,
        adapter_type: &str,
        auth_type: &str,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO sites (name, url, api_url, adapter_type, auth_type) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(url)
        .bind(api_url)
        .bind(adapter_type)
        .bind(auth_type)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_site(&self, id: i64) -> Result<Option<SiteRow>, CoreError> {
        let site = sqlx::query_as::<_, SiteRow>("SELECT * FROM sites WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(site)
    }

    pub async fn list_sites(&self) -> Result<Vec<SiteRow>, CoreError> {
        let sites = sqlx::query_as::<_, SiteRow>("SELECT * FROM sites ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(sites)
    }

    pub async fn update_site_credentials(
        &self,
        id: i64,
        encrypted_cookie: Option<&[u8]>,
        cookie_nonce: Option<&[u8]>,
        encrypted_passkey: Option<&[u8]>,
        passkey_nonce: Option<&[u8]>,
        encrypted_token: Option<&[u8]>,
        token_nonce: Option<&[u8]>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE sites SET \
             encrypted_cookie = ?, cookie_nonce = ?, \
             encrypted_passkey = ?, passkey_nonce = ?, \
             encrypted_token = ?, token_nonce = ?, \
             updated_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(encrypted_cookie)
        .bind(cookie_nonce)
        .bind(encrypted_passkey)
        .bind(passkey_nonce)
        .bind(encrypted_token)
        .bind(token_nonce)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn update_probe_status(
        &self,
        id: i64,
        status: &str,
        detail_json: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE sites SET probe_status = ?, probe_detail_json = ?, \
             probed_at = datetime('now'), updated_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(status)
        .bind(detail_json)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn update_site_url(
        &self,
        id: i64,
        url: &str,
        api_url: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE sites SET url = ?, api_url = ?, \
             updated_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(url)
        .bind(api_url)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn delete_site(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM sites WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- User Stats ---

    pub async fn insert_user_stats(
        &self,
        site_id: i64,
        stats: &UserStatRecord,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO user_stats \
             (site_id, uploaded, downloaded, ratio, bonus, user_class, \
              seeding_count, leeching_count, seeding_size, upload_time_seconds) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(site_id)
        .bind(stats.uploaded)
        .bind(stats.downloaded)
        .bind(stats.ratio)
        .bind(stats.bonus)
        .bind(&stats.user_class)
        .bind(stats.seeding_count)
        .bind(stats.leeching_count)
        .bind(stats.seeding_size)
        .bind(stats.upload_time_seconds)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_latest_stats_by_site(
        &self,
        site_id: i64,
    ) -> Result<Option<UserStatRecord>, CoreError> {
        let record = sqlx::query_as::<_, UserStatRecord>(
            "SELECT * FROM user_stats WHERE site_id = ? ORDER BY fetched_at DESC LIMIT 1",
        )
        .bind(site_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(record)
    }

    pub async fn get_stats_history(
        &self,
        site_id: i64,
        limit: i64,
    ) -> Result<Vec<UserStatRecord>, CoreError> {
        let records = sqlx::query_as::<_, UserStatRecord>(
            "SELECT * FROM user_stats WHERE site_id = ? ORDER BY fetched_at DESC LIMIT ?",
        )
        .bind(site_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(records)
    }

    // --- Downloaders ---

    pub async fn create_downloader(&self, row: &DownloaderRow) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO downloaders \
             (name, dl_type, host, port, encrypted_username, username_nonce, \
              encrypted_password, password_nonce, role, torrent_dir, default_save_path, \
              skip_hash_check, auto_start, tag, enabled) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.name)
        .bind(&row.dl_type)
        .bind(&row.host)
        .bind(row.port)
        .bind(&row.encrypted_username)
        .bind(&row.username_nonce)
        .bind(&row.encrypted_password)
        .bind(&row.password_nonce)
        .bind(&row.role)
        .bind(&row.torrent_dir)
        .bind(&row.default_save_path)
        .bind(row.skip_hash_check)
        .bind(row.auto_start)
        .bind(&row.tag)
        .bind(row.enabled)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_downloader(&self, id: i64) -> Result<Option<DownloaderRow>, CoreError> {
        let row = sqlx::query_as::<_, DownloaderRow>("SELECT * FROM downloaders WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    pub async fn list_downloaders(&self) -> Result<Vec<DownloaderRow>, CoreError> {
        let rows = sqlx::query_as::<_, DownloaderRow>("SELECT * FROM downloaders ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    pub async fn update_downloader(&self, row: &DownloaderRow) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE downloaders SET \
             name = ?, dl_type = ?, host = ?, port = ?, \
             encrypted_username = ?, username_nonce = ?, \
             encrypted_password = ?, password_nonce = ?, \
             role = ?, torrent_dir = ?, default_save_path = ?, \
             skip_hash_check = ?, auto_start = ?, tag = ?, enabled = ? \
             WHERE id = ?",
        )
        .bind(&row.name)
        .bind(&row.dl_type)
        .bind(&row.host)
        .bind(row.port)
        .bind(&row.encrypted_username)
        .bind(&row.username_nonce)
        .bind(&row.encrypted_password)
        .bind(&row.password_nonce)
        .bind(&row.role)
        .bind(&row.torrent_dir)
        .bind(&row.default_save_path)
        .bind(row.skip_hash_check)
        .bind(row.auto_start)
        .bind(&row.tag)
        .bind(row.enabled)
        .bind(row.id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn delete_downloader(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM downloaders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Pieces Cache ---

    pub async fn upsert_pieces_cache(&self, entry: &PiecesCacheEntry) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO pieces_cache \
             (pieces_hash, info_hash, torrent_name, file_path, total_size, announce_url) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(info_hash) DO UPDATE SET \
             pieces_hash = excluded.pieces_hash, \
             torrent_name = excluded.torrent_name, \
             file_path = excluded.file_path, \
             total_size = excluded.total_size, \
             announce_url = excluded.announce_url, \
             cached_at = datetime('now')",
        )
        .bind(&entry.pieces_hash)
        .bind(&entry.info_hash)
        .bind(&entry.torrent_name)
        .bind(&entry.file_path)
        .bind(entry.total_size)
        .bind(&entry.announce_url)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn find_by_pieces_hash(
        &self,
        pieces_hash: &str,
    ) -> Result<Vec<PiecesCacheEntry>, CoreError> {
        let rows = sqlx::query_as::<_, PiecesCacheEntry>(
            "SELECT * FROM pieces_cache WHERE pieces_hash = ?",
        )
        .bind(pieces_hash)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    pub async fn find_by_info_hash(
        &self,
        info_hash: &str,
    ) -> Result<Option<PiecesCacheEntry>, CoreError> {
        let row =
            sqlx::query_as::<_, PiecesCacheEntry>("SELECT * FROM pieces_cache WHERE info_hash = ?")
                .bind(info_hash)
                .fetch_optional(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    /// Batch lookup: which of the given info_hashes already exist in pieces_cache.
    pub async fn find_existing_info_hashes(
        &self,
        info_hashes: &[String],
    ) -> Result<HashSet<String>, CoreError> {
        let mut existing = HashSet::new();
        if info_hashes.is_empty() {
            return Ok(existing);
        }

        for chunk in info_hashes.chunks(SQLITE_IN_CHUNK) {
            let mut qb =
                sqlx::QueryBuilder::new("SELECT info_hash FROM pieces_cache WHERE info_hash IN (");
            {
                let mut separated = qb.separated(", ");
                for hash in chunk {
                    separated.push_bind(hash);
                }
            }
            qb.push(")");

            let rows: Vec<(String,)> = qb
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
            existing.extend(rows.into_iter().map(|r| r.0));
        }

        Ok(existing)
    }

    /// Batch load announce URLs grouped by pieces_hash.
    pub async fn find_announce_urls_by_pieces_hashes(
        &self,
        pieces_hashes: &[String],
    ) -> Result<HashMap<String, HashSet<String>>, CoreError> {
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        if pieces_hashes.is_empty() {
            return Ok(map);
        }

        for chunk in pieces_hashes.chunks(SQLITE_IN_CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT pieces_hash, announce_url FROM pieces_cache WHERE pieces_hash IN (",
            );
            {
                let mut separated = qb.separated(", ");
                for hash in chunk {
                    separated.push_bind(hash);
                }
            }
            qb.push(")");

            let rows: Vec<(String, Option<String>)> = qb
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;

            for (pieces_hash, announce_url) in rows {
                if let Some(url) = announce_url {
                    map.entry(pieces_hash).or_default().insert(url);
                } else {
                    map.entry(pieces_hash).or_default();
                }
            }
        }

        // Ensure every requested hash has an entry (even if empty) for easy lookup.
        for hash in pieces_hashes {
            map.entry(hash.clone()).or_default();
        }

        Ok(map)
    }

    // --- Reseed History ---

    pub async fn insert_reseed_history(
        &self,
        pieces_hash: &str,
        site_id: i64,
        torrent_id: Option<i64>,
        info_hash: Option<&str>,
        status: &str,
        error_reason: Option<&str>,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO reseed_history \
             (pieces_hash, site_id, torrent_id, info_hash, status, error_reason) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(pieces_hash)
        .bind(site_id)
        .bind(torrent_id)
        .bind(info_hash)
        .bind(status)
        .bind(error_reason)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn find_reseed_history(
        &self,
        pieces_hash: &str,
        site_id: i64,
    ) -> Result<Vec<ReseedHistoryEntry>, CoreError> {
        let rows = sqlx::query_as::<_, ReseedHistoryEntry>(
            "SELECT * FROM reseed_history WHERE pieces_hash = ? AND site_id = ? \
             ORDER BY created_at DESC",
        )
        .bind(pieces_hash)
        .bind(site_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    /// Batch load successful reseed history info_hashes for a site, keyed by pieces_hash.
    pub async fn find_successful_reseed_info_hashes(
        &self,
        pieces_hashes: &[String],
        site_id: i64,
    ) -> Result<HashMap<String, HashSet<String>>, CoreError> {
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        if pieces_hashes.is_empty() {
            return Ok(map);
        }

        for chunk in pieces_hashes.chunks(SQLITE_IN_CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT pieces_hash, info_hash FROM reseed_history \
                 WHERE site_id = ",
            );
            qb.push_bind(site_id);
            qb.push(" AND status = 'success' AND pieces_hash IN (");
            {
                let mut separated = qb.separated(", ");
                for hash in chunk {
                    separated.push_bind(hash);
                }
            }
            qb.push(")");

            let rows: Vec<(String, Option<String>)> = qb
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;

            for (pieces_hash, info_hash) in rows {
                if let Some(ih) = info_hash {
                    map.entry(pieces_hash).or_default().insert(ih);
                }
            }
        }

        Ok(map)
    }

    // --- Folders ---

    pub async fn create_folder(
        &self,
        path: &str,
        scan_mode: &str,
        downloader_id: Option<i64>,
    ) -> Result<i64, CoreError> {
        let result =
            sqlx::query("INSERT INTO folders (path, scan_mode, downloader_id) VALUES (?, ?, ?)")
                .bind(path)
                .bind(scan_mode)
                .bind(downloader_id)
                .execute(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn list_folders(&self) -> Result<Vec<FolderRow>, CoreError> {
        let rows = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    pub async fn get_folder(&self, id: i64) -> Result<Option<FolderRow>, CoreError> {
        let row = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    pub async fn update_folder(
        &self,
        id: i64,
        path: &str,
        scan_mode: &str,
        downloader_id: Option<i64>,
        enabled: bool,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE folders SET path = ?, scan_mode = ?, downloader_id = ?, enabled = ? \
             WHERE id = ?",
        )
        .bind(path)
        .bind(scan_mode)
        .bind(downloader_id)
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn update_folder_scanned(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("UPDATE folders SET last_scanned_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn delete_folder(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Task-Folder Associations ---

    pub async fn set_task_folders(
        &self,
        task_id: i64,
        folder_ids: &[i64],
    ) -> Result<(), CoreError> {
        let mut tx = self.pool.begin().await.map_err(DbError::Sqlx)?;
        sqlx::query("DELETE FROM task_folders WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(DbError::Sqlx)?;

        if !folder_ids.is_empty() {
            for chunk in folder_ids.chunks(SQLITE_IN_CHUNK) {
                let mut qb =
                    sqlx::QueryBuilder::new("INSERT INTO task_folders (task_id, folder_id) ");
                qb.push_values(chunk, |mut b, &folder_id| {
                    b.push_bind(task_id).push_bind(folder_id);
                });
                qb.build()
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
            }
        }

        tx.commit().await.map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn get_task_folders(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT folder_id FROM task_folders WHERE task_id = ?")
                .bind(task_id)
                .fetch_all(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    // --- Task-Site Associations ---

    pub async fn set_task_sites(&self, task_id: i64, site_ids: &[i64]) -> Result<(), CoreError> {
        let mut tx = self.pool.begin().await.map_err(DbError::Sqlx)?;
        sqlx::query("DELETE FROM task_sites WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(DbError::Sqlx)?;

        if !site_ids.is_empty() {
            for chunk in site_ids.chunks(SQLITE_IN_CHUNK) {
                let mut qb =
                    sqlx::QueryBuilder::new("INSERT INTO task_sites (task_id, site_id) ");
                qb.push_values(chunk, |mut b, &site_id| {
                    b.push_bind(task_id).push_bind(site_id);
                });
                qb.build()
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
            }
        }

        tx.commit().await.map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn get_task_sites(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT site_id FROM task_sites WHERE task_id = ?")
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    // --- Task-Source-Downloader Associations ---

    pub async fn set_task_source_downloaders(
        &self,
        task_id: i64,
        downloader_ids: &[i64],
    ) -> Result<(), CoreError> {
        let mut tx = self.pool.begin().await.map_err(DbError::Sqlx)?;
        sqlx::query("DELETE FROM task_source_downloaders WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(DbError::Sqlx)?;

        if !downloader_ids.is_empty() {
            for chunk in downloader_ids.chunks(SQLITE_IN_CHUNK) {
                let mut qb = sqlx::QueryBuilder::new(
                    "INSERT INTO task_source_downloaders (task_id, downloader_id) ",
                );
                qb.push_values(chunk, |mut b, &downloader_id| {
                    b.push_bind(task_id).push_bind(downloader_id);
                });
                qb.build()
                    .execute(&mut *tx)
                    .await
                    .map_err(DbError::Sqlx)?;
            }
        }

        tx.commit().await.map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn get_task_source_downloaders(&self, task_id: i64) -> Result<Vec<i64>, CoreError> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT downloader_id FROM task_source_downloaders WHERE task_id = ?")
                .bind(task_id)
                .fetch_all(&self.pool)
                .await
                .map_err(DbError::Sqlx)?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    // --- Tasks ---

    pub async fn create_task(
        &self,
        name: &str,
        task_type: &str,
        trigger_type: &str,
        cron_expression: Option<&str>,
        destination_downloader_id: Option<i64>,
        config_json: Option<&str>,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO tasks \
             (name, task_type, trigger_type, cron_expression, \
              destination_downloader_id, config_json) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(task_type)
        .bind(trigger_type)
        .bind(cron_expression)
        .bind(destination_downloader_id)
        .bind(config_json)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_task(&self, id: i64) -> Result<Option<TaskRow>, CoreError> {
        let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskRow>, CoreError> {
        let rows = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    pub async fn update_task_status(&self, id: i64, status: &str) -> Result<(), CoreError> {
        sqlx::query("UPDATE tasks SET status = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn recover_interrupted_tasks(&self) -> Result<u64, CoreError> {
        let result = sqlx::query(
            "UPDATE tasks SET status = 'error', updated_at = datetime('now') WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.rows_affected())
    }

    pub async fn try_mark_task_running(&self, id: i64) -> Result<bool, CoreError> {
        let result = sqlx::query(
            "UPDATE tasks SET status = 'running', updated_at = datetime('now') \
             WHERE id = ? AND status != 'running'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_task_next_run_at(
        &self,
        id: i64,
        next_run_at: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query("UPDATE tasks SET next_run_at = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(next_run_at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn update_task_run_times(
        &self,
        id: i64,
        last_run_at: &str,
        next_run_at: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE tasks SET last_run_at = ?, next_run_at = ?, \
             run_count = COALESCE(run_count, 0) + 1, \
             updated_at = datetime('now') WHERE id = ?",
        )
        .bind(last_run_at)
        .bind(next_run_at)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    /// Update a task's core fields in-place (preserves task ID).
    pub async fn update_task(
        &self,
        id: i64,
        name: &str,
        task_type: &str,
        trigger_type: &str,
        cron_expression: Option<&str>,
        destination_downloader_id: Option<i64>,
        config_json: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            r#"UPDATE tasks
               SET name = ?1, task_type = ?2, trigger_type = ?3,
                   cron_expression = ?4,
                   destination_downloader_id = ?5, config_json = ?6,
                   updated_at = datetime('now')
               WHERE id = ?7"#,
        )
        .bind(name)
        .bind(task_type)
        .bind(trigger_type)
        .bind(cron_expression)
        .bind(destination_downloader_id)
        .bind(config_json)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::Db(DbError::Sqlx(e)))?;
        Ok(())
    }

    pub async fn delete_task(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Task Logs ---

    pub async fn insert_task_log(
        &self,
        task_id: i64,
        status: &str,
        matched: i64,
        succeeded: i64,
        failed: i64,
        duration_ms: Option<i64>,
        log_text: Option<&str>,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO task_logs \
             (task_id, status, matched_count, succeeded_count, failed_count, \
              duration_ms, log_text) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task_id)
        .bind(status)
        .bind(matched)
        .bind(succeeded)
        .bind(failed)
        .bind(duration_ms)
        .bind(log_text)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_task_logs(&self, task_id: i64, limit: i64) -> Result<Vec<TaskLog>, CoreError> {
        let rows = sqlx::query_as::<_, TaskLog>(
            "SELECT * FROM task_logs WHERE task_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(task_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    // --- Repost Queue ---

    pub async fn create_repost_entry(
        &self,
        source_site_id: i64,
        source_torrent_id: &str,
        target_site_id: i64,
        raw_info_json: &str,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO repost_queue \
             (source_site_id, source_torrent_id, target_site_id, raw_info_json) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(source_site_id)
        .bind(source_torrent_id)
        .bind(target_site_id)
        .bind(raw_info_json)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn get_repost_entry(&self, id: i64) -> Result<Option<RepostQueueEntry>, CoreError> {
        let row = sqlx::query_as::<_, RepostQueueEntry>("SELECT * FROM repost_queue WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    pub async fn list_repost_entries(
        &self,
        status_filter: Option<&str>,
    ) -> Result<Vec<RepostQueueEntry>, CoreError> {
        let rows = if let Some(status) = status_filter {
            sqlx::query_as::<_, RepostQueueEntry>(
                "SELECT * FROM repost_queue WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(status)
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?
        } else {
            sqlx::query_as::<_, RepostQueueEntry>(
                "SELECT * FROM repost_queue ORDER BY created_at DESC",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Sqlx)?
        };
        Ok(rows)
    }

    pub async fn update_repost_status(
        &self,
        id: i64,
        status: &str,
        review_notes: Option<&str>,
        adapted_info_json: Option<&str>,
        submitted_at: Option<&str>,
    ) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE repost_queue SET \
             status = ?, review_notes = ?, adapted_info_json = ?, submitted_at = ? \
             WHERE id = ?",
        )
        .bind(status)
        .bind(review_notes)
        .bind(adapted_info_json)
        .bind(submitted_at)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(())
    }

    pub async fn delete_repost_entry(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM repost_queue WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    async fn setup_repo() -> Repository {
        let pool = init_db("sqlite::memory:").await.unwrap();
        Repository::new(pool)
    }

    #[tokio::test]
    async fn create_and_find_user_by_username() {
        let repo = setup_repo().await;
        let id = repo
            .create_user("alice", "hash123", b"salt", b"wdek", b"nonce")
            .await
            .unwrap();
        assert!(id > 0);

        let user = repo.find_user_by_username("alice").await.unwrap().unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.password_hash, "hash123");
        assert_eq!(user.kdf_salt, b"salt");
    }

    #[tokio::test]
    async fn find_user_by_username_returns_none_when_missing() {
        let repo = setup_repo().await;
        let user = repo.find_user_by_username("nobody").await.unwrap();
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn update_last_login_sets_timestamp() {
        let repo = setup_repo().await;
        let id = repo
            .create_user("bob", "hash", b"s", b"w", b"n")
            .await
            .unwrap();
        repo.update_last_login(id).await.unwrap();
        let user = repo.find_user_by_username("bob").await.unwrap().unwrap();
        assert!(user.last_login_at.is_some());
    }

    #[tokio::test]
    async fn update_password_crypto_changes_fields() {
        let repo = setup_repo().await;
        let id = repo
            .create_user("carol", "old_hash", b"old_salt", b"old_dek", b"old_nonce")
            .await
            .unwrap();
        repo.update_password_crypto(id, "new_hash", b"new_salt", b"new_dek", b"new_nonce")
            .await
            .unwrap();
        let user = repo.find_user_by_username("carol").await.unwrap().unwrap();
        assert_eq!(user.password_hash, "new_hash");
        assert_eq!(user.kdf_salt, b"new_salt");
    }

    #[tokio::test]
    async fn create_and_find_session() {
        let repo = setup_repo().await;
        let uid = repo.create_user("u", "h", b"s", b"w", b"n").await.unwrap();
        let token_hash = b"session_token_hash";
        let sid = repo
            .create_session(uid, token_hash, "2099-01-01 00:00:00")
            .await
            .unwrap();
        assert!(sid > 0);

        let session = repo
            .find_session_by_hash(token_hash)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.user_id, uid);
    }

    #[tokio::test]
    async fn delete_session_removes_row() {
        let repo = setup_repo().await;
        let uid = repo.create_user("u2", "h", b"s", b"w", b"n").await.unwrap();
        let sid = repo
            .create_session(uid, b"tok", "2099-01-01 00:00:00")
            .await
            .unwrap();
        repo.delete_session(sid).await.unwrap();
        let found = repo.find_session_by_hash(b"tok").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn set_and_get_config() {
        let repo = setup_repo().await;
        repo.set_config("theme", "dark").await.unwrap();
        let val = repo.get_config("theme").await.unwrap().unwrap();
        assert_eq!(val, "dark");
    }

    #[tokio::test]
    async fn set_config_upserts_existing_key() {
        let repo = setup_repo().await;
        repo.set_config("k", "v1").await.unwrap();
        repo.set_config("k", "v2").await.unwrap();
        let val = repo.get_config("k").await.unwrap().unwrap();
        assert_eq!(val, "v2");
    }

    #[tokio::test]
    async fn get_config_returns_none_for_missing_key() {
        let repo = setup_repo().await;
        let val = repo.get_config("nonexistent").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn create_and_get_site() {
        let repo = setup_repo().await;
        let id = repo
            .create_site(
                "HDSky",
                "https://hdsky.me",
                Some("https://hdsky.me/api"),
                "nexusphp",
                "cookie",
            )
            .await
            .unwrap();
        let site = repo.get_site(id).await.unwrap().unwrap();
        assert_eq!(site.name, "HDSky");
        assert_eq!(site.url, "https://hdsky.me");
        assert_eq!(site.api_url, Some("https://hdsky.me/api".to_string()));
        assert_eq!(site.adapter_type, "nexusphp");
    }

    #[tokio::test]
    async fn list_sites_returns_all_sites() {
        let repo = setup_repo().await;
        repo.create_site("Site1", "http://1", None, "np", "cookie")
            .await
            .unwrap();
        repo.create_site("Site2", "http://2", None, "np", "cookie")
            .await
            .unwrap();
        let sites = repo.list_sites().await.unwrap();
        assert_eq!(sites.len(), 2);
    }

    #[tokio::test]
    async fn delete_site_removes_row() {
        let repo = setup_repo().await;
        let id = repo
            .create_site("ToDelete", "http://x", None, "np", "cookie")
            .await
            .unwrap();
        repo.delete_site(id).await.unwrap();
        let site = repo.get_site(id).await.unwrap();
        assert!(site.is_none());
    }

    #[tokio::test]
    async fn create_and_get_downloader() {
        let repo = setup_repo().await;
        let row = DownloaderRow {
            id: 0,
            name: "qb1".to_string(),
            dl_type: "qbittorrent".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            encrypted_username: None,
            username_nonce: None,
            encrypted_password: None,
            password_nonce: None,
            role: "both".to_string(),
            torrent_dir: None,
            default_save_path: Some("/downloads".to_string()),
            skip_hash_check: Some(true),
            auto_start: Some(true),
            tag: Some("PT-Reseeder".to_string()),
            enabled: true,
            created_at: String::new(),
        };
        let id = repo.create_downloader(&row).await.unwrap();
        let fetched = repo.get_downloader(id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "qb1");
        assert_eq!(fetched.host, "localhost");
        assert_eq!(fetched.port, 8080);
    }

    #[tokio::test]
    async fn list_downloaders_returns_all() {
        let repo = setup_repo().await;
        let row = DownloaderRow {
            id: 0,
            name: "dl".to_string(),
            dl_type: "qbittorrent".to_string(),
            host: "h".to_string(),
            port: 1,
            encrypted_username: None,
            username_nonce: None,
            encrypted_password: None,
            password_nonce: None,
            role: "both".to_string(),
            torrent_dir: None,
            default_save_path: None,
            skip_hash_check: None,
            auto_start: None,
            tag: None,
            enabled: true,
            created_at: String::new(),
        };
        repo.create_downloader(&row).await.unwrap();
        let list = repo.list_downloaders().await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn delete_downloader_removes_row() {
        let repo = setup_repo().await;
        let row = DownloaderRow {
            id: 0,
            name: "del".to_string(),
            dl_type: "transmission".to_string(),
            host: "h".to_string(),
            port: 9091,
            encrypted_username: None,
            username_nonce: None,
            encrypted_password: None,
            password_nonce: None,
            role: "source".to_string(),
            torrent_dir: None,
            default_save_path: None,
            skip_hash_check: None,
            auto_start: None,
            tag: None,
            enabled: true,
            created_at: String::new(),
        };
        let id = repo.create_downloader(&row).await.unwrap();
        repo.delete_downloader(id).await.unwrap();
        let fetched = repo.get_downloader(id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn upsert_and_find_pieces_cache() {
        let repo = setup_repo().await;
        let entry = PiecesCacheEntry {
            id: 0,
            pieces_hash: "phash1".to_string(),
            info_hash: "ihash1".to_string(),
            torrent_name: Some("torrent.mkv".to_string()),
            file_path: Some("/path/to/file".to_string()),
            total_size: Some(1024),
            announce_url: Some("http://tracker".to_string()),
            cached_at: String::new(),
        };
        repo.upsert_pieces_cache(&entry).await.unwrap();

        let found = repo.find_by_pieces_hash("phash1").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].info_hash, "ihash1");

        let by_info = repo.find_by_info_hash("ihash1").await.unwrap().unwrap();
        assert_eq!(by_info.pieces_hash, "phash1");
    }

    #[tokio::test]
    async fn upsert_pieces_cache_updates_on_conflict() {
        let repo = setup_repo().await;
        let entry1 = PiecesCacheEntry {
            id: 0,
            pieces_hash: "ph_old".to_string(),
            info_hash: "ih_same".to_string(),
            torrent_name: Some("old.mkv".to_string()),
            file_path: None,
            total_size: Some(100),
            announce_url: None,
            cached_at: String::new(),
        };
        repo.upsert_pieces_cache(&entry1).await.unwrap();

        let entry2 = PiecesCacheEntry {
            id: 0,
            pieces_hash: "ph_new".to_string(),
            info_hash: "ih_same".to_string(),
            torrent_name: Some("new.mkv".to_string()),
            file_path: None,
            total_size: Some(200),
            announce_url: None,
            cached_at: String::new(),
        };
        repo.upsert_pieces_cache(&entry2).await.unwrap();

        let found = repo.find_by_info_hash("ih_same").await.unwrap().unwrap();
        assert_eq!(found.pieces_hash, "ph_new");
        assert_eq!(found.torrent_name, Some("new.mkv".to_string()));
    }

    #[tokio::test]
    async fn insert_and_find_reseed_history() {
        let repo = setup_repo().await;
        let site_id = repo
            .create_site("S", "http://s", None, "np", "cookie")
            .await
            .unwrap();
        repo.insert_reseed_history("ph", site_id, Some(42), Some("ih"), "success", None)
            .await
            .unwrap();
        let history = repo.find_reseed_history("ph", site_id).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "success");
        assert_eq!(history[0].torrent_id, Some(42));
    }

    #[tokio::test]
    async fn create_and_list_tasks() {
        let repo = setup_repo().await;
        repo.create_task("task1", "reseed", "cron", Some("0 * * * *"), None, None)
            .await
            .unwrap();
        let tasks = repo.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "task1");
        assert_eq!(tasks[0].status, "idle");
    }

    #[tokio::test]
    async fn recover_interrupted_tasks_marks_running_tasks_as_error() {
        let repo = setup_repo().await;
        let running_id = repo
            .create_task("running", "sync_stats", "manual", None, None, None)
            .await
            .unwrap();
        let idle_id = repo
            .create_task("idle", "sync_stats", "manual", None, None, None)
            .await
            .unwrap();
        repo.update_task_status(running_id, "running").await.unwrap();

        assert_eq!(repo.recover_interrupted_tasks().await.unwrap(), 1);
        assert_eq!(repo.get_task(running_id).await.unwrap().unwrap().status, "error");
        assert_eq!(repo.get_task(idle_id).await.unwrap().unwrap().status, "idle");
    }

    #[tokio::test]
    async fn try_mark_task_running_succeeds_when_idle() {
        let repo = setup_repo().await;
        let id = repo
            .create_task("t", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        let marked = repo.try_mark_task_running(id).await.unwrap();
        assert!(marked);

        // Second attempt should fail (already running)
        let marked2 = repo.try_mark_task_running(id).await.unwrap();
        assert!(!marked2);
    }

    #[tokio::test]
    async fn delete_task_removes_row() {
        let repo = setup_repo().await;
        let id = repo
            .create_task("del", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        repo.delete_task(id).await.unwrap();
        let fetched = repo.get_task(id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn insert_and_get_task_logs() {
        let repo = setup_repo().await;
        let task_id = repo
            .create_task("logged", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        repo.insert_task_log(task_id, "success", 10, 8, 2, Some(500), Some("log text"))
            .await
            .unwrap();

        let logs = repo.get_task_logs(task_id, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].matched_count, Some(10));
        assert_eq!(logs[0].succeeded_count, Some(8));
        assert_eq!(logs[0].failed_count, Some(2));
    }

    #[tokio::test]
    async fn create_and_list_folders() {
        let repo = setup_repo().await;
        repo.create_folder("/data/torrents", "local", None)
            .await
            .unwrap();
        let folders = repo.list_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, "/data/torrents");
        assert_eq!(folders[0].scan_mode, "local");
    }

    #[tokio::test]
    async fn delete_folder_removes_row() {
        let repo = setup_repo().await;
        let id = repo
            .create_folder("/tmp/fold", "local", None)
            .await
            .unwrap();
        repo.delete_folder(id).await.unwrap();
        let fetched = repo.get_folder(id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn set_and_get_task_folders() {
        let repo = setup_repo().await;
        let tid = repo
            .create_task("tf", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        let f1 = repo.create_folder("/a", "local", None).await.unwrap();
        let f2 = repo.create_folder("/b", "local", None).await.unwrap();

        repo.set_task_folders(tid, &[f1, f2]).await.unwrap();
        let mut folders = repo.get_task_folders(tid).await.unwrap();
        folders.sort();
        assert_eq!(folders, vec![f1, f2]);
    }

    #[tokio::test]
    async fn set_and_get_task_sites() {
        let repo = setup_repo().await;
        let tid = repo
            .create_task("ts", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        let s1 = repo
            .create_site("S1", "http://1", None, "np", "cookie")
            .await
            .unwrap();
        let s2 = repo
            .create_site("S2", "http://2", None, "np", "cookie")
            .await
            .unwrap();

        repo.set_task_sites(tid, &[s1, s2]).await.unwrap();
        let mut sites = repo.get_task_sites(tid).await.unwrap();
        sites.sort();
        assert_eq!(sites, vec![s1, s2]);
    }


    #[tokio::test]
    async fn set_and_get_task_source_downloaders() {
        let repo = setup_repo().await;
        let tid = repo
            .create_task("tsd", "reseed", "manual", None, None, None)
            .await
            .unwrap();
        let d1 = repo
            .create_downloader(&crate::db::models::DownloaderRow {
                id: 0,
                name: "dl1".into(),
                dl_type: "qbittorrent".into(),
                host: "127.0.0.1".into(),
                port: 8080,
                encrypted_username: None,
                username_nonce: None,
                encrypted_password: None,
                password_nonce: None,
                role: "both".into(),
                torrent_dir: None,
                default_save_path: None,
                skip_hash_check: Some(true),
                auto_start: Some(true),
                tag: None,
                enabled: true,
                created_at: String::new(),
            })
            .await
            .unwrap();
        let d2 = repo
            .create_downloader(&crate::db::models::DownloaderRow {
                id: 0,
                name: "dl2".into(),
                dl_type: "qbittorrent".into(),
                host: "127.0.0.1".into(),
                port: 8081,
                encrypted_username: None,
                username_nonce: None,
                encrypted_password: None,
                password_nonce: None,
                role: "both".into(),
                torrent_dir: None,
                default_save_path: None,
                skip_hash_check: Some(true),
                auto_start: Some(true),
                tag: None,
                enabled: true,
                created_at: String::new(),
            })
            .await
            .unwrap();

        repo.set_task_source_downloaders(tid, &[d1, d2]).await.unwrap();
        let mut ids = repo.get_task_source_downloaders(tid).await.unwrap();
        ids.sort();
        assert_eq!(ids, vec![d1, d2]);

        repo.set_task_source_downloaders(tid, &[d2]).await.unwrap();
        assert_eq!(repo.get_task_source_downloaders(tid).await.unwrap(), vec![d2]);
    }

    #[tokio::test]
    async fn create_task_persists_destination_downloader_id() {
        let repo = setup_repo().await;
        let dest_id = repo
            .create_downloader(&crate::db::models::DownloaderRow {
                id: 0,
                name: "dest".into(),
                dl_type: "qbittorrent".into(),
                host: "127.0.0.1".into(),
                port: 9090,
                encrypted_username: None,
                username_nonce: None,
                encrypted_password: None,
                password_nonce: None,
                role: "destination".into(),
                torrent_dir: None,
                default_save_path: None,
                skip_hash_check: Some(true),
                auto_start: Some(true),
                tag: None,
                enabled: true,
                created_at: String::new(),
            })
            .await
            .unwrap();
        let tid = repo
            .create_task("dest-task", "reseed", "manual", None, Some(dest_id), None)
            .await
            .unwrap();
        let task = repo.get_task(tid).await.unwrap().unwrap();
        assert_eq!(task.destination_downloader_id, Some(dest_id));
    }

}
