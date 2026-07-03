use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use sqlx::SqlitePool;
use pt_reseeder_core::config::AppConfig;
use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::writer::DbWriterHandle;
use pt_reseeder_core::db::repo::Repository;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub db_pool: SqlitePool,
    pub db_writer: DbWriterHandle,
    pub repo: Repository,
    pub vault: RwLock<Option<Vault>>,
    pub config: AppConfig,
    pub cancel_token: CancellationToken,
    pub start_time: std::time::Instant,
}

impl AppState {
    pub fn new(
        db_pool: SqlitePool,
        db_writer: DbWriterHandle,
        config: AppConfig,
        cancel_token: CancellationToken,
    ) -> Self {
        let repo = Repository::new(db_pool.clone());
        Self {
            inner: Arc::new(AppStateInner {
                db_pool,
                db_writer,
                repo,
                vault: RwLock::new(None),
                config,
                cancel_token,
                start_time: std::time::Instant::now(),
            }),
        }
    }
}
