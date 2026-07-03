use sqlx::SqlitePool;

use super::models::*;
use crate::error::{CoreError, DbError};

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

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<User>, CoreError> {
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
        let result = sqlx::query(
            "INSERT INTO sessions (user_id, token_hash, expires_at) VALUES (?, ?, ?)",
        )
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
        let session = sqlx::query_as::<_, Session>(
            "SELECT * FROM sessions WHERE token_hash = ?",
        )
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
        let result = sqlx::query(
            "DELETE FROM sessions WHERE expires_at < datetime('now')",
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.rows_affected())
    }

    // --- App Config ---

    pub async fn get_config(&self, key: &str) -> Result<Option<String>, CoreError> {
        let row = sqlx::query_as::<_, AppConfigEntry>(
            "SELECT * FROM app_config WHERE key = ?",
        )
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
        let row = sqlx::query_as::<_, DownloaderRow>(
            "SELECT * FROM downloaders WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(row)
    }

    pub async fn list_downloaders(&self) -> Result<Vec<DownloaderRow>, CoreError> {
        let rows = sqlx::query_as::<_, DownloaderRow>(
            "SELECT * FROM downloaders ORDER BY id",
        )
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

    // --- Downloader Pairs ---

    pub async fn create_downloader_pair(
        &self,
        name: &str,
        source_id: i64,
        destination_id: i64,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO downloader_pairs (name, source_id, destination_id) VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(source_id)
        .bind(destination_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(result.last_insert_rowid())
    }

    pub async fn list_downloader_pairs(&self) -> Result<Vec<DownloaderPairRow>, CoreError> {
        let rows = sqlx::query_as::<_, DownloaderPairRow>(
            "SELECT * FROM downloader_pairs ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(rows)
    }

    pub async fn delete_downloader_pair(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM downloader_pairs WHERE id = ?")
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
        let row = sqlx::query_as::<_, PiecesCacheEntry>(
            "SELECT * FROM pieces_cache WHERE info_hash = ?",
        )
        .bind(info_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::Sqlx)?;
        Ok(row)
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

    // --- Folders ---

    pub async fn create_folder(
        &self,
        path: &str,
        scan_mode: &str,
        downloader_id: Option<i64>,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO folders (path, scan_mode, downloader_id) VALUES (?, ?, ?)",
        )
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

    pub async fn delete_folder(&self, id: i64) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(DbError::Sqlx)?;
        Ok(())
    }

    // --- Tasks ---

    pub async fn create_task(
        &self,
        name: &str,
        task_type: &str,
        trigger_type: &str,
        cron_expression: Option<&str>,
        downloader_pair_id: Option<i64>,
        config_json: Option<&str>,
    ) -> Result<i64, CoreError> {
        let result = sqlx::query(
            "INSERT INTO tasks \
             (name, task_type, trigger_type, cron_expression, downloader_pair_id, config_json) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(task_type)
        .bind(trigger_type)
        .bind(cron_expression)
        .bind(downloader_pair_id)
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
        sqlx::query(
            "UPDATE tasks SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
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

    pub async fn get_task_logs(
        &self,
        task_id: i64,
        limit: i64,
    ) -> Result<Vec<TaskLog>, CoreError> {
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
}
