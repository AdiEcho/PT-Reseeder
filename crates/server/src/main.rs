use pt_reseeder_core::config::AppConfig;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
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

    // Init tracing with dual-write: stdout + file appender
    let log_dir = config.log_dir.clone();
    std::fs::create_dir_all(&log_dir).expect("Failed to create log directory");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| config.log_min_level.clone().into());

    let file_appender = tracing_appender::rolling::daily(&log_dir, "pt-reseeder");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let (log_tx, _) = tokio::sync::broadcast::channel::<String>(1024);
    let broadcast_layer = pt_reseeder_server::log::BroadcastLayer::new(log_tx.clone());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .with(broadcast_layer)
        .init();

    let cancel_token = CancellationToken::new();
    let ct = cancel_token.clone();

    // Handle SIGTERM/SIGINT
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received shutdown signal");
        ct.cancel();
    });

    match pt_reseeder_server::run_server(config, cancel_token.clone(), log_tx).await {
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
