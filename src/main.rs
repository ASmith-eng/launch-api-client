mod api;
mod cache;
mod clock;
mod config;
mod error;
mod logging;
mod models;
mod tui;
mod vendor;

use config::{load_config, AppDirs};

fn main() {
    let dirs = AppDirs::resolve().expect("failed to resolve platform directories");
    if let Err(e) = dirs.ensure_dirs() {
        eprintln!("warning: failed to create app directories: {e}");
    }

    let config = load_config(&dirs.config_dir.join("config.toml"));

    if let Err(e) = logging::init_logging(&dirs.config_dir, &config.log) {
        eprintln!("warning: logging setup failed: {e}");
    }

    tracing::info!("launch-client starting");
    println!("launch-client");
}
