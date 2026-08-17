// REST API endpoints.
//
// Each domain module exports a `pub fn router() -> Router<AppState>` that mounts
// its routes. The top-level `build_router` in `app.rs` assembles them under the
// `/api` prefix with auth and CSRF middleware layers.
pub mod auth;
pub mod config;
pub mod dashboard;
pub mod downloaders;
pub mod folders;
pub mod health;
pub mod logs;
pub mod repost;
pub mod repost_ext;
pub mod sites;
pub mod tasks;
