//! Configuration loading with sensible defaults.
//!
//! The app works out of the box without a config file. If `config.toml` exists
//! in the platform config directory, it is parsed and merged over defaults.
//! Missing keys keep their defaults; invalid values fall back to defaults with
//! a warning logged.

use std::fmt;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Strong types for config values
// ---------------------------------------------------------------------------

/// Time display format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeFormat {
    #[serde(rename = "12h")]
    TwelveHour,
    #[serde(rename = "24h")]
    TwentyFourHour,
}

/// Staleness display style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StalenessStyle {
    Relative,
    Absolute,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        };
        f.write_str(s)
    }
}

/// Top-level application configuration.
///
/// All fields have defaults so the app works without a config file.
/// Constructed via [`load_config`] or [`Config::default`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api: ApiConfig,
    pub cache: CacheConfig,
    pub ui: UiConfig,
    pub log: LogConfig,
}

/// API connection settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    /// Optional API key for higher rate limits (30/hr vs 15/hr).
    pub api_key: String,
}

/// Cache TTL and pruning settings.
///
/// Past launches use a `Permanent` cache strategy internally (data is final
/// and never re-fetched), so no TTL config is exposed for them.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// TTL for launches >7 days away (minutes). Range: 1–43 200 (30 days).
    pub ttl_far_future: u64,
    /// TTL for launches 1–7 days away (minutes). Range: 1–43 200 (30 days).
    pub ttl_near_future: u64,
    /// TTL for launches <24 hours away (minutes). Range: 1–43 200 (30 days).
    pub ttl_imminent: u64,
    /// TTL for in-progress launches (minutes). Range: 1–43 200 (30 days).
    pub ttl_active: u64,
    /// TTL for the launch list (minutes). Range: 1–43 200 (30 days).
    pub ttl_launch_list: u64,
    /// Delete detail cache files older than this many days. Min: 1.
    pub max_detail_age_days: u32,
    /// Maximum number of detail cache files to keep. Min: 1.
    pub max_detail_files: u32,
    /// Run cache pruning every N app startups. Range: 1–50.
    pub prune_every_n_startups: u32,
}

/// UI display preferences.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Time format: `12h` or `24h`.
    pub time_format: TimeFormat,
    /// Staleness display: `relative` or `absolute`.
    pub staleness_style: StalenessStyle,
    /// Number of launches per page.
    pub launches_per_page: u32,
    /// Opt out of the startup splash animation.
    pub disable_startup_splash: bool,
}

/// Logging settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Log level.
    pub level: LogLevel,
    /// Log file name (stored in config_dir).
    pub file: String,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
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
            time_format: TimeFormat::TwelveHour,
            staleness_style: StalenessStyle::Relative,
            launches_per_page: 25,
            disable_startup_splash: false,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Warn,
            file: "app.log".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sanitisation
// ---------------------------------------------------------------------------

const MAX_LAUNCHES_PER_PAGE: u32 = 100;
/// 30 days in minutes — upper bound for all TTL fields.
const MAX_TTL_MINUTES: u64 = 43_200;
/// Upper bound for prune_every_n_startups.
const MAX_PRUNE_STARTUPS: u32 = 50;

impl Config {
    /// Replace invalid or dangerous values with safe defaults.
    ///
    /// Called after deserialization so that well-typed but semantically wrong
    /// values (e.g. `prune_every_n_startups = 0`, path traversal in log file
    /// name) don't cause surprising behaviour at runtime.
    pub fn sanitize(&mut self) {
        self.cache.sanitize();
        self.ui.sanitize();
        self.log.sanitize();
    }
}

impl CacheConfig {
    fn sanitize(&mut self) {
        let defaults = Self::default();

        // TTL values: must be 1–43 200 (30 days)
        sanitize_range(
            "ttl_far_future",
            &mut self.ttl_far_future,
            1,
            MAX_TTL_MINUTES,
            defaults.ttl_far_future,
        );
        sanitize_range(
            "ttl_near_future",
            &mut self.ttl_near_future,
            1,
            MAX_TTL_MINUTES,
            defaults.ttl_near_future,
        );
        sanitize_range(
            "ttl_imminent",
            &mut self.ttl_imminent,
            1,
            MAX_TTL_MINUTES,
            defaults.ttl_imminent,
        );
        sanitize_range("ttl_active", &mut self.ttl_active, 1, MAX_TTL_MINUTES, defaults.ttl_active);
        sanitize_range(
            "ttl_launch_list",
            &mut self.ttl_launch_list,
            1,
            MAX_TTL_MINUTES,
            defaults.ttl_launch_list,
        );

        // Pruning: age and count have a minimum of 1, no enforced max
        sanitize_min_u32(
            "max_detail_age_days",
            &mut self.max_detail_age_days,
            1,
            defaults.max_detail_age_days,
        );
        sanitize_min_u32(
            "max_detail_files",
            &mut self.max_detail_files,
            1,
            defaults.max_detail_files,
        );
        // Prune frequency: 1–50
        sanitize_range_u32(
            "prune_every_n_startups",
            &mut self.prune_every_n_startups,
            1,
            MAX_PRUNE_STARTUPS,
            defaults.prune_every_n_startups,
        );
    }
}

impl UiConfig {
    fn sanitize(&mut self) {
        let defaults = Self::default();

        if self.launches_per_page == 0 || self.launches_per_page > MAX_LAUNCHES_PER_PAGE {
            warn!(
                value = self.launches_per_page,
                default = defaults.launches_per_page,
                "Invalid launches_per_page (must be 1–{MAX_LAUNCHES_PER_PAGE}), using default"
            );
            self.launches_per_page = defaults.launches_per_page;
        }
    }
}

impl LogConfig {
    fn sanitize(&mut self) {
        let defaults = Self::default();

        // Guard against path traversal in log file name
        if self.file.contains('/')
            || self.file.contains('\\')
            || self.file.contains("..")
            || self.file.is_empty()
        {
            warn!(
                value = %self.file,
                "Invalid log file name (must be a plain filename), using default"
            );
            self.file = defaults.file;
        }
    }
}

/// Replace a `u64` field with its default if outside [min, max].
fn sanitize_range(name: &str, value: &mut u64, min: u64, max: u64, default: u64) {
    if *value < min || *value > max {
        warn!(
            field = name,
            value = *value,
            min,
            max,
            default,
            "Config value out of range, using default"
        );
        *value = default;
    }
}

/// Replace a `u32` field with its default if below a minimum.
fn sanitize_min_u32(name: &str, value: &mut u32, min: u32, default: u32) {
    if *value < min {
        warn!(
            field = name,
            value = *value,
            default,
            "Config value below minimum, using default"
        );
        *value = default;
    }
}

/// Replace a `u32` field with its default if outside [min, max].
fn sanitize_range_u32(name: &str, value: &mut u32, min: u32, max: u32, default: u32) {
    if *value < min || *value > max {
        warn!(
            field = name,
            value = *value,
            min,
            max,
            default,
            "Config value out of range, using default"
        );
        *value = default;
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
    let mut config = match std::fs::read_to_string(path) {
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
    };
    config.sanitize();
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert!(config.api.api_key.is_empty());
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.cache.ttl_near_future, 180);
        assert_eq!(config.cache.ttl_imminent, 30);
        assert_eq!(config.cache.ttl_active, 1);
        assert_eq!(config.cache.ttl_launch_list, 30);
        assert_eq!(config.cache.max_detail_age_days, 30);
        assert_eq!(config.cache.max_detail_files, 100);
        assert_eq!(config.cache.prune_every_n_startups, 5);
        assert_eq!(config.ui.time_format, TimeFormat::TwelveHour);
        assert_eq!(config.ui.staleness_style, StalenessStyle::Relative);
        assert_eq!(config.ui.launches_per_page, 25);
        assert_eq!(config.log.level, LogLevel::Warn);
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn missing_file_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, LogLevel::Warn);
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
        assert_eq!(config.cache.ttl_far_future, 720);
        // Unset keys keep defaults
        assert_eq!(config.cache.ttl_near_future, 180);
        assert_eq!(config.cache.ttl_imminent, 30);
        assert_eq!(config.ui.time_format, TimeFormat::TwentyFourHour);
        assert_eq!(config.ui.launches_per_page, 50);
        assert_eq!(config.log.level, LogLevel::Debug);
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
        assert_eq!(config.ui.time_format, TimeFormat::TwentyFourHour);
        // Everything else is defaults
        assert!(config.api.api_key.is_empty());
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, LogLevel::Warn);
        assert_eq!(config.ui.launches_per_page, 25);
    }

    #[test]
    fn invalid_toml_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is not valid toml {{{{").unwrap();

        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 1440);
        assert_eq!(config.log.level, LogLevel::Warn);
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

    // =================================================================
    // Sanitisation tests
    // =================================================================

    #[test]
    fn sanitize_zero_ttl_values_reset_to_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
ttl_far_future = 0
ttl_near_future = 0
ttl_imminent = 0
ttl_active = 0
ttl_launch_list = 0
"#,
        )
        .unwrap();

        let config = load_config(&path);
        let defaults = CacheConfig::default();
        assert_eq!(config.cache.ttl_far_future, defaults.ttl_far_future);
        assert_eq!(config.cache.ttl_near_future, defaults.ttl_near_future);
        assert_eq!(config.cache.ttl_imminent, defaults.ttl_imminent);
        assert_eq!(config.cache.ttl_active, defaults.ttl_active);
        assert_eq!(config.cache.ttl_launch_list, defaults.ttl_launch_list);
    }

    #[test]
    fn sanitize_ttl_over_max_reset_to_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
ttl_far_future = 99999
ttl_near_future = 50000
ttl_launch_list = 43201
"#,
        )
        .unwrap();

        let config = load_config(&path);
        let defaults = CacheConfig::default();
        assert_eq!(config.cache.ttl_far_future, defaults.ttl_far_future);
        assert_eq!(config.cache.ttl_near_future, defaults.ttl_near_future);
        assert_eq!(config.cache.ttl_launch_list, defaults.ttl_launch_list);
    }

    #[test]
    fn sanitize_ttl_at_max_boundary_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
ttl_far_future = 43200
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 43200);
    }

    #[test]
    fn sanitize_prune_startups_over_max_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
prune_every_n_startups = 51
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(
            config.cache.prune_every_n_startups,
            CacheConfig::default().prune_every_n_startups
        );
    }

    #[test]
    fn sanitize_prune_startups_at_max_boundary_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
prune_every_n_startups = 50
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.cache.prune_every_n_startups, 50);
    }

    #[test]
    fn sanitize_zero_pruning_values_reset_to_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
prune_every_n_startups = 0
max_detail_files = 0
max_detail_age_days = 0
"#,
        )
        .unwrap();

        let config = load_config(&path);
        let defaults = CacheConfig::default();
        assert_eq!(config.cache.prune_every_n_startups, defaults.prune_every_n_startups);
        assert_eq!(config.cache.max_detail_files, defaults.max_detail_files);
        assert_eq!(config.cache.max_detail_age_days, defaults.max_detail_age_days);
    }

    #[test]
    fn sanitize_valid_ttl_values_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[cache]
ttl_far_future = 720
ttl_near_future = 60
ttl_imminent = 10
ttl_active = 2
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.cache.ttl_far_future, 720);
        assert_eq!(config.cache.ttl_near_future, 60);
        assert_eq!(config.cache.ttl_imminent, 10);
        assert_eq!(config.cache.ttl_active, 2);
    }

    #[test]
    fn invalid_enum_values_fall_back_to_full_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        // Invalid enum values cause a TOML parse failure → full defaults.
        fs::write(
            &path,
            r#"
[ui]
time_format = "25h"
staleness_style = "funky"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.ui.time_format, TimeFormat::TwelveHour);
        assert_eq!(config.ui.staleness_style, StalenessStyle::Relative);
    }

    #[test]
    fn sanitize_launches_per_page_zero_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[ui]
launches_per_page = 0
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.ui.launches_per_page, 25);
    }

    #[test]
    fn sanitize_launches_per_page_over_max_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[ui]
launches_per_page = 500
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.ui.launches_per_page, 25);
    }

    #[test]
    fn invalid_log_level_falls_back_to_full_defaults() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        // Invalid enum value causes a TOML parse failure → full defaults.
        fs::write(
            &path,
            r#"
[log]
level = "verbose"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.log.level, LogLevel::Warn);
    }

    #[test]
    fn sanitize_log_file_path_traversal_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[log]
file = "../../../etc/passwd"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn sanitize_log_file_with_slashes_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[log]
file = "/tmp/evil.log"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn sanitize_empty_log_file_reset_to_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[log]
file = ""
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.log.file, "app.log");
    }

    #[test]
    fn sanitize_valid_log_config_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[log]
level = "debug"
file = "my-app.log"
"#,
        )
        .unwrap();

        let config = load_config(&path);
        assert_eq!(config.log.level, LogLevel::Debug);
        assert_eq!(config.log.file, "my-app.log");
    }

    // =================================================================
    // Directory tests
    // =================================================================

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

    // --- Splash opt-out ---

    fn ui_config_from(toml: &str) -> UiConfig {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, toml).unwrap();
        load_config(&path).ui
    }

    #[test]
    fn splash_is_on_when_the_key_is_omitted() {
        let ui = ui_config_from(
            r#"
[ui]
time_format = "24h"
"#,
        );
        assert!(!ui.disable_startup_splash);
    }

    #[test]
    fn splash_is_on_when_there_is_no_ui_table_at_all() {
        assert!(!ui_config_from("").disable_startup_splash);
        assert!(!UiConfig::default().disable_startup_splash);
    }

    #[test]
    fn splash_can_be_turned_off() {
        let ui = ui_config_from(
            r#"
[ui]
disable_startup_splash = true
"#,
        );
        assert!(ui.disable_startup_splash);
        // Sibling keys keep their defaults.
        assert_eq!(ui.launches_per_page, 25);
    }
}
