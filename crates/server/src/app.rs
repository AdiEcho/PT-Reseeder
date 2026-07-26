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

/// Server function endpoints are generated as `/api/{fn_name}{hash}`, where the
/// hash is derived from `module_path!()` by the `#[server]` macro. Strip that
/// trailing hash so the public-endpoint allowlist matches the function name.
///
/// No server function in this crate ends in a digit, so trimming trailing digits
/// cannot truncate a real name.
fn server_fn_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(|c: char| c.is_ascii_digit())
}

fn server_fn_requires_auth(path: &str) -> bool {
    !matches!(
        server_fn_name(path),
        "login" | "register" | "get_current_user" | "has_user"
    )
}

async fn validate_server_fn_request(
    state: &AppState,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> Result<Option<i64>, StatusCode> {
    // No `X-PT-Reseeder` check here. Server functions are issued by the stock
    // Leptos browser client, which sends only `Content-Type` and `Accept`
    // (`server_fn::request::browser`), so requiring a custom header would reject
    // every call the app makes. CSRF is covered instead by the session cookie's
    // `SameSite=Strict` attribute (see `create_session_cookie`), which the
    // browser enforces on cross-site requests.
    //
    // The hand-written REST routes under `/api/…` keep their own `csrf_check`
    // layer, because `pages/repost.rs` sends the header explicitly for those.
    let _ = method;

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

/// Guards the server-function endpoints that `leptos_routes_with_context`
/// registers.
///
/// `leptos_routes_with_context` auto-registers every `#[server]` function as an
/// *exact* route (`server_fn::axum::server_fn_paths()`), and in axum an exact
/// route wins over the `/api/{*fn_name}` wildcard. Those generated routes call
/// `handle_server_fns_with_context` directly, so `server_fn_handler` — and the
/// auth and CSRF checks inside it — never run for a real server function. Only
/// *unregistered* paths fall through to the wildcard, which is why an unknown
/// endpoint used to 401 while every real one answered unauthenticated.
///
/// Applying the same validation as a layer closes that gap: a layer runs for
/// whichever route matched.
async fn guard_server_fns(
    State(state): State<AppState>,
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path();

    // Hand-written REST endpoints under /api are nested separately and carry
    // their own `require_auth` / `csrf_check` layers; re-checking here would
    // double-guard them and reject the public auth routes.
    if !is_server_fn_path(path) {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let path = path.to_string();
    let headers = request.headers().clone();
    match validate_server_fn_request(&state, &method, &path, &headers).await {
        Ok(_) => next.run(request).await,
        Err(status) => status.into_response(),
    }
}

/// Whether `path` is one of the registered `#[server]` endpoints.
///
/// The layer runs on every request, including static assets and SSR page loads,
/// so the registry is collected into a set once rather than rescanned per call.
fn is_server_fn_path(path: &str) -> bool {
    static PATHS: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    PATHS
        .get_or_init(|| {
            leptos::server_fn::axum::server_fn_paths()
                .map(|(registered, _)| registered)
                .collect()
        })
        .contains(path)
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
        // Applied last so it wraps the routes `leptos_routes_with_context`
        // registered above, which is where the server functions actually live.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard_server_fns,
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated endpoints carry a `module_path!()`-derived hash suffix, so
    /// the allowlist has to match on the name with that suffix stripped.
    #[test]
    fn allowlist_matches_hashed_endpoints() {
        const HASH: &str = "13234041467895400166";

        for name in ["login", "register", "get_current_user", "has_user"] {
            let path = format!("/api/{name}{HASH}");
            assert!(!server_fn_requires_auth(&path), "{path} should be public");
        }

        for name in ["get_sites", "delete_site", "create_task", "logout"] {
            let path = format!("/api/{name}{HASH}");
            assert!(
                server_fn_requires_auth(&path),
                "{path} must stay authenticated"
            );
        }
    }

    /// A name that merely starts with a public one must not be let through.
    #[test]
    fn allowlist_does_not_match_on_prefix() {
        for path in ["/api/login_v2", "/api/relogin", "/api/loginX", "/api/"] {
            assert!(
                server_fn_requires_auth(path),
                "{path} must stay authenticated"
            );
        }
    }

    /// The stock Leptos browser client sends only `Content-Type` and `Accept`
    /// (`server_fn::request::browser`), so `validate_server_fn_request` must not
    /// demand a custom header. Requiring `X-PT-Reseeder` here previously made
    /// every authenticated call — including logout — return 403 in a real
    /// browser while still passing header-carrying curl tests.
    #[test]
    fn validation_does_not_depend_on_a_custom_header() {
        let src = include_str!("app.rs");
        let body = src
            .split_once("async fn validate_server_fn_request")
            .expect("validator present")
            .1
            .split_once("\nasync fn ")
            .map(|(body, _)| body)
            .unwrap_or(src);
        let code: String = body
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        assert!(
            !code.contains("X-PT-Reseeder"),
            "server-fn validation must not require a custom header; the Leptos \
             client cannot send one"
        );
    }

    /// Guards the regression this middleware exists for: `leptos_routes_with_context`
    /// registers each server function as an exact route, which takes precedence
    /// over the `/api/{*fn_name}` wildcard and therefore bypasses
    /// `server_fn_handler`. If the registry ever comes back empty, the layer
    /// would wave every request through instead of validating it.
    #[test]
    fn server_fn_registry_is_populated() {
        let count = leptos::server_fn::axum::server_fn_paths().count();
        assert!(
            count > 0,
            "no server functions registered; guard_server_fns would be a no-op"
        );
    }
}
