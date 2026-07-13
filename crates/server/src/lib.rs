pub mod api;
pub mod app;
pub mod auth;
pub mod state;
pub mod ws;

use pt_reseeder_core::config::AppConfig;
use pt_reseeder_core::db;
use pt_reseeder_core::site::registry::SiteRegistry;
use std::net::SocketAddr;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct BoundAddr(pub SocketAddr);

#[cfg(feature = "headless-browser")]
async fn initialize_repost_autofiller() -> (
    Option<Arc<dyn pt_reseeder_core::browser::RepostAutoFiller>>,
    Option<String>,
) {
    let chrome_path = std::env::var("CHROME_PATH")
        .ok()
        .filter(|path| !path.trim().is_empty());
    match pt_reseeder_core::browser::headless::HeadlessBrowser::from_env(chrome_path).await {
        Ok(browser) => {
            tracing::info!("headless repost autofill is available");
            (Some(Arc::new(browser)), None)
        }
        Err(error) => {
            let message = format!("headless browser unavailable: {error}");
            tracing::warn!(%message);
            (None, Some(message))
        }
    }
}

#[cfg(not(feature = "headless-browser"))]
async fn initialize_repost_autofiller() -> (
    Option<Arc<dyn pt_reseeder_core::browser::RepostAutoFiller>>,
    Option<String>,
) {
    (
        None,
        Some("server was built without the headless-browser feature".to_string()),
    )
}

pub async fn run_server(
    config: AppConfig,
    cancel_token: CancellationToken,
) -> Result<BoundAddr, Box<dyn std::error::Error>> {
    // Init DB
    let pool = db::init_db(&config.database_url).await?;

    // Create DbWriter
    let db_writer = db::writer::spawn_writer(&config.database_url, 100)?;

    // Build state
    let site_registry = SiteRegistry::new();
    let fetch_seeding_size = Arc::new(AtomicBool::new(
        pt_reseeder_core::db::repo::Repository::new(pool.clone())
            .get_config("fetch_seeding_size")
            .await?
            .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1"),
    ));
    let (repost_autofiller, repost_autofiller_error) = initialize_repost_autofiller().await;
    let state = state::AppState::new(
        pool,
        db_writer,
        config.clone(),
        cancel_token.clone(),
        site_registry,
        repost_autofiller,
        repost_autofiller_error,
        fetch_seeding_size,
    );
    state.start_task_runtime().await?;

    // Build router
    let router = app::build_router(state);

    // Bind
    let listener = TcpListener::bind(config.server_bind).await?;
    let addr = listener.local_addr()?;

    tracing::info!("Server listening on {}", addr);

    // Serve with graceful shutdown
    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        cancel_token.cancelled().await;
        tracing::info!("Shutdown signal received");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("Server error: {}", e);
        }
    });

    Ok(BoundAddr(addr))
}
