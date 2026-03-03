//! Configuration loading with sensible defaults.
//!
//! The app works out of the box without a config file. If `config.toml` exists
//! in the platform config directory, it is parsed and merged over defaults.
//! Missing keys keep their defaults; invalid values fall back to defaults with
//! a warning logged.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::Deserialize;
use tracing::warn;

use crate::error::AppError;

/// Top-level application configuration.
///
/// All fields have defaults so the app works without a config file.
/// Constructed via [`load_config`] or [`Config::default`].
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api: ApiConfig,
    pub cache: CacheConfig,
    pub ui: UiConfig,
    pub log: LogConfig,
}

/// API connection settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    /// Optional API key for higher rate limits (30/hr vs 15/hr).
    pub api_key: String,
    /// Base URL for the Launch Library 2 API.
    pub base_url: String,
}

/// Cache TTL and pruning settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// TTL for past launches in minutes (0 = never expires).
    pub ttl_past_launches: u64,
    /// TTL for launches >7 days away (minutes).
    pub ttl_far_future: u64,
    /// TTL for launches 1–7 days away (minutes).
    pub ttl_near_future: u64,
    /// TTL for launches <24 hours away (minutes).
    pub ttl_imminent: u64,
    /// TTL for in-progress launches (minutes).
    pub ttl_active: u64,
    /// TTL for the launch list (minutes).
    pub ttl_launch_list: u64,
    /// Delete detail cache files older than this many days.
    pub max_detail_age_days: u32,
    /// Maximum number of detail cache files to keep.
    pub max_detail_files: u32,
    /// Run cache pruning every N app startups.
    pub prune_every_n_startups: u32,
}

/// UI display preferences.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Time format: "12h" or "24h".
    pub time_format: String,
    /// Staleness display: "relative" or "absolute".
    pub staleness_style: String,
    /// Number of launches per page.
    pub launches_per_page: u32,
}

/// Logging settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Log level: "error", "warn", "info", "debug", "trace".
    pub level: String,
    /// Log file name (stored in config_dir).
    pub file: String,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for Config {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            cache: CacheConfig::default(),
            ui: UiConfig::default(),
            log: LogConfig::default(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://lldev.thespacedevs.com/2.3.0".into(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl_past_launches: 0,
            ttl_far_future: 1440,
            ttl_near_future: 180,
            ttl_imminent: 30,
            ttl_active: 1,
            ttl_launch_list: 30,
            max_detail_age_days: 30,
            max_detail_files: 100,
            prune_every_n_startups: 5,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            time_format: "12h".into(),
            staleness_style: "relative".into(),
            launches_per_page: 25,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "warn".into(),
            file: "app.log".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Directory resolution
// ---------------------------------------------------------------------------

/// Resolved directory paths for the application.
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppDirs {
    /// Resolve platform-appropriate directories using the `directories` crate.
    ///
    /// Returns `None` if the platform has no valid home directory.
    pub fn resolve() -> Option<Self> {
        let proj = ProjectDirs::from("", "", "launch-client")?;
        Some(Self {
            config_dir: proj.config_dir().to_path_buf(),
            cache_dir: proj.cache_dir().to_path_buf(),
        })
    }

    /// Ensure both config and cache directories exist on disk.
    pub fn ensure_dirs(&self) -> Result<(), AppError> {
        std::fs::create_dir_all(&self.config_dir).map_err(AppError::CacheIo)?;
        std::fs::create_dir_all(&self.cache_dir).map_err(AppError::CacheIo)?;
        // The details/ subdirectory inside cache_dir
        std::fs::create_dir_all(self.cache_dir.join("details")).map_err(AppError::CacheIo)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Load configuration from a TOML file at the given path.
///
/// - If the file does not exist, returns `Config::default()`.
/// - If the file exists but cannot be parsed, logs a warning and returns
///   `Config::default()`.
/// - If the file is partially filled, missing keys use their defaults
///   (thanks to `#[serde(default)]` on every struct).
pub fn load_config(path: &Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(config) => config,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to parse config file, using defaults"
                );
                Config::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to read config file, using defaults"
            );
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert_eq!(config.api.base_url, "https://lldev.thespacedevs.com/2.3.0");
        assert!(config.api.api_key.is_empty());
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.cache.ttl_near_future, 180);
        assert_eq!(config.cache.ttl_imminent, 30);
        assert_eq!(config.cache.ttl_active, 1);
        assert_eq!(config.cache.ttl_launch_list, 30);
        assert_eq!(config.cache.max_detail_age_days, 30);
        assert_eq!(config.cache.max_detail_files, 100);
        assert_eq!(config.cache.prune_every_n_startups, 5);
        assert_eq!(config.ui.time_format, "12h");
        assert_eq!(config.ui.staleness_style, "relative");
        assert_eq!(config.ui.launches_per_page, 25);
        assert_eq!(config.log.level, "warn");
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn missing_file_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, "warn");
    }

    #[test]
    fn valid_file_overrides_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[api]
api_key = "my-secret-key"
base_url = "https://ll.thespacedevs.com/2.3.0"

[cache]
ttl_far_future = 720

[ui]
time_format = "24h"
launches_per_page = 50

[log]
level = "debug"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.api.api_key, "my-secret-key");
        assert_eq!(config.api.base_url, "https://ll.thespacedevs.com/2.3.0");
        assert_eq!(config.cache.ttl_far_future, 720);
        // Unset keys keep defaults
        assert_eq!(config.cache.ttl_near_future, 180);
        assert_eq!(config.cache.ttl_imminent, 30);
        assert_eq!(config.ui.time_format, "24h");
        assert_eq!(config.ui.launches_per_page, 50);
        assert_eq!(config.log.level, "debug");
        // Unset log.file keeps default
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn partial_file_fills_missing_keys_with_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[ui]
time_format = "24h"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        // Overridden
        assert_eq!(config.ui.time_format, "24h");
        // Everything else is defaults
        assert!(config.api.api_key.is_empty());
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, "warn");
        assert_eq!(config.ui.launches_per_page, 25);
    }

    #[test]
    fn invalid_toml_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is not valid toml {{{{").unwrap();

        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, "warn");
    }

    #[test]
    fn wrong_types_return_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
ttl_far_future = "not a number"
"#,
        )
        .unwrap();

        // toml will fail to parse the whole file when a type is wrong,
        // so we fall back to full defaults
        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 1440);
    }

    #[test]
    fn ensure_dirs_creates_directories() {
        let dir = TempDir::new().unwrap();
        let app_dirs = AppDirs {
            config_dir: dir.path().join("config"),
            cache_dir: dir.path().join("cache"),
        };

        assert!(!app_dirs.config_dir.exists());
        assert!(!app_dirs.cache_dir.exists());

        app_dirs.ensure_dirs().unwrap();

        assert!(app_dirs.config_dir.is_dir());
        assert!(app_dirs.cache_dir.is_dir());
        assert!(app_dirs.cache_dir.join("details").is_dir());
    }

    #[test]
    fn ensure_dirs_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let app_dirs = AppDirs {
            config_dir: dir.path().join("config"),
            cache_dir: dir.path().join("cache"),
        };

        app_dirs.ensure_dirs().unwrap();
        // Second call should not fail
        app_dirs.ensure_dirs().unwrap();
        assert!(app_dirs.config_dir.is_dir());
    }
}
