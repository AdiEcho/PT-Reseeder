mod webview;

use pt_reseeder_core::config::AppConfig;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

fn main() {
    let cancel_token = CancellationToken::new();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![webview::inject_repost_autofill])
        .setup({
            let cancel_token = cancel_token.clone();
            move |app| {
                let site_root = app.path().resource_dir()?.join("site");
                let data_dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&data_dir)?;

                let db_path = data_dir.join("pt-reseeder.db");
                let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
                let server_cancel = cancel_token.clone();
                let (addr_tx, addr_rx) = std::sync::mpsc::channel();

                std::thread::spawn(move || {
                    let runtime = match tokio::runtime::Runtime::new() {
                        Ok(runtime) => runtime,
                        Err(err) => {
                            let _ = addr_tx.send(Err(format!(
                                "failed to create desktop server runtime: {err}"
                            )));
                            return;
                        }
                    };

                    runtime.block_on(async move {
                        let mut config = AppConfig::load();
                        config.server_bind = match "127.0.0.1:0".parse() {
                            Ok(addr) => addr,
                            Err(err) => {
                                let _ = addr_tx.send(Err(format!(
                                    "failed to parse desktop bind address: {err}"
                                )));
                                return;
                            }
                        };
                        config.database_url = database_url;
                        config.data_dir = data_dir;
                        config.leptos_site_root = site_root;

                        match pt_reseeder_server::run_server(config, server_cancel).await {
                            Ok(bound) => {
                                let _ = addr_tx.send(Ok(bound.0));
                                std::future::pending::<()>().await;
                            }
                            Err(err) => {
                                let _ = addr_tx
                                    .send(Err(format!("failed to start embedded server: {err}")));
                            }
                        }
                    });
                });

                let addr = addr_rx
                    .recv()
                    .map_err(|_| "embedded server did not start")??;
                let url = url::Url::parse(&format!("http://{addr}"))?;

                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(url))
                    .title("PT-Reseeder")
                    .inner_size(1200.0, 800.0)
                    .build()?;

                Ok(())
            }
        })
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                cancel_token.cancel();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
