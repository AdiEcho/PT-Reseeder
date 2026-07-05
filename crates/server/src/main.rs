use pt_reseeder_core::config::AppConfig;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    // --healthcheck mode: simple TCP connect to verify server is running
    if args.iter().any(|a| a == "--healthcheck") {
        match std::net::TcpStream::connect("127.0.0.1:3000") {
            Ok(_) => {
                eprintln!("Health check: server is reachable");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Health check failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    let mut config = AppConfig::load();

    // --bind override
    if let Some(pos) = args.iter().position(|a| a == "--bind") {
        if let Some(addr) = args.get(pos + 1) {
            config.server_bind = addr.parse().expect("Invalid bind address");
        }
    }

    let cancel_token = CancellationToken::new();
    let ct = cancel_token.clone();

    // Handle SIGTERM/SIGINT
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received shutdown signal");
        ct.cancel();
    });

    match pt_reseeder_server::run_server(config, cancel_token.clone()).await {
        Ok(addr) => {
            tracing::info!("Server bound to {}", addr.0);
            // Keep running until cancelled
            cancel_token.cancelled().await;
        }
        Err(e) => {
            tracing::error!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
