use axum::Router;
use crate::state::AppState;
use crate::auth::{csrf_check, require_auth};
use crate::api;

pub fn build_router(state: AppState) -> Router {
    // Routes that require authentication
    let authed_routes = Router::new()
        .merge(api::sites::router())
        .merge(api::downloaders::router())
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
        .with_state(state)
}
