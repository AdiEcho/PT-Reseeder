pub mod models;
pub mod repo;
pub mod writer;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::error::{CoreError, DbError};

pub async fn init_db(database_url: &str) -> Result<SqlitePool, CoreError> {
    let options = SqliteConnectOptions::from_str(database_url)
        .map_err(DbError::Sqlx)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_millis(5000))
        .foreign_keys(true);

    let max_conns = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);

    let pool = SqlitePoolOptions::new()
        .max_connections(max_conns)
        .connect_with(options)
        .await
        .map_err(DbError::Sqlx)?;

    // Run migrations
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .map_err(|e| DbError::MigrationFailed(e.to_string()))?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
        count > 0
    }

    async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(pool)
            .await
            .unwrap();
        rows.into_iter().any(|row| {
            let name: String = row.try_get("name").unwrap();
            name == column
        })
    }

    async fn apply_sql(pool: &SqlitePool, sql: &str) {
        for stmt in sql.split(';') {
            let without_comments: String = stmt
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            if without_comments.is_empty() {
                continue;
            }
            sqlx::query(&without_comments).execute(pool).await.unwrap();
        }
    }

    async fn open_raw_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap()
    }

    async fn setup_005_era_schema(pool: &SqlitePool) {
        let schema = r#"
CREATE TABLE downloaders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    dl_type TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    encrypted_username BLOB,
    username_nonce BLOB,
    encrypted_password BLOB,
    password_nonce BLOB,
    role TEXT NOT NULL DEFAULT 'both',
    torrent_dir TEXT,
    default_save_path TEXT,
    skip_hash_check INTEGER DEFAULT 1,
    auto_start INTEGER DEFAULT 1,
    tag TEXT DEFAULT 'PT-Reseeder',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE downloader_pairs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    source_id INTEGER NOT NULL REFERENCES downloaders(id),
    destination_id INTEGER NOT NULL REFERENCES downloaders(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    api_url TEXT,
    adapter_type TEXT NOT NULL DEFAULT 'nexusphp',
    auth_type TEXT NOT NULL DEFAULT 'cookie',
    encrypted_cookie BLOB,
    cookie_nonce BLOB,
    encrypted_passkey BLOB,
    passkey_nonce BLOB,
    encrypted_token BLOB,
    token_nonce BLOB,
    rate_limit_interval_ms INTEGER DEFAULT 5000,
    rate_limit_burst INTEGER DEFAULT 1,
    download_interval_ms INTEGER DEFAULT 5000,
    probe_status TEXT NOT NULL DEFAULT 'unknown',
    probe_detail_json TEXT,
    probed_at TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    task_type TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    cron_expression TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    downloader_pair_id INTEGER REFERENCES downloader_pairs(id),
    destination_downloader_id INTEGER REFERENCES downloaders(id),
    last_run_at TEXT,
    next_run_at TEXT,
    run_count INTEGER DEFAULT 0,
    config_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE task_sites (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, site_id)
);

CREATE TABLE task_source_downloaders (
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    downloader_id INTEGER NOT NULL REFERENCES downloaders(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, downloader_id)
);
"#;
        apply_sql(pool, schema).await;
    }

    async fn run_006(pool: &SqlitePool) {
        let sql = include_str!("../../../../migrations/006_drop_downloader_pairs.sql");
        apply_sql(pool, sql).await;
    }

    #[tokio::test]
    async fn init_db_drops_downloader_pairs_and_pair_column() {
        let pool = init_db("sqlite::memory:").await.unwrap();
        assert!(!table_exists(&pool, "downloader_pairs").await);
        assert!(!column_exists(&pool, "tasks", "downloader_pair_id").await);
        assert!(column_exists(&pool, "tasks", "destination_downloader_id").await);
        assert!(table_exists(&pool, "task_source_downloaders").await);
    }

    #[tokio::test]
    async fn migration_006_backfills_dest_and_source_then_drops_pair() {
        let pool = open_raw_pool().await;
        setup_005_era_schema(&pool).await;

        let src = sqlx::query(
            "INSERT INTO downloaders (name, dl_type, host, port) VALUES ('src', 'qbittorrent', 'h', 1)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let dst = sqlx::query(
            "INSERT INTO downloaders (name, dl_type, host, port) VALUES ('dst', 'qbittorrent', 'h', 2)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let other_src = sqlx::query(
            "INSERT INTO downloaders (name, dl_type, host, port) VALUES ('other', 'qbittorrent', 'h', 3)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let pair = sqlx::query(
            "INSERT INTO downloader_pairs (name, source_id, destination_id) VALUES ('pair', ?, ?)",
        )
        .bind(src)
        .bind(dst)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let site = sqlx::query(
            "INSERT INTO sites (name, url, adapter_type, auth_type) VALUES ('S', 'http://s', 'np', 'cookie')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        // Case A: pair + null dest + zero sources + child association
        let task_a = sqlx::query(
            "INSERT INTO tasks (name, task_type, trigger_type, downloader_pair_id, destination_downloader_id) \
             VALUES ('a', 'reseed', 'manual', ?, NULL)",
        )
        .bind(pair)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO task_sites (task_id, site_id) VALUES (?, ?)")
            .bind(task_a)
            .bind(site)
            .execute(&pool)
            .await
            .unwrap();

        // Case B: existing source must not be overwritten; dest still null so dest backfill runs
        let task_b = sqlx::query(
            "INSERT INTO tasks (name, task_type, trigger_type, downloader_pair_id, destination_downloader_id) \
             VALUES ('b', 'reseed', 'manual', ?, NULL)",
        )
        .bind(pair)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO task_source_downloaders (task_id, downloader_id) VALUES (?, ?)")
            .bind(task_b)
            .bind(other_src)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO task_sites (task_id, site_id) VALUES (?, ?)")
            .bind(task_b)
            .bind(site)
            .execute(&pool)
            .await
            .unwrap();

        let child_sites_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_sites")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(child_sites_before, 2);

        run_006(&pool).await;

        // dest backfill
        let dest_a: Option<i64> =
            sqlx::query_scalar("SELECT destination_downloader_id FROM tasks WHERE id = ?")
                .bind(task_a)
                .fetch_one(&pool)
                .await
                .unwrap();
        let dest_b: Option<i64> =
            sqlx::query_scalar("SELECT destination_downloader_id FROM tasks WHERE id = ?")
                .bind(task_b)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(dest_a, Some(dst));
        assert_eq!(dest_b, Some(dst));

        // source backfill for zero-source task only
        let sources_a: Vec<i64> = sqlx::query_scalar(
            "SELECT downloader_id FROM task_source_downloaders WHERE task_id = ? ORDER BY downloader_id",
        )
        .bind(task_a)
        .fetch_all(&pool)
        .await
        .unwrap();
        let sources_b: Vec<i64> = sqlx::query_scalar(
            "SELECT downloader_id FROM task_source_downloaders WHERE task_id = ? ORDER BY downloader_id",
        )
        .bind(task_b)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(sources_a, vec![src]);
        assert_eq!(sources_b, vec![other_src]);

        // child counts unchanged
        let child_sites_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_sites")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(child_sites_after, child_sites_before);

        assert!(!table_exists(&pool, "downloader_pairs").await);
        assert!(!column_exists(&pool, "tasks", "downloader_pair_id").await);
    }
}
