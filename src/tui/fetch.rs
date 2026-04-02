//! Fetch dispatch, async fetch futures, and result handling.
//!
//! The event loop delegates all data-fetching decisions here. Key
//! entry points:
//!
//! - [`check_needs_fetch`] — state-driven decision on whether to fetch
//! - [`spawn_fetch`] — creates a pinned future for the given fetch kind
//! - [`handle_fetch_result`] — processes completed fetches, updates app
//!   state and persists to cache
//! - [`dismiss_expired_errors`] — auto-clears transient errors after timeout
//! - [`maybe_load_detail_from_disk`] — disk-cache fallback before fetch

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tracing::{debug, warn};

use crate::api::client::{LaunchApi, LaunchListResponse, Ll2Client};
use crate::cache::{self, CacheManager, CacheStrategy};
use crate::clock::Clock;
use crate::config::CacheConfig;
use crate::error::{AppError, ErrorState};
use crate::models::{LaunchDetailCache, LaunchListCache, CACHE_VERSION};
use crate::tui::app::{App, AppScreen};
use crate::vendor::launch_library_2::endpoints::ListParams;

/// Duration after which a transient error is auto-dismissed (10 seconds).
pub const TRANSIENT_DISMISS_SECS: u64 = 10;

// ---------------------------------------------------------------------------
// Fetch dispatch
// ---------------------------------------------------------------------------

/// What kind of fetch to perform.
#[derive(Debug, Clone)]
pub enum FetchKind {
    LaunchList,
    LaunchDetail(String),
    /// Proactive rate limit sync via `/api-throttle/`.
    ThrottleSync,
}

/// Result from an async fetch operation dispatched from the event loop.
#[derive(Debug)]
pub enum FetchResult {
    /// Launch list fetched successfully.
    LaunchList {
        launches: Vec<crate::models::LaunchSummary>,
        total_count: u32,
    },
    /// Launch detail fetched successfully.
    LaunchDetail {
        launch_id: String,
        detail: Box<crate::models::LaunchDetail>,
    },
    /// Throttle sync completed successfully.
    ThrottleSync,
    /// Fetch failed.
    Error(AppError),
}

/// Check whether a proactive rate limit sync is needed.
///
/// Returns `Some(ThrottleSync)` when the rate limiter's `should_sync()`
/// triggers (remaining <= 2, never synced, or >1 hour since last sync).
/// Only fires when no fetch is in progress and the app isn't in an error state.
pub fn check_needs_throttle_sync<C: Clock>(app: &App, client: &Ll2Client<C>) -> Option<FetchKind> {
    if app.loading || app.error_state.is_some() {
        return None;
    }
    if client.rate_limiter().should_sync() {
        debug!("proactive throttle sync needed");
        Some(FetchKind::ThrottleSync)
    } else {
        None
    }
}

/// Determine if the current screen state requires a fetch.
///
/// Returns `None` if no fetch is needed (data is present and fresh,
/// or a fetch is already in-flight, or an error is being displayed).
pub fn check_needs_fetch(app: &App) -> Option<FetchKind> {
    if app.loading || app.error_state.is_some() {
        return None;
    }

    // Manual refresh takes priority — validated against staleness + rate limit.
    if app.refresh_requested {
        return check_refresh(app);
    }

    // Auto-fetch based on screen state.
    match active_screen(&app.screen) {
        // List: auto-fetch only when there's no data at all (first run).
        AppScreen::List => {
            if app.launches.is_empty() {
                debug!("auto-fetching list (no data loaded)");
                Some(FetchKind::LaunchList)
            } else {
                None
            }
        }
        // Detail: auto-fetch when in-memory cache is missing or stale.
        AppScreen::Detail(id) => {
            let needs = match app.detail_cache.get(id) {
                None => {
                    debug!(launch_id = %id, "detail not in memory cache, will fetch");
                    true
                }
                Some(cached) if cached.is_stale(Utc::now()) => {
                    debug!(launch_id = %id, "detail cache is stale, will re-fetch");
                    true
                }
                _ => false,
            };
            if needs {
                Some(FetchKind::LaunchDetail(id.clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Validate a manual refresh request against staleness and rate limiting.
fn check_refresh(app: &App) -> Option<FetchKind> {
    // Don't allow refresh while rate limited.
    if matches!(app.error_state, Some(ErrorState::RateLimited { .. })) {
        return None;
    }

    match active_screen(&app.screen) {
        AppScreen::List => {
            let is_stale = match app.cache_expires_at {
                Some(expires) => Utc::now() >= expires,
                None => true,
            };
            if is_stale {
                Some(FetchKind::LaunchList)
            } else {
                None
            }
        }
        AppScreen::Detail(id) => {
            let is_stale = match app.detail_cache.get(id) {
                None => true,
                Some(cached) => cached.is_stale(Utc::now()),
            };
            if is_stale {
                Some(FetchKind::LaunchDetail(id.clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Resolve the "real" screen when a help overlay is active.
fn active_screen(screen: &AppScreen) -> &AppScreen {
    match screen {
        AppScreen::Help(inner) => active_screen(inner),
        other => other,
    }
}

/// If we're on a detail screen without in-memory data, try loading from disk.
pub fn maybe_load_detail_from_disk<C: Clock>(app: &mut App, cache_manager: &CacheManager<C>) {
    if let AppScreen::Detail(id) = active_screen(&app.screen) {
        if !app.detail_cache.contains_key(id) {
            match cache_manager.load_launch_detail(id) {
                Ok(Some(cached)) => {
                    debug!(launch_id = %id, "loaded detail from disk cache");
                    app.detail_cache.insert(id.clone(), cached);
                }
                Ok(None) => {
                    debug!(launch_id = %id, "detail not in disk cache, will fetch");
                }
                Err(e) => {
                    warn!(error = %e, launch_id = %id, "failed to load detail from disk cache");
                }
            }
        }
    }
}

/// Create a pinned future for the given fetch kind.
pub fn spawn_fetch<C: Clock + Send + Sync + 'static>(
    kind: FetchKind,
    client: &Arc<Ll2Client<C>>,
    app: &App,
) -> Pin<Box<dyn Future<Output = FetchResult> + Send>> {
    match kind {
        FetchKind::LaunchList => {
            let mut params = ListParams {
                limit: app.launches_per_page,
                offset: app.page_offset(),
                ..Default::default()
            };
            app.filter_state.apply_to_params(&mut params, Utc::now());
            let c = Arc::clone(client);
            Box::pin(fetch_launch_list(c, params))
        }
        FetchKind::LaunchDetail(id) => {
            let c = Arc::clone(client);
            Box::pin(fetch_launch_detail(c, id))
        }
        FetchKind::ThrottleSync => {
            let c = Arc::clone(client);
            Box::pin(async move {
                match c.sync_throttle_with_retry().await {
                    Ok(_) => FetchResult::ThrottleSync,
                    Err(e) => {
                        // Graceful degradation — log and continue with local tracking.
                        warn!(error = %e, "proactive throttle sync failed");
                        FetchResult::ThrottleSync
                    }
                }
            })
        }
    }
}

/// Fetch the launch list, returning a [`FetchResult`] for the event loop.
async fn fetch_launch_list<C: Clock + Send + Sync + 'static>(
    client: Arc<Ll2Client<C>>,
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

/// Fetch a launch detail, returning a [`FetchResult`] for the event loop.
async fn fetch_launch_detail<C: Clock + Send + Sync + 'static>(
    client: Arc<Ll2Client<C>>,
    launch_id: String,
) -> FetchResult {
    match client.fetch_launch_detail(&launch_id).await {
        Ok(detail) => FetchResult::LaunchDetail {
            launch_id,
            detail: Box::new(detail),
        },
        Err(e) => FetchResult::Error(e),
    }
}

// ---------------------------------------------------------------------------
// Fetch result handling
// ---------------------------------------------------------------------------

/// Handle the result of a completed fetch.
pub fn handle_fetch_result<C: Clock>(
    app: &mut App,
    client: &Ll2Client<C>,
    cache_manager: &CacheManager<C>,
    cache_config: &CacheConfig,
    result: FetchResult,
) {
    app.loading = false;

    match result {
        FetchResult::LaunchList {
            launches,
            total_count,
        } => {
            app.error_state = None;
            app.launches = launches.clone();
            app.total_count = total_count;

            // Reset selection if it's now out of bounds.
            if app.selected_index >= app.launches.len() && !app.launches.is_empty() {
                app.selected_index = app.launches.len() - 1;
            }

            // Persist to cache.
            let now = Utc::now();
            let expires_at = cache::list_expires_at(now, cache_config);
            let cache = LaunchListCache {
                version: CACHE_VERSION,
                fetched_at: now,
                expires_at,
                total_count,
                launches,
            };
            if let Err(e) = cache_manager.save_launch_list(&cache) {
                warn!(error = %e, "failed to save launch list cache");
            }

            // Update status bar metadata.
            app.cache_fetched_at = Some(now);
            app.cache_expires_at = Some(expires_at);
        }
        FetchResult::LaunchDetail { launch_id, detail } => {
            app.error_state = None;

            let now = Utc::now();
            let strategy = CacheStrategy::for_launch(detail.status.id, detail.net, now);
            let expires_at = cache::compute_expires_at(strategy, now, cache_config);

            let cached = LaunchDetailCache {
                version: CACHE_VERSION,
                launch_id: launch_id.clone(),
                fetched_at: now,
                expires_at,
                ttl_strategy: strategy.as_str().to_string(),
                data: *detail,
            };

            // Persist to disk.
            if let Err(e) = cache_manager.save_launch_detail(&cached) {
                warn!(error = %e, launch_id = %launch_id, "failed to save detail cache");
            }

            // Store in memory.
            app.detail_cache.insert(launch_id, cached);
        }
        FetchResult::ThrottleSync => {
            // Sync completed (success or graceful degradation) — no UI update
            // needed beyond the rate limit display, which is updated below.
            debug!("proactive throttle sync complete");
        }
        FetchResult::Error(err) => {
            // Always log the full error detail for debugging.
            warn!(error = %err, error_debug = ?err, "fetch failed");

            if let AppError::RequestCapExceeded(_) = &err {
                app.error_state = Some(ErrorState::RequestCapExceeded);
            } else if err.is_offline_signal() {
                app.error_state = Some(ErrorState::Offline);
            } else if let AppError::RateLimited(available_at) = err {
                app.error_state = Some(ErrorState::RateLimited { available_at });
            } else {
                // Generic UI message — the log has the full detail.
                let ui_message = match &err {
                    AppError::Network(e) if e.is_decode() => {
                        "Unexpected response from server".to_string()
                    }
                    AppError::ApiParse(_) => {
                        "Unexpected response from server".to_string()
                    }
                    AppError::ApiError { status, .. } => {
                        format!("Server returned an error ({status})")
                    }
                    _ => "Something went wrong".to_string(),
                };
                app.error_state = Some(ErrorState::Transient {
                    message: ui_message,
                    dismiss_at: Instant::now()
                        + std::time::Duration::from_secs(TRANSIENT_DISMISS_SECS),
                });
            }
        }
    }

    // Update rate limit display from current limiter state.
    let mut limiter = client.rate_limiter();
    app.rate_limit_remaining = Some(limiter.remaining() as u32);
    app.rate_limit_total = Some(limiter.limit() as u32);
}

/// Auto-dismiss expired transient errors.
pub fn dismiss_expired_errors(app: &mut App) {
    if let Some(ErrorState::Transient { dismiss_at, .. }) = &app.error_state {
        if Instant::now() >= *dismiss_at {
            app.error_state = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rate_limiter::RateLimiter;
    use crate::cache::CacheManager;
    use crate::clock::testing::FakeClock;
    use crate::config::CacheConfig;
    use crate::models::tests::dummy_launch_detail;
    use crate::models::{
        LaunchDetailCache, LaunchStatus, LaunchSummary, LocationInfo, NetPrecision, PadInfo,
        Provider,
    };
    use chrono::{Duration, TimeZone, Utc};

    fn base_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
    }

    fn make_client() -> Ll2Client<FakeClock> {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        Ll2Client::new("http://localhost:9999".to_string(), None, limiter).unwrap()
    }

    fn make_cache_manager() -> (CacheManager<FakeClock>, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("details")).unwrap();
        let clock = FakeClock::new(base_time());
        let cm = CacheManager::new(tmp.path().to_path_buf(), clock);
        (cm, tmp)
    }

    fn sample_launch(id: &str, name: &str) -> LaunchSummary {
        LaunchSummary {
            id: id.to_string(),
            name: name.to_string(),
            net: base_time(),
            net_precision: Some(NetPrecision {
                id: 1,
                name: "Day".to_string(),
                abbrev: "Day".to_string(),
            }),
            window_start: None,
            window_end: None,
            status: LaunchStatus {
                id: 1,
                name: "Go for Launch".to_string(),
                abbrev: "Go".to_string(),
            },
            launch_service_provider: Provider {
                name: "SpaceX".to_string(),
                provider_type: None,
            },
            pad: PadInfo {
                name: Some("LC-39A".to_string()),
                location: LocationInfo {
                    name: "KSC, Florida".to_string(),
                    timezone_name: Some("America/New_York".to_string()),
                    country: None,
                },
            },
            mission: None,
        }
    }

    // --- handle_fetch_result: launch list ---

    #[test]
    fn fetch_result_launch_list_updates_app_state() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));

        let launches = vec![
            sample_launch("id-1", "Launch 1"),
            sample_launch("id-2", "Launch 2"),
        ];

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchList {
                launches: launches.clone(),
                total_count: 42,
            },
        );

        assert_eq!(app.launches.len(), 2);
        assert_eq!(app.total_count, 42);
        assert!(!app.loading);
        assert!(!app.is_offline());
        assert!(app.error_state.is_none());
        assert!(app.cache_fetched_at.is_some());
        assert!(app.cache_expires_at.is_some());
        assert!(app.rate_limit_remaining.is_some());
        assert!(app.rate_limit_total.is_some());
    }

    #[test]
    fn fetch_result_launch_list_persists_to_cache() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));

        let launches = vec![sample_launch("id-1", "Launch 1")];

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchList {
                launches,
                total_count: 1,
            },
        );

        let loaded = cm.load_launch_list().unwrap();
        assert!(loaded.is_some());
        let cached = loaded.unwrap();
        assert_eq!(cached.launches.len(), 1);
        assert_eq!(cached.launches[0].name, "Launch 1");
    }

    #[test]
    fn fetch_result_resets_selection_when_out_of_bounds() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.selected_index = 10;

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchList {
                launches: vec![sample_launch("id-1", "Launch 1")],
                total_count: 1,
            },
        );

        assert_eq!(app.selected_index, 0);
    }

    // --- handle_fetch_result: launch detail ---

    #[test]
    fn fetch_result_detail_populates_in_memory_cache() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));

        let detail = dummy_launch_detail();
        let launch_id = detail.id.clone();

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchDetail {
                launch_id: launch_id.clone(),
                detail: Box::new(detail),
            },
        );

        assert!(app.detail_cache.contains_key(&launch_id));
        let cached = &app.detail_cache[&launch_id];
        assert_eq!(cached.data.name, "Starship IFT-7");
        assert!(!cached.ttl_strategy.is_empty());
    }

    #[test]
    fn fetch_result_detail_persists_to_disk() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));

        let detail = dummy_launch_detail();
        let launch_id = detail.id.clone();

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchDetail {
                launch_id: launch_id.clone(),
                detail: Box::new(detail),
            },
        );

        let loaded = cm.load_launch_detail(&launch_id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().data.name, "Starship IFT-7");
    }

    #[test]
    fn fetch_result_detail_clears_offline() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.error_state = Some(ErrorState::Offline);

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchDetail {
                launch_id: "some-id".into(),
                detail: Box::new(dummy_launch_detail()),
            },
        );

        assert!(!app.is_offline());
        assert!(app.error_state.is_none());
    }

    // --- handle_fetch_result: errors ---

    #[test]
    fn fetch_result_error_sets_transient_error_state() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.loading = true;

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::Error(AppError::ApiError {
                status: 500,
                message: "Server Error".into(),
            }),
        );

        assert!(!app.loading);
        match &app.error_state {
            Some(ErrorState::Transient { message, .. }) => {
                assert!(message.contains("500"));
            }
            other => panic!("expected Transient error, got: {other:?}"),
        }
    }

    #[test]
    fn fetch_result_rate_limited_sets_rate_limited_state() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.loading = true;

        let available_at = base_time();
        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::Error(AppError::RateLimited(available_at)),
        );

        match &app.error_state {
            Some(ErrorState::RateLimited {
                available_at: at, ..
            }) => {
                assert_eq!(*at, available_at);
            }
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }

    #[test]
    fn fetch_result_clears_offline_on_success() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.error_state = Some(ErrorState::Offline);

        handle_fetch_result(
            &mut app,
            &client,
            &cm,
            &config,
            FetchResult::LaunchList {
                launches: vec![],
                total_count: 0,
            },
        );

        assert!(!app.is_offline());
        assert!(app.error_state.is_none());
    }

    // --- check_needs_fetch ---

    #[test]
    fn needs_fetch_list_empty_launches() {
        let app = App::new((120, 40));
        let result = check_needs_fetch(&app);
        assert!(matches!(result, Some(FetchKind::LaunchList)));
    }

    #[test]
    fn needs_fetch_list_with_data_returns_none() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("id-1", "Launch 1")];
        let result = check_needs_fetch(&app);
        assert!(result.is_none());
    }

    #[test]
    fn needs_fetch_skipped_when_loading() {
        let mut app = App::new((120, 40));
        app.loading = true;
        assert!(check_needs_fetch(&app).is_none());
    }

    #[test]
    fn needs_fetch_skipped_when_error_present() {
        let mut app = App::new((120, 40));
        app.error_state = Some(ErrorState::Offline);
        assert!(check_needs_fetch(&app).is_none());
    }

    #[test]
    fn needs_fetch_detail_not_cached() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("some-uuid".into());
        let result = check_needs_fetch(&app);
        assert!(matches!(result, Some(FetchKind::LaunchDetail(id)) if id == "some-uuid"));
    }

    #[test]
    fn needs_fetch_detail_cached_fresh() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("some-uuid".into());
        app.detail_cache.insert(
            "some-uuid".into(),
            LaunchDetailCache {
                version: CACHE_VERSION,
                launch_id: "some-uuid".into(),
                fetched_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
                ttl_strategy: "short_term".into(),
                data: dummy_launch_detail(),
            },
        );
        assert!(check_needs_fetch(&app).is_none());
    }

    #[test]
    fn needs_fetch_detail_cached_stale() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("some-uuid".into());
        app.detail_cache.insert(
            "some-uuid".into(),
            LaunchDetailCache {
                version: CACHE_VERSION,
                launch_id: "some-uuid".into(),
                fetched_at: Utc::now() - Duration::hours(2),
                expires_at: Utc::now() - Duration::hours(1),
                ttl_strategy: "short_term".into(),
                data: dummy_launch_detail(),
            },
        );
        let result = check_needs_fetch(&app);
        assert!(matches!(result, Some(FetchKind::LaunchDetail(_))));
    }

    // --- check_refresh ---

    #[test]
    fn refresh_list_when_stale() {
        let mut app = App::new((120, 40));
        app.refresh_requested = true;
        app.launches = vec![sample_launch("id-1", "Launch 1")];
        app.cache_expires_at = Some(Utc::now() - Duration::minutes(5));
        let result = check_needs_fetch(&app);
        assert!(matches!(result, Some(FetchKind::LaunchList)));
    }

    #[test]
    fn refresh_list_blocked_when_fresh() {
        let mut app = App::new((120, 40));
        app.refresh_requested = true;
        app.launches = vec![sample_launch("id-1", "Launch 1")];
        app.cache_expires_at = Some(Utc::now() + Duration::hours(1));
        let result = check_needs_fetch(&app);
        assert!(result.is_none());
    }

    #[test]
    fn refresh_blocked_when_rate_limited() {
        let mut app = App::new((120, 40));
        app.refresh_requested = true;
        app.cache_expires_at = Some(Utc::now() - Duration::minutes(5));
        app.error_state = Some(ErrorState::RateLimited {
            available_at: base_time(),
        });
        // error_state.is_some() blocks check_needs_fetch entirely
        assert!(check_needs_fetch(&app).is_none());
    }

    #[test]
    fn refresh_detail_when_stale() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("some-uuid".into());
        app.refresh_requested = true;
        app.detail_cache.insert(
            "some-uuid".into(),
            LaunchDetailCache {
                version: CACHE_VERSION,
                launch_id: "some-uuid".into(),
                fetched_at: Utc::now() - Duration::hours(2),
                expires_at: Utc::now() - Duration::hours(1),
                ttl_strategy: "short_term".into(),
                data: dummy_launch_detail(),
            },
        );
        let result = check_needs_fetch(&app);
        assert!(matches!(result, Some(FetchKind::LaunchDetail(_))));
    }

    // --- active_screen ---

    #[test]
    fn active_screen_unwraps_help() {
        let screen = AppScreen::Help(Box::new(AppScreen::Detail("abc".into())));
        match active_screen(&screen) {
            AppScreen::Detail(id) => assert_eq!(id, "abc"),
            other => panic!("expected Detail, got: {other:?}"),
        }
    }

    #[test]
    fn active_screen_nested_help() {
        let screen = AppScreen::Help(Box::new(AppScreen::Help(Box::new(AppScreen::List))));
        assert!(matches!(active_screen(&screen), AppScreen::List));
    }
}
