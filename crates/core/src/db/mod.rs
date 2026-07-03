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
