//! Logging initialisation using `tracing` with file output.
//!
//! Writes structured logs to `<config_dir>/<log.file>`. The log file is
//! truncated on startup if it exceeds 1 MB.

use std::fs::{self, File};
use std::path::Path;

use tracing_subscriber::EnvFilter;

use crate::config::LogConfig;

/// Maximum log file size before truncation on startup (1 MB).
const MAX_LOG_SIZE: u64 = 1_024 * 1_024;

/// Truncate the log file if it exceeds [`MAX_LOG_SIZE`].
///
/// This runs once at startup to prevent unbounded log growth.
fn truncate_if_oversized(path: &Path) {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_LOG_SIZE => {
            if let Err(e) = fs::write(path, b"") {
                eprintln!("warning: failed to truncate log file: {e}");
            }
        }
        _ => {}
    }
}

/// Initialise the `tracing` subscriber writing to a log file.
///
/// - Log level is taken from `config.level` (e.g. "warn", "debug").
/// - The file is created (or truncated if >1 MB) at `config_dir/<config.file>`.
/// - Returns an error description if setup fails, but the app can continue
///   without logging.
pub fn init_logging(config_dir: &Path, config: &LogConfig) -> Result<(), String> {
    let log_path = config_dir.join(&config.file);

    truncate_if_oversized(&log_path);

    let file = File::options()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("failed to open log file {}: {e}", log_path.display()))?;

    let level_str = config.level.to_string();
    let filter = EnvFilter::try_new(&level_str)
        .map_err(|e| format!("invalid log level '{}': {e}", level_str))?;

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .with_target(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| format!("failed to set tracing subscriber: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn truncate_if_oversized_leaves_small_files_alone() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("small.log");
        fs::write(&path, "some log output").unwrap();

        truncate_if_oversized(&path);

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "some log output");
    }

    #[test]
    fn truncate_if_oversized_clears_large_files() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("big.log");
        // Write just over 1 MB
        let data = vec![b'x'; (MAX_LOG_SIZE + 1) as usize];
        fs::write(&path, &data).unwrap();

        truncate_if_oversized(&path);

        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), 0);
    }

    #[test]
    fn truncate_if_oversized_handles_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.log");
        // Should not panic
        truncate_if_oversized(&path);
    }

    #[test]
    fn truncate_if_oversized_keeps_exactly_1mb() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("exact.log");
        let data = vec![b'x'; MAX_LOG_SIZE as usize];
        fs::write(&path, &data).unwrap();

        truncate_if_oversized(&path);

        // Exactly at the limit — should NOT truncate
        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), MAX_LOG_SIZE);
    }
}
