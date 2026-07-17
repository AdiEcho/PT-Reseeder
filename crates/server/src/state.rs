use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

use pt_reseeder_core::browser::RepostAutoFiller;
use pt_reseeder_core::config::AppConfig;
use pt_reseeder_core::crypto::Vault;
use pt_reseeder_core::db::models::SiteRow;
use pt_reseeder_core::db::repo::Repository;
use pt_reseeder_core::db::writer::DbWriterHandle;
use pt_reseeder_core::error::CoreError;
use pt_reseeder_core::scheduler::{CronScheduler, FileWatcher, TaskExecutor};
use pt_reseeder_core::site::adapters::gazelle::GazelleAdapter;
use pt_reseeder_core::site::adapters::mteam::MTeamAdapter;
use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
use pt_reseeder_core::site::adapters::unit3d::Unit3dAdapter;
use pt_reseeder_core::site::adapters::zhuque::ZhuqueAdapter;
use pt_reseeder_core::site::definitions::load_all_definitions;
use pt_reseeder_core::site::models::{SiteDefinition, SiteId, UserInfoSelectors};
use pt_reseeder_core::site::rate_limiter::SiteRateLimiter;
use pt_reseeder_core::site::registry::{AdapterHandle, SiteRegistry};
use pt_reseeder_core::site::traits::{
    RepostCapable, ReseedCapable, SearchCapable, SiteCore, UserInfoCapable,
};
use sqlx::SqlitePool;

impl axum::extract::FromRef<AppState> for leptos::config::LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options()
    }
}
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

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
    pub leptos_options: leptos::config::LeptosOptions,
    pub cancel_token: CancellationToken,
    pub start_time: std::time::Instant,
    /// Registry is stored as `Arc` so snapshots only clone the Arc pointer.
    pub site_registry: Arc<RwLock<Arc<SiteRegistry>>>,
    site_registry_refresh: Mutex<()>,
    pub fetch_seeding_size: Arc<AtomicBool>,
    pub cron_scheduler: RwLock<Option<Arc<CronScheduler>>>,
    pub file_watcher: RwLock<Option<Arc<FileWatcher>>>,
    pub repost_autofiller: Option<Arc<dyn RepostAutoFiller>>,
    pub repost_autofiller_error: Option<String>,
    pub log_broadcast: tokio::sync::broadcast::Sender<String>,
}

impl AppState {
    pub fn new(
        db_pool: SqlitePool,
        db_writer: DbWriterHandle,
        config: AppConfig,
        cancel_token: CancellationToken,
        site_registry: SiteRegistry,
        repost_autofiller: Option<Arc<dyn RepostAutoFiller>>,
        repost_autofiller_error: Option<String>,
        fetch_seeding_size: Arc<AtomicBool>,
        log_broadcast: tokio::sync::broadcast::Sender<String>,
    ) -> Self {
        let repo = Repository::new(db_pool.clone());
        let leptos_options = leptos::config::LeptosOptions::builder()
            .output_name("pt-reseeder")
            .site_root(config.leptos_site_root.to_string_lossy().to_string())
            .site_pkg_dir("pkg")
            .site_addr(config.server_bind)
            .build();
        Self {
            inner: Arc::new(AppStateInner {
                db_pool,
                db_writer,
                repo,
                vault: Arc::new(RwLock::new(None)),
                config,
                leptos_options,
                cancel_token,
                start_time: std::time::Instant::now(),
                site_registry: Arc::new(RwLock::new(Arc::new(site_registry))),
                site_registry_refresh: Mutex::new(()),
                fetch_seeding_size,
                cron_scheduler: RwLock::new(None),
                file_watcher: RwLock::new(None),
                repost_autofiller,
                repost_autofiller_error,
                log_broadcast,
            }),
        }
    }

    pub fn leptos_options(&self) -> leptos::config::LeptosOptions {
        self.inner.leptos_options.clone()
    }

    /// Build a ServerFnContext for Leptos SSR / server functions.
    pub fn server_fn_context(
        &self,
        authenticated_user_id: Option<i64>,
    ) -> pt_reseeder_frontend::server_fns::ServerFnContext {
        let refresh_state = self.clone();
        let reconfigure_state = self.clone();
        let remove_state = self.clone();
        pt_reseeder_frontend::server_fns::ServerFnContext {
            pool: self.inner.db_pool.clone(),
            vault: self.inner.vault.clone(),
            session_ttl_hours: self.inner.config.session_ttl_hours,
            cookie_secure: self.inner.config.cookie_secure,
            data_dir: self.inner.config.data_dir.clone(),
            log_dir: self.inner.config.log_dir.clone(),
            site_registry: self.inner.site_registry.clone(),
            refresh_site_registry: std::sync::Arc::new(move || {
                let state = refresh_state.clone();
                Box::pin(async move {
                    state
                        .refresh_site_registry()
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
            fetch_seeding_size: self.inner.fetch_seeding_size.clone(),
            trigger_task_execution: std::sync::Arc::new({
                let state = self.clone();
                move |task_id, dry_run| spawn_task_execution(state.clone(), task_id, dry_run)
            }),
            reconfigure_task_runtime: std::sync::Arc::new(move |task_id| {
                let state = reconfigure_state.clone();
                Box::pin(async move {
                    state
                        .reconfigure_task_runtime(task_id)
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
            remove_task_runtime: std::sync::Arc::new(move |task_id| {
                let state = remove_state.clone();
                Box::pin(async move {
                    state
                        .remove_task_runtime(task_id)
                        .await
                        .map_err(|error| error.to_string())
                })
            }),
            authenticated_user_id,
        }
    }

    pub async fn start_task_runtime(&self) -> Result<(), CoreError> {
        let recovered = self.inner.repo.recover_interrupted_tasks().await?;
        if recovered > 0 {
            warn!(recovered, "marked interrupted running tasks as error");
        }

        let cron_state = self.clone();
        let cron_scheduler = Arc::new(
            CronScheduler::new(Arc::new(move |task_id| {
                // Scheduled runs are always real execution — never dry-run.
                spawn_task_execution(cron_state.clone(), task_id, false);
            }))
            .await?,
        );
        cron_scheduler.start().await?;

        let watcher_state = self.clone();
        let file_watcher = Arc::new(FileWatcher::new(Arc::new(move |task_id, _path| {
            // File-watch triggers are always real execution — never dry-run.
            spawn_task_execution(watcher_state.clone(), task_id, false);
        }))?);
        file_watcher.start().await?;

        *self.inner.cron_scheduler.write().await = Some(cron_scheduler);
        *self.inner.file_watcher.write().await = Some(file_watcher);
        self.configure_task_runtime().await?;
        self.start_session_cleanup_task();
        Ok(())
    }

    /// Periodically remove expired sessions from the database.
    fn start_session_cleanup_task(&self) {
        let repo = self.inner.repo.clone();
        let cancel = self.inner.cancel_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            // First tick completes immediately; skip so we don't clean on boot race.
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        match repo.cleanup_expired_sessions().await {
                            Ok(n) if n > 0 => {
                                info!(removed = n, "cleaned up expired sessions");
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!(%e, "failed to clean expired sessions");
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn refresh_site_registry(&self) -> Result<(), CoreError> {
        let _refresh_guard = self.inner.site_registry_refresh.lock().await;
        let Some(vault) = self.inner.vault.read().await.clone() else {
            return Ok(());
        };

        // Load definitions once for the entire refresh.
        let definitions = load_all_definitions(Some(&self.inner.config.data_dir));

        let sites = self.inner.repo.list_sites().await?;
        let mut registry = SiteRegistry::new();
        for site in sites.into_iter().filter(|site| site.enabled) {
            match build_adapter_handle(
                &site,
                &vault,
                &definitions,
                self.inner.fetch_seeding_size.clone(),
            ) {
                Ok(Some(handle)) => registry.register(SiteId::from(site.id), handle),
                Ok(None) => {}
                Err(error) => {
                    warn!(site_id = site.id, site = %site.name, %error, "skipping site with invalid credentials during registry refresh");
                }
            }
        }

        *self.inner.site_registry.write().await = Arc::new(registry);
        Ok(())
    }

    pub async fn site_registry_snapshot(&self) -> Arc<SiteRegistry> {
        Arc::clone(&*self.inner.site_registry.read().await)
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
        let cron = self.inner.cron_scheduler.read().await.as_ref().cloned();
        let watcher = self.inner.file_watcher.read().await.as_ref().cloned();

        if let Some(cron) = cron.as_ref() {
            cron.remove_job(task_id).await?;
        }
        if let Some(watcher) = watcher.as_ref() {
            watcher.unwatch_task(task_id).await?;
        }

        let Some(task) = self.inner.repo.get_task(task_id).await? else {
            return Ok(());
        };

        match task.trigger_type.as_str() {
            "cron" => {
                if let (Some(cron), Some(expr)) = (cron.as_ref(), task.cron_expression.as_deref()) {
                    cron.add_job(task_id, expr).await?;
                }
            }
            "file_watch" => {
                if let Some(watcher) = watcher.as_ref() {
                    self.configure_file_watch_task(watcher, task_id).await?;
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

fn spawn_task_execution(state: AppState, task_id: i64, dry_run: bool) {
    tokio::spawn(async move {
        let executor = state.task_executor().await;
        if let Err(e) = executor.execute(task_id, dry_run).await {
            error!(task_id, dry_run, %e, "task execution failed");
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

fn lookup_definition<'a>(
    definitions: &'a HashMap<String, SiteDefinition>,
    site: &SiteRow,
) -> Option<&'a SiteDefinition> {
    let site_key = site.name.to_ascii_lowercase();
    definitions.get(&site_key).or_else(|| {
        definitions
            .values()
            .find(|def| def.site.name.eq_ignore_ascii_case(&site.name))
    })
}

fn selectors_for(
    site: &SiteRow,
    definitions: &HashMap<String, SiteDefinition>,
) -> UserInfoSelectors {
    lookup_definition(definitions, site)
        .and_then(|def| def.user_info.clone())
        .unwrap_or(UserInfoSelectors {
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

fn decrypt_token(site: &SiteRow, vault: &Vault) -> Result<Option<String>, CoreError> {
    if let (Some(enc), Some(nonce)) = (&site.encrypted_token, &site.token_nonce) {
        Ok(Some(decrypt_credential(vault, enc, nonce)?))
    } else {
        Ok(None)
    }
}

fn make_adapter_handle<T>(adapter: Arc<T>, rate_limiter: Arc<SiteRateLimiter>) -> AdapterHandle
where
    T: SiteCore
        + ReseedCapable
        + RepostCapable
        + UserInfoCapable
        + SearchCapable
        + 'static,
{
    AdapterHandle {
        core: adapter.clone() as Arc<dyn SiteCore>,
        reseed: Some(adapter.clone() as Arc<dyn ReseedCapable>),
        repost: Some(adapter.clone() as Arc<dyn RepostCapable>),
        user_info: Some(adapter.clone() as Arc<dyn UserInfoCapable>),
        search: Some(adapter as Arc<dyn SearchCapable>),
        rate_limiter,
    }
}

fn build_adapter_handle(
    site: &SiteRow,
    vault: &Vault,
    definitions: &HashMap<String, SiteDefinition>,
    fetch_seeding_size: Arc<AtomicBool>,
) -> Result<Option<AdapterHandle>, CoreError> {
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

    let batch_size = lookup_definition(definitions, site)
        .and_then(|def| def.site.batch_size)
        .unwrap_or(1000);

    let rate_limiter = Arc::new(SiteRateLimiter::new(
        site.rate_limit_interval_ms.unwrap_or(5000).max(1) as u64,
        site.rate_limit_burst.unwrap_or(1).max(1) as u32,
    ));

    match site.adapter_type.as_str() {
        "nexusphp" => {
            let adapter = Arc::new(
                NexusPhpAdapter::new(
                    site.name.clone(),
                    site.url.clone(),
                    site.api_url.clone(),
                    cookie,
                    passkey,
                    None,
                    selectors_for(site, definitions),
                    batch_size,
                )
                .with_fetch_seeding_size_switch(fetch_seeding_size),
            );
            Ok(Some(make_adapter_handle(adapter, rate_limiter)))
        }
        "mteam" => {
            let api_token = decrypt_token(site, vault)?;
            let adapter = Arc::new(MTeamAdapter::new(
                site.name.clone(),
                site.url.clone(),
                api_token,
                passkey,
                batch_size,
            ));
            Ok(Some(make_adapter_handle(adapter, rate_limiter)))
        }
        "unit3d" => {
            let api_token = decrypt_token(site, vault)?;
            let adapter = Arc::new(Unit3dAdapter::new(
                site.name.clone(),
                site.url.clone(),
                api_token,
                passkey,
                batch_size,
            ));
            Ok(Some(make_adapter_handle(adapter, rate_limiter)))
        }
        "gazelle" => {
            let adapter = Arc::new(GazelleAdapter::new(
                site.name.clone(),
                site.url.clone(),
                cookie,
                passkey,
                batch_size,
            ));
            Ok(Some(make_adapter_handle(adapter, rate_limiter)))
        }
        "zhuque" => {
            let api_token = decrypt_token(site, vault)?;
            let adapter = Arc::new(ZhuqueAdapter::new(
                site.name.clone(),
                site.url.clone(),
                api_token,
                passkey,
                cookie,
                batch_size,
            ));
            Ok(Some(make_adapter_handle(adapter, rate_limiter)))
        }
        other => {
            warn!(site = %site.name, adapter_type = %other, "unknown adapter type, skipping");
            Ok(None)
        }
    }
}
