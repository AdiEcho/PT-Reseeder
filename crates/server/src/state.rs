use std::path::PathBuf;
use std::sync::Arc;

use pt_reseeder_core::config::AppConfig;
use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::SiteRow;
use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::db::writer::DbWriterHandle;
use pt_reseeder_core::error::CoreError;
use pt_reseeder_core::scheduler::{CronScheduler, FileWatcher, TaskExecutor};
use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
use pt_reseeder_core::site::definitions::load_all_definitions;
use pt_reseeder_core::site::models::{SiteId, UserInfoSelectors};
use pt_reseeder_core::site::rate_limiter::SiteRateLimiter;
use pt_reseeder_core::site::registry::{AdapterHandle, SiteRegistry};
use pt_reseeder_core::site::traits::{
    RepostCapable, ReseedCapable, SearchCapable, SiteCore, UserInfoCapable,
};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub db_pool: SqlitePool,
    pub db_writer: DbWriterHandle,
    pub repo: Repository,
    pub vault: Arc<RwLock<Option<Vault>>>,
    pub config: AppConfig,
    pub cancel_token: CancellationToken,
    pub start_time: std::time::Instant,
    pub site_registry: RwLock<SiteRegistry>,
    pub cron_scheduler: RwLock<Option<Arc<CronScheduler>>>,
    pub file_watcher: RwLock<Option<Arc<FileWatcher>>>,
}

impl AppState {
    pub fn new(
        db_pool: SqlitePool,
        db_writer: DbWriterHandle,
        config: AppConfig,
        cancel_token: CancellationToken,
        site_registry: SiteRegistry,
    ) -> Self {
        let repo = Repository::new(db_pool.clone());
        Self {
            inner: Arc::new(AppStateInner {
                db_pool,
                db_writer,
                repo,
                vault: Arc::new(RwLock::new(None)),
                config,
                cancel_token,
                start_time: std::time::Instant::now(),
                site_registry: RwLock::new(site_registry),
                cron_scheduler: RwLock::new(None),
                file_watcher: RwLock::new(None),
            }),
        }
    }

    pub async fn start_task_runtime(&self) -> Result<(), CoreError> {
        let cron_state = self.clone();
        let cron_scheduler = Arc::new(
            CronScheduler::new(Arc::new(move |task_id| {
                spawn_task_execution(cron_state.clone(), task_id);
            }))
            .await?,
        );
        cron_scheduler.start().await?;

        let watcher_state = self.clone();
        let file_watcher = Arc::new(FileWatcher::new(Arc::new(move |task_id, _path| {
            spawn_task_execution(watcher_state.clone(), task_id);
        }))?);
        file_watcher.start().await?;

        *self.inner.cron_scheduler.write().await = Some(cron_scheduler);
        *self.inner.file_watcher.write().await = Some(file_watcher);
        self.configure_task_runtime().await
    }

    pub async fn refresh_site_registry(&self) -> Result<(), CoreError> {
        let vault_guard = self.inner.vault.read().await;
        let Some(vault) = vault_guard.as_ref() else {
            return Ok(());
        };

        let sites = self.inner.repo.list_sites().await?;
        let mut registry = SiteRegistry::new();
        for site in sites.into_iter().filter(|site| site.enabled) {
            if let Some(handle) = build_adapter_handle(&site, vault, &self.inner.config)? {
                registry.register(SiteId::from(site.id), handle);
            }
        }

        *self.inner.site_registry.write().await = registry;
        Ok(())
    }

    pub async fn site_registry_snapshot(&self) -> Arc<SiteRegistry> {
        Arc::new(self.inner.site_registry.read().await.clone())
    }

    pub async fn vault_snapshot(&self) -> Option<Vault> {
        self.inner.vault.read().await.clone()
    }

    pub async fn task_executor(&self) -> TaskExecutor {
        TaskExecutor::new(
            self.inner.repo.clone(),
            self.inner.db_writer.clone(),
            self.site_registry_snapshot().await,
            self.inner.cancel_token.clone(),
            self.vault_snapshot().await,
        )
    }

    pub async fn configure_task_runtime(&self) -> Result<(), CoreError> {
        let tasks = self.inner.repo.list_tasks().await?;

        if let Some(cron) = self.inner.cron_scheduler.read().await.as_ref().cloned() {
            for task in tasks.iter().filter(|task| task.trigger_type == "cron") {
                if let Some(expr) = task.cron_expression.as_deref() {
                    cron.add_job(task.id, expr).await?;
                }
            }
        }

        if let Some(watcher) = self.inner.file_watcher.read().await.as_ref().cloned() {
            for task in tasks
                .iter()
                .filter(|task| task.trigger_type == "file_watch")
            {
                self.configure_file_watch_task(&watcher, task.id).await?;
            }
        }

        Ok(())
    }

    pub async fn reconfigure_task_runtime(&self, task_id: i64) -> Result<(), CoreError> {
        if let Some(cron) = self.inner.cron_scheduler.read().await.as_ref().cloned() {
            cron.remove_job(task_id).await?;
        }
        if let Some(watcher) = self.inner.file_watcher.read().await.as_ref().cloned() {
            watcher.unwatch_task(task_id).await?;
        }

        let Some(task) = self.inner.repo.get_task(task_id).await? else {
            return Ok(());
        };

        match task.trigger_type.as_str() {
            "cron" => {
                if let (Some(cron), Some(expr)) = (
                    self.inner.cron_scheduler.read().await.as_ref().cloned(),
                    task.cron_expression.as_deref(),
                ) {
                    cron.add_job(task_id, expr).await?;
                }
            }
            "file_watch" => {
                if let Some(watcher) = self.inner.file_watcher.read().await.as_ref().cloned() {
                    self.configure_file_watch_task(&watcher, task_id).await?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub async fn remove_task_runtime(&self, task_id: i64) -> Result<(), CoreError> {
        if let Some(cron) = self.inner.cron_scheduler.read().await.as_ref().cloned() {
            cron.remove_job(task_id).await?;
        }
        if let Some(watcher) = self.inner.file_watcher.read().await.as_ref().cloned() {
            watcher.unwatch_task(task_id).await?;
        }
        Ok(())
    }

    async fn configure_file_watch_task(
        &self,
        watcher: &FileWatcher,
        task_id: i64,
    ) -> Result<(), CoreError> {
        let mut folders = Vec::new();
        for folder_id in self.inner.repo.get_task_folders(task_id).await? {
            if let Some(folder) = self.inner.repo.get_folder(folder_id).await? {
                if folder.enabled && folder.scan_mode == "local" {
                    folders.push(PathBuf::from(folder.path));
                }
            }
        }

        if folders.is_empty() {
            warn!(task_id, "file_watch task has no enabled local folders");
            return Ok(());
        }

        watcher.watch_task(task_id, folders).await
    }
}

fn spawn_task_execution(state: AppState, task_id: i64) {
    tokio::spawn(async move {
        let executor = state.task_executor().await;
        if let Err(e) = executor.execute(task_id).await {
            error!(task_id, %e, "scheduled task execution failed");
        }
    });
}

fn decrypt_credential(vault: &Vault, encrypted: &[u8], nonce: &[u8]) -> Result<String, CoreError> {
    let nonce_arr: [u8; 12] = nonce
        .try_into()
        .map_err(|_| CoreError::Internal("invalid credential nonce length".to_string()))?;
    let plaintext = vault.decrypt(encrypted, &nonce_arr)?;
    String::from_utf8(plaintext)
        .map_err(|e| CoreError::Internal(format!("credential is not valid UTF-8: {}", e)))
}

fn selectors_for(site: &SiteRow, data_dir: &std::path::Path) -> UserInfoSelectors {
    let definitions = load_all_definitions(Some(data_dir));
    let site_key = site.name.to_ascii_lowercase();
    definitions
        .get(&site_key)
        .or_else(|| {
            definitions
                .values()
                .find(|def| def.site.name.eq_ignore_ascii_case(&site.name))
        })
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
        })
}

fn build_adapter_handle(
    site: &SiteRow,
    vault: &Vault,
    config: &AppConfig,
) -> Result<Option<AdapterHandle>, CoreError> {
    if site.adapter_type != "nexusphp" {
        return Ok(None);
    }

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

    let definitions = load_all_definitions(Some(&config.data_dir));
    let site_key = site.name.to_ascii_lowercase();
    let batch_size = definitions
        .get(&site_key)
        .or_else(|| {
            definitions
                .values()
                .find(|def| def.site.name.eq_ignore_ascii_case(&site.name))
        })
        .and_then(|def| def.site.batch_size)
        .unwrap_or(1000);

    let adapter = Arc::new(NexusPhpAdapter::new(
        site.name.clone(),
        site.url.clone(),
        site.api_url.clone(),
        cookie,
        passkey,
        None,
        selectors_for(site, &config.data_dir),
        batch_size,
    ));
    let core: Arc<dyn SiteCore> = adapter.clone();
    let reseed: Arc<dyn ReseedCapable> = adapter.clone();
    let repost: Arc<dyn RepostCapable> = adapter.clone();
    let user_info: Arc<dyn UserInfoCapable> = adapter.clone();
    let search: Arc<dyn SearchCapable> = adapter;
    let rate_limiter = Arc::new(SiteRateLimiter::new(
        site.rate_limit_interval_ms.unwrap_or(5000).max(1) as u64,
        site.rate_limit_burst.unwrap_or(1).max(1) as u32,
    ));

    Ok(Some(AdapterHandle {
        core,
        reseed: Some(reseed),
        repost: Some(repost),
        user_info: Some(user_info),
        search: Some(search),
        rate_limiter,
    }))
}
