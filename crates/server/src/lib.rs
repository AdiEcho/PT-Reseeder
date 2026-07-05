pub mod api;
pub mod app;
pub mod auth;
pub mod state;

use pt_reseeder_core::config::AppConfig;
use pt_reseeder_core::db;
use pt_reseeder_core::site::registry::SiteRegistry;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct BoundAddr(pub SocketAddr);

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
    let state = state::AppState::new(
        pool,
        db_writer,
        config.clone(),
        cancel_token.clone(),
        site_registry,
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
