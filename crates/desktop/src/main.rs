mod webview;

use pt_reseeder_core::config::AppConfig;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

fn main() {
    let cancel_token = CancellationToken::new();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            webview::inject_repost_autofill,
            webview::open_upload_page,
            webview::check_autofill_available,
        ])
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
                        let bind = std::env::var("PT_RESEEDER_GUI_BIND")
                            .or_else(|_| std::env::var("LEPTOS_SITE_ADDR"))
                            .unwrap_or_else(|_| "0.0.0.0:3000".to_string());
                        config.server_bind = match bind.parse() {
                            Ok(addr) => addr,
                            Err(err) => {
                                let _ = addr_tx.send(Err(format!(
                                    "failed to parse GUI bind address: {err}"
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
                let window_host = if addr.ip().is_unspecified() {
                    "127.0.0.1".to_string()
                } else {
                    addr.ip().to_string()
                };
                let url = url::Url::parse(&format!("http://{}:{}", window_host, addr.port()))?;

                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(url))
                    .title("PT-Reseeder")
                    .inner_size(1200.0, 800.0)
                    .build()?;

                Ok(())
            }
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
