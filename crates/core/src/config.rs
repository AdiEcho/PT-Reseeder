use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_database_url() {
        let config = AppConfig::default();
        assert_eq!(config.database_url, "sqlite://pt-reseeder.db?mode=rwc");
    }

    #[test]
    fn default_config_has_expected_server_bind() {
        let config = AppConfig::default();
        let expected: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        assert_eq!(config.server_bind, expected);
    }

    #[test]
    fn default_config_has_24_hour_session_ttl() {
        let config = AppConfig::default();
        assert_eq!(config.session_ttl_hours, 24);
    }

    #[test]
    fn default_config_serializes_to_json_and_back() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).expect("serialize should succeed");
        let deserialized: AppConfig =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(deserialized.database_url, config.database_url);
        assert_eq!(deserialized.server_bind, config.server_bind);
        assert_eq!(deserialized.session_ttl_hours, config.session_ttl_hours);
        assert_eq!(deserialized.data_dir, config.data_dir);
        assert_eq!(deserialized.leptos_site_root, config.leptos_site_root);
    }
}
