use std::net::SocketAddr;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub server_bind: SocketAddr,
    pub session_ttl_hours: u64,
    pub data_dir: PathBuf,
    pub leptos_site_root: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite://pt-reseeder.db?mode=rwc".to_string(),
            server_bind: "127.0.0.1:3000".parse().unwrap(),
            session_ttl_hours: 24,
            data_dir: PathBuf::from("data"),
            leptos_site_root: PathBuf::from("target/site"),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| Self::default().database_url),
            server_bind: std::env::var("LEPTOS_SITE_ADDR")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Self::default().server_bind),
            session_ttl_hours: std::env::var("SESSION_TTL_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(24),
            data_dir: std::env::var("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| Self::default().data_dir),
            leptos_site_root: std::env::var("LEPTOS_SITE_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| Self::default().leptos_site_root),
        }
    }
}
