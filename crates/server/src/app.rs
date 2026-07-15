use crate::api;
use crate::auth::{csrf_check, require_auth};
use crate::state::AppState;
use crate::ws;
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use leptos::prelude::provide_context;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower::ServiceExt;
use tower_http::services::ServeDir;

fn server_fn_requires_auth(path: &str) -> bool {
    !matches!(
        path.rsplit('/').next().unwrap_or_default(),
        "login" | "register" | "get_current_user" | "has_user"
    )
}

fn server_fn_requires_csrf(method: &Method, path: &str) -> bool {
    server_fn_requires_auth(path)
        && !matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

async fn validate_server_fn_request(
    state: &AppState,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> Result<Option<i64>, StatusCode> {
    if server_fn_requires_csrf(method, path)
        && headers.get("X-PT-Reseeder").map(|v| v == "1") != Some(true)
    {
        return Err(StatusCode::FORBIDDEN);
    }

    if !server_fn_requires_auth(path) {
        return Ok(None);
    }

    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token = cookie_header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == crate::auth::SESSION_COOKIE_NAME).then_some(value))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = crate::auth::hash_token(token).ok_or(StatusCode::UNAUTHORIZED)?;
    let session = state
        .inner
        .repo
        .find_session_by_hash(&token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if pt_reseeder_core::session::is_session_expired(&session.expires_at) {
        let _ = state.inner.repo.delete_session(session.id).await;
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Some(session.user_id))
}

fn provide_server_fn_context(context: pt_reseeder_frontend::server_fns::ServerFnContext) {
    provide_context(context.pool.clone());
    provide_context(context);
}

async fn server_fn_handler(State(state): State<AppState>, request: Request<Body>) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let user_id = match validate_server_fn_request(&state, &method, &path, &headers).await {
        Ok(user_id) => user_id,
        Err(status) => return status.into_response(),
    };
    let context = state.server_fn_context(user_id);
    leptos_axum::handle_server_fns_with_context(
        move || provide_server_fn_context(context.clone()),
        request,
    )
    .await
    .into_response()
}

async fn static_fallback(State(state): State<AppState>, request: Request<Body>) -> Response {
    let site_root = state.inner.config.leptos_site_root.clone();
    ServeDir::new(site_root)
        .oneshot(request)
        .await
        .map(|res| res.into_response())
        .unwrap_or_else(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to serve static asset: {err}"),
            )
                .into_response()
        })
}

pub fn build_router(state: AppState) -> Router {
    let routes = generate_route_list(pt_reseeder_frontend::app::App);
    let leptos_options = state.leptos_options();

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

    Router::new()
        .route("/api/{*fn_name}", post(server_fn_handler))
        .nest("/api", api_routes)
        .route("/ws/dashboard", get(ws::ws_handler))
        .route("/ws/logs", get(ws::ws_logs_handler))
        .fallback(static_fallback)
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let context = state.server_fn_context(None);
                move || provide_server_fn_context(context.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || pt_reseeder_frontend::app::shell(leptos_options.clone())
            },
        )
        .with_state(state)
}
