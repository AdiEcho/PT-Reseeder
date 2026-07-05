use crate::api;
use crate::auth::{csrf_check, require_auth};
use crate::state::AppState;
use crate::ws;
use axum::{routing::get, Router};

pub fn build_router(state: AppState) -> Router {
    // Routes that require authentication
    let authed_routes = Router::new()
        .merge(api::sites::router())
        .merge(api::downloaders::router())
        .merge(api::tasks::router())
        .merge(api::folders::router())
        .merge(api::repost::router())
        .merge(api::stats::router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    // Public routes (auth endpoints + health check)
    let public_routes = Router::new()
        .merge(api::auth::router())
        .merge(api::health::router());

    let api_routes = Router::new()
        .merge(public_routes)
        .merge(authed_routes)
        .layer(axum::middleware::from_fn(csrf_check));

    // TODO: Leptos SSR integration will be finalized when frontend crate is complete.
    Router::new()
        .nest("/api", api_routes)
        .route("/ws/dashboard", get(ws::ws_handler))
        .with_state(state)
}
