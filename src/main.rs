mod api;
mod cache;
mod clock;
mod config;
mod error;
mod logging;
mod models;
mod tui;
mod vendor;

use std::pin::Pin;

use chrono::Utc;
use tracing::{info, warn};

use api::client::{LaunchApi, LaunchListResponse, Ll2Client};
use api::rate_limiter::RateLimiter;
use cache::CacheManager;
use clock::{Clock, SystemClock};
use config::{load_config, AppDirs};
use models::{AppState, CACHE_VERSION};
use tui::app::App;
use tui::event::{FetchResult, run_event_loop};
use tui::terminal::{install_panic_hook, setup_terminal};
use vendor::launch_library_2::endpoints::ListParams;

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

    info!("launch-client starting");

    let clock = SystemClock;

    // 2. Initialize cache manager and load app state.
    let cache_manager = CacheManager::new(dirs.cache_dir.clone(), clock);

    let mut app_state = cache_manager
        .load_app_state()
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
        info!("running cache pruning (startup #{})", app_state.startup_count);
        if let Err(e) = cache_manager.prune_details(&config.cache) {
            warn!(error = %e, "cache pruning failed");
        }
        app_state.last_prune = Some(Utc::now());
    }

    // 4. Create API client with rate limiter from persisted state.
    let authenticated = !config.api.api_key.is_empty();
    let rate_limiter = RateLimiter::from_state(clock, &app_state.rate_limit);
    let api_client = Ll2Client::new(
        config.api.base_url.clone(),
        if authenticated {
            Some(config.api.api_key.clone())
        } else {
            None
        },
        rate_limiter,
    )?;

    // 5. Sync rate limit with /api-throttle/ (best-effort).
    if let Err(e) = api_client.sync_throttle_with_retry().await {
        warn!(error = %e, "startup throttle sync failed, continuing with local tracking");
    }

    // 6. Load launch list from cache or prepare to fetch.
    let cached_list = cache_manager.load_launch_list().unwrap_or_else(|e| {
        warn!(error = %e, "failed to load cache.json");
        None
    });

    let (initial_launches, initial_total, needs_fetch) = match cached_list {
        Some(ref cache) if !cache.is_stale(clock.now()) => {
            info!(
                count = cache.launches.len(),
                "loaded fresh launch list from cache"
            );
            (cache.launches.clone(), cache.total_count, false)
        }
        Some(ref cache) => {
            info!("cached launch list is stale, will re-fetch");
            (cache.launches.clone(), cache.total_count, true)
        }
        None => {
            info!("no cached launch list, will fetch");
            (vec![], 0, true)
        }
    };

    // 7. Set up terminal and TUI.
    install_panic_hook();
    let (mut terminal, _guard) = setup_terminal()?;

    let size = terminal.size()?;
    let mut app = App::new((size.width, size.height));
    app.launches = initial_launches;
    app.total_count = initial_total;

    // 8. If we need to fetch, start the request and mark as loading.
    let pending_fetch: Option<Pin<Box<dyn std::future::Future<Output = FetchResult> + Send>>> =
        if needs_fetch {
            app.loading = true;
            let params = ListParams {
                limit: config.ui.launches_per_page,
                ..Default::default()
            };
            // Spawn the fetch as a future we can select on.
            Some(Box::pin(fetch_launch_list(api_client, params)))
        } else {
            None
        };

    // 9. Run the event loop.
    let result = run_event_loop(&mut terminal, &mut app, pending_fetch).await;

    // 10. Save app state on exit.
    // Note: api_client was moved into the fetch future if needs_fetch was true.
    // Persisting rate limit state will be fully wired in Step 8.1 when we
    // keep the client accessible throughout the event loop.
    app_state.last_startup = Utc::now();
    if let Err(e) = cache_manager.save_app_state(&app_state) {
        warn!(error = %e, "failed to save app_state.json on exit");
    }

    // _guard dropped here → terminal restored.
    result.map_err(Into::into)
}

/// Fetch the launch list, returning a [`FetchResult`] for the event loop.
async fn fetch_launch_list<C: clock::Clock + Send + Sync + 'static>(
    client: Ll2Client<C>,
    params: ListParams,
) -> FetchResult {
    match client.fetch_launch_list(&params).await {
        Ok(LaunchListResponse {
            launches,
            total_count,
        }) => FetchResult::LaunchList {
            launches,
            total_count,
        },
        Err(e) => FetchResult::Error(e),
    }
}
