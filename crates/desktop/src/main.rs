mod webview;

use pt_reseeder_core::config::AppConfig;
use tokio_util::sync::CancellationToken;

fn main() {
    let cancel_token = CancellationToken::new();
    let server_cancel = cancel_token.clone();

    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime =
            tokio::runtime::Runtime::new().expect("failed to create desktop server runtime");
        runtime.block_on(async move {
            let mut config = AppConfig::load();
            config.server_bind = "127.0.0.1:0".parse().expect("valid desktop bind address");
            match pt_reseeder_server::run_server(config, server_cancel).await {
                Ok(bound) => {
                    let _ = addr_tx.send(bound.0);
                    std::future::pending::<()>().await;
                }
                Err(err) => {
                    eprintln!("failed to start embedded server: {err}");
                }
            }
        });
    });

    let addr = addr_rx.recv().expect("embedded server did not start");
    let url = url::Url::parse(&format!("http://{addr}")).expect("valid embedded server URL");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![webview::inject_repost_autofill])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(url.clone()))
                .title("PT-Reseeder")
                .inner_size(1200.0, 800.0)
                .build()?;
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                cancel_token.cancel();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
