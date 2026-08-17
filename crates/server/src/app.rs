use crate::api;
use crate::auth::{csrf_check, require_auth};
use crate::state::AppState;
use crate::ws;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use tower::ServiceExt;
use tower_http::services::ServeDir;

/// JSON error body for unknown `/api` paths.
#[derive(serde::Serialize)]
struct ApiNotFound {
    error: String,
}

/// Fallback for unmatched `/api/…` routes: returns JSON 404 instead of the SPA
/// index.html that the top-level fallback would serve.
async fn api_fallback() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ApiNotFound {
            error: "not found".to_string(),
        }),
    )
}

/// SPA fallback: serves `index.html` for any non-API, non-WS path so that
/// client-side routing works.
async fn spa_fallback(State(state): State<AppState>, request: Request<Body>) -> Response {
    let site_root = &state.inner.config.site_root;

    // Try to serve the exact static file first.
    let response = ServeDir::new(site_root)
        .oneshot(request)
        .await
        .map(|res| res.into_response());

    match response {
        Ok(res) if res.status() != StatusCode::NOT_FOUND => res,
        // File not found — serve index.html for client-side routing.
        _ => match tokio::fs::read(site_root.join("index.html")).await {
            Ok(body) => Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html; charset=utf-8")
                .body(Body::from(body))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "index.html not found",
            )
                .into_response(),
        },
    }
}

pub fn build_router(state: AppState) -> Router {
    // --- Public auth routes (no session required) ---
    let public_auth = Router::new().merge(api::auth::public_router());

    // --- Authenticated routes ---
    let authed_routes = Router::new()
        .merge(api::auth::authed_router())
        .merge(api::sites::router())
        .merge(api::downloaders::router())
        .merge(api::folders::router())
        .merge(api::tasks::router())
        .merge(api::logs::router())
        .merge(api::repost::router())
        .merge(api::repost_ext::router())
        .merge(api::config::router())
        .merge(api::dashboard::router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    // --- /api subtree: CSRF check wraps everything, auth is selective ---
    let api_routes = Router::new()
        .merge(api::health::router())
        .merge(public_auth)
        .merge(authed_routes)
        .fallback(api_fallback)
        .layer(axum::middleware::from_fn(csrf_check));

    Router::new()
        .nest("/api", api_routes)
        .route("/ws/dashboard", get(ws::ws_handler))
        .route("/ws/logs", get(ws::ws_logs_handler))
        .fallback(spa_fallback)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unknown /api paths must get JSON 404, not the SPA index.html.
    #[test]
    fn api_not_found_shape() {
        // Just a compile-time sanity check that the types work.
        let _: ApiNotFound = ApiNotFound {
            error: "test".to_string(),
        };
    }
}
