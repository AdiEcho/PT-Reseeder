pub mod events;

use crate::auth::{hash_token, SESSION_COOKIE_NAME};
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
    // Validate Origin header for CSRF
    if let Some(origin) = headers.get("origin") {
        let origin_str = origin.to_str().unwrap_or("");
        let bind = state.inner.config.server_bind;
        let expected_origins = [
            format!("http://127.0.0.1:{}", bind.port()),
            format!("http://localhost:{}", bind.port()),
        ];
        if !expected_origins.iter().any(|o| origin_str == o) {
            return Err(axum::http::StatusCode::FORBIDDEN);
        }
    }

    // Validate session cookie from headers
    let cookie_header = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let session_token = cookie_header
        .split(';')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, '=');
            let name = parts.next()?;
            let value = parts.next()?;
            if name == SESSION_COOKIE_NAME {
                Some(value.to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let token_hash = hash_token(&session_token).ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let session = state
        .inner
        .repo
        .find_session_by_hash(&token_hash)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    if pt_reseeder_core::session::is_session_expired(&session.expires_at) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

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
