pub mod events;

use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::Response,
};
use events::WsEvent;
use pt_reseeder_core::site::models::SiteId;
use pt_reseeder_core::stats::reseed::ReseedStatsService;
use pt_reseeder_core::stats::user_info::UserInfoService;
use std::time::Duration;
use tokio::time::interval;

/// Validate WebSocket authentication from headers (shared by all WS endpoints).
async fn validate_ws_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), axum::http::StatusCode> {
    // Origin is the *only* CSRF defence on this path: `csrf_check` is layered on
    // the /api subtree, not /ws. An upgrade with no Origin header used to skip
    // the check entirely; browsers always send Origin on a WS upgrade, so a
    // missing one means a non-browser client — reject rather than wave through.
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::FORBIDDEN)?;
    let allowed = state.inner.config.effective_allowed_origins();
    if !allowed.iter().any(|o| o == origin) {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    // Validate session cookie from headers. Shared with the REST middleware and
    // the server functions, so an expired session gets its row deleted here too
    // — this path used to leave stale rows behind.
    crate::auth::resolve_session_from_headers(state, headers).await?;

    Ok(())
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, axum::http::StatusCode> {
    validate_ws_auth(&state, &headers).await?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut tick = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let event = build_dashboard_event(&state).await;
                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            _ = state.inner.cancel_token.cancelled() => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }
}

async fn build_dashboard_event(state: &AppState) -> WsEvent {
    let reseed_svc = ReseedStatsService::new(state.inner.db_pool.clone());
    let user_svc = UserInfoService::new(state.inner.db_pool.clone());

    let overview = reseed_svc.get_overview().await.ok();
    let mut site_stats = reseed_svc.get_site_reseed_stats().await.ok();
    let user_info = user_svc.get_aggregated_user_info().await.ok();

    if let Some(stats) = site_stats.as_mut() {
        let registry = state.site_registry_snapshot().await;
        for site in stats {
            site.breaker_status = match registry.get(&SiteId::from(site.site_id)) {
                Some(handle) if handle.rate_limiter.is_circuit_open().await => {
                    "tripped".to_string()
                }
                Some(_) => "ok".to_string(),
                None => "unknown".to_string(),
            };
        }
    }

    WsEvent::DashboardUpdate {
        overview,
        site_stats,
        user_info,
    }
}

// ── WebSocket /ws/logs ──────────────────────────────────────────────────

pub async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, axum::http::StatusCode> {
    validate_ws_auth(&state, &headers).await?;
    Ok(ws.on_upgrade(move |socket| handle_logs_socket(socket, state)))
}

async fn handle_logs_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.inner.log_broadcast.subscribe();

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(line) => {
                        let event = WsEvent::LogLine { line };
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            _ = state.inner.cancel_token.cancelled() => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }
}
