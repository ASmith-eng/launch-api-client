mod api;
mod cache;
mod clock;
mod config;
mod error;
mod logging;
mod models;
mod tui;
mod vendor;

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, warn};

use api::client::Ll2Client;
use api::rate_limiter::RateLimiter;
use cache::CacheManager;
use clock::{Clock, SystemClock};
use config::{load_config, AppDirs};
use vendor::launch_library_2::endpoints::PROD_BASE_URL;
use models::{AppState, CACHE_VERSION};
use tui::app::App;
use tui::event::run_event_loop;
use tui::terminal::{install_panic_hook, setup_terminal};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Resolve directories and load config.
    let dirs = AppDirs::resolve().expect("failed to resolve platform directories");
    if let Err(e) = dirs.ensure_dirs() {
        eprintln!("warning: failed to create app directories: {e}");
    }

    let config = load_config(&dirs.config_dir.join("config.toml"));

    if let Err(e) = logging::init_logging(&dirs.config_dir, &config.log) {
        eprintln!("warning: logging setup failed: {e}");
    }

    info!(
        base_url = PROD_BASE_URL,
        authenticated = !config.api.api_key.is_empty(),
        log_level = %config.log.level,
        launches_per_page = config.ui.launches_per_page,
        "launch-client starting"
    );

    let clock = SystemClock;

    // 2. Initialize cache manager and load app state.
    let cache_manager = CacheManager::new(dirs.cache_dir.clone(), clock);

    let mut app_state = cache_manager
        .load_app_state()
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "failed to load app_state.json, using defaults");
            None
        })
        .unwrap_or_else(|| {
            info!("no app_state.json found, creating default");
            AppState {
                version: CACHE_VERSION,
                rate_limit: models::RateLimitState {
                    limit: 15,
                    requests: vec![],
                    last_sync: Utc::now(),
                    authenticated: false,
                },
                last_startup: Utc::now(),
                startup_count: 0,
                last_prune: None,
            }
        });

    // Increment startup count.
    app_state.startup_count += 1;
    app_state.last_startup = Utc::now();

    // 3. Maybe prune detail cache.
    if cache::should_prune(app_state.startup_count, &config.cache) {
        info!(
            "running cache pruning (startup #{})",
            app_state.startup_count
        );
        if let Err(e) = cache_manager.prune_details(&config.cache).await {
            warn!(error = %e, "cache pruning failed");
        }
        app_state.last_prune = Some(Utc::now());
    }

    // 4. Create API client with rate limiter from persisted state.
    //    Update the authenticated flag before restoring, so the rate limiter
    //    uses the correct limit (15 unauth / 30 auth) even if the user
    //    added or removed an API key since the last session.
    let authenticated = !config.api.api_key.is_empty();
    app_state.rate_limit.authenticated = authenticated;
    let rate_limiter = RateLimiter::from_state(clock, &app_state.rate_limit);
    let api_client = Arc::new(Ll2Client::new(
        PROD_BASE_URL.to_string(),
        if authenticated {
            Some(config.api.api_key.clone())
        } else {
            None
        },
        rate_limiter,
    )?);

    // 5. Sync rate limit with /api-throttle/ (best-effort).
    if let Err(e) = api_client.sync_throttle_with_retry().await {
        warn!(error = %e, "startup throttle sync failed, continuing with local tracking");
    }

    // 6. Load launch list from cache.
    let cached_list = cache_manager.load_launch_list().await.unwrap_or_else(|e| {
        warn!(error = %e, "failed to load cache.json");
        None
    });

    let needs_refresh = match cached_list {
        Some(ref cache) if !cache.is_stale(clock.now()) => {
            info!(
                count = cache.launches.len(),
                "loaded fresh launch list from cache"
            );
            false
        }
        Some(_) => {
            info!("cached launch list is stale, will re-fetch");
            true
        }
        None => {
            info!("no cached launch list, will fetch");
            false // Empty list triggers auto-fetch via event loop state check.
        }
    };

    // 7. Set up terminal and TUI.
    install_panic_hook();
    let (mut terminal, _guard) = setup_terminal()?;

    let size = terminal.size()?;
    let mut app = App::new((size.width, size.height));

    // Populate app with cached data (if any).
    if let Some(ref cache) = cached_list {
        app.launches = cache.launches.clone();
        app.total_count = cache.total_count;
        app.cache_fetched_at = Some(cache.fetched_at);
        app.cache_expires_at = Some(cache.expires_at);
    }

    // Populate rate limit display from current limiter state.
    {
        let mut limiter = api_client.rate_limiter();
        app.rate_limit_remaining = Some(limiter.remaining() as u32);
        app.rate_limit_total = Some(limiter.limit() as u32);
    }

    app.ui_config = config.ui.clone();
    app.launches_per_page = config.ui.launches_per_page;

    // If cache is stale, signal the event loop to refresh on first iteration.
    if needs_refresh {
        app.refresh_requested = true;
    }

    // 8. Run the event loop (drives all fetching based on app state).
    let result = run_event_loop(
        &mut terminal,
        &mut app,
        Arc::clone(&api_client),
        &cache_manager,
        &config.cache,
    )
    .await;

    // 9. Save app state on exit, including current rate limit state.
    {
        let limiter = api_client.rate_limiter();
        app_state.rate_limit = limiter.to_state();
    }
    app_state.last_startup = Utc::now();
    if let Err(e) = cache_manager.save_app_state(&app_state).await {
        warn!(error = %e, "failed to save app_state.json on exit");
    }

    // _guard dropped here → terminal restored.
    result.map_err(Into::into)
}
