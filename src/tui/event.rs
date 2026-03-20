//! Async event loop multiplexing terminal input and API responses.
//!
//! Uses `tokio::select!` to handle crossterm events alongside pending API
//! calls. Terminal resize events update the app's stored size. The loop
//! exits when `app.should_quit` is set.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use tracing::warn;

use crate::api::client::Ll2Client;
use crate::cache::{self, CacheManager};
use crate::clock::Clock;
use crate::config::CacheConfig;
use crate::error::{AppError, ErrorState};
use crate::models::{LaunchListCache, CACHE_VERSION};
use crate::tui::app::{App, AppScreen, MIN_COLS, MIN_ROWS};
use crate::tui::terminal::Tui;
use crate::tui::views::list::{self, compute_scroll_offset};

/// Duration after which a transient error is auto-dismissed (10 seconds).
const TRANSIENT_DISMISS_SECS: u64 = 10;

/// Run the main event loop until the user quits.
///
/// `pending_fetch` allows the caller to pass in an already-in-flight API
/// call (e.g. the initial launch list fetch started during startup).
/// `client` and `cache_manager` are available for spawning new fetches and
/// persisting results during the loop.
pub async fn run_event_loop<C: Clock + Send + Sync + 'static>(
    terminal: &mut Tui,
    app: &mut App,
    client: Arc<Ll2Client<C>>,
    cache_manager: &CacheManager<C>,
    cache_config: &CacheConfig,
    pending_fetch: Option<Pin<Box<dyn Future<Output = FetchResult> + Send>>>,
) -> Result<(), AppError> {
    let mut reader = EventStream::new();
    let mut pending: Option<Pin<Box<dyn Future<Output = FetchResult> + Send>>> = pending_fetch;

    loop {
        // Auto-dismiss transient errors.
        dismiss_expired_errors(app);

        // Render.
        render(terminal, app)?;

        // Multiplex input events and pending API calls.
        tokio::select! {
            biased;

            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => handle_key(app, key),
                    Some(Ok(Event::Resize(cols, rows))) => {
                        app.terminal_size = (cols, rows);
                    }
                    Some(Err(_)) | None => {
                        // Stream ended or error — treat as quit.
                        app.should_quit = true;
                    }
                    _ => {}
                }
            }

            result = async { pending.as_mut().unwrap().as_mut().await },
                if pending.is_some() && app.loading => {
                pending = None;
                handle_fetch_result(app, &client, cache_manager, cache_config, result);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
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
    LaunchDetail(Box<crate::models::LaunchDetail>),
    /// Fetch failed.
    Error(AppError),
}

/// Handle the result of a completed fetch.
fn handle_fetch_result<C: Clock>(
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
            app.is_offline = false;
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
        FetchResult::LaunchDetail(detail) => {
            app.is_offline = false;
            app.error_state = None;
            // Detail caching and screen transition will be handled in Step 5.2.
            let _ = detail;
        }
        FetchResult::Error(err) => {
            if err.is_offline_signal() {
                app.is_offline = true;
                app.error_state = Some(ErrorState::Offline);
            } else if let AppError::RateLimited(available_at) = err {
                app.error_state = Some(ErrorState::RateLimited { available_at });
            } else {
                app.error_state = Some(ErrorState::Transient {
                    message: err.to_string(),
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
fn dismiss_expired_errors(app: &mut App) {
    if let Some(ErrorState::Transient { dismiss_at, .. }) = &app.error_state {
        if Instant::now() >= *dismiss_at {
            app.error_state = None;
        }
    }
}

/// Process a key event.
fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl+C always quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    match &app.screen {
        AppScreen::Help(prev) => match key.code {
            KeyCode::Esc | KeyCode::Char('?') => {
                app.screen = *prev.clone();
            }
            KeyCode::Char('q') => app.should_quit = true,
            _ => {}
        },
        AppScreen::List => handle_list_key(app, key),
        AppScreen::Detail(_) => handle_detail_key(app, key),
        AppScreen::FilterPanel => handle_filter_key(app, key),
    }
}

/// Handle keys in the list view.
fn handle_list_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => {
            app.screen = AppScreen::Help(Box::new(app.screen.clone()));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
                update_list_scroll(app);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.launches.is_empty() && app.selected_index < app.launches.len() - 1 {
                app.selected_index += 1;
                update_list_scroll(app);
            }
        }
        KeyCode::Home => {
            app.selected_index = 0;
            update_list_scroll(app);
        }
        KeyCode::End => {
            if !app.launches.is_empty() {
                app.selected_index = app.launches.len() - 1;
                update_list_scroll(app);
            }
        }
        KeyCode::Enter => {
            // Transition to detail view (fetch wired in Step 5.2).
            if let Some(launch) = app.launches.get(app.selected_index) {
                let id = launch.id.clone();
                app.detail_scroll_offset = 0;
                app.screen = AppScreen::Detail(id);
            }
        }
        _ => {}
    }
}

/// Update the list scroll offset after a selection change.
fn update_list_scroll(app: &mut App) {
    // Estimate visible items from terminal height. The list area is roughly
    // terminal height minus outer block borders (2) minus hint bar (2).
    let list_height = (app.terminal_size.1 as usize).saturating_sub(4);
    let max_visible = list_height / 3; // each item is ~3 lines
    app.list_scroll_offset = compute_scroll_offset(
        app.selected_index,
        app.list_scroll_offset,
        max_visible,
        app.launches.len(),
    );
}

/// Handle keys in the detail view.
fn handle_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            app.screen = AppScreen::List;
        }
        KeyCode::Char('?') => {
            app.screen = AppScreen::Help(Box::new(app.screen.clone()));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll_offset += 1;
        }
        _ => {}
    }
}

/// Handle keys in the filter panel (placeholder for Step 6.1).
fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.screen = AppScreen::List;
        }
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the current app state to the terminal.
fn render(terminal: &mut Tui, app: &App) -> Result<(), AppError> {
    terminal
        .draw(|frame| {
            let area = frame.area();

            if app.is_terminal_too_small() {
                render_size_warning(frame, area);
                return;
            }

            match &app.screen {
                AppScreen::List => list::render_list(frame, area, app),
                AppScreen::Detail(id) => render_detail_placeholder(frame, area, id, app),
                AppScreen::Help(prev) => {
                    // Render the underlying screen first, then overlay help.
                    match prev.as_ref() {
                        AppScreen::List => list::render_list(frame, area, app),
                        AppScreen::Detail(id) => {
                            render_detail_placeholder(frame, area, id, app);
                        }
                        _ => {}
                    }
                    render_help_overlay(frame, area);
                }
                AppScreen::FilterPanel => list::render_list(frame, area, app),
            }

            // Render error state overlay if present.
            if let Some(error_state) = &app.error_state {
                render_error(frame, area, error_state);
            }
        })
        .map_err(AppError::Io)?;

    Ok(())
}

/// Render a warning when the terminal is too small.
fn render_size_warning(frame: &mut ratatui::Frame, area: Rect) {
    let text = Text::from(vec![
        Line::raw(""),
        Line::raw("  Terminal window too small."),
        Line::raw(""),
        Line::raw(format!(
            "  Please resize to at least {MIN_COLS}x{MIN_ROWS}"
        )),
        Line::raw("  to start seeing rockets!"),
        Line::raw(""),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Warning ");

    let paragraph = Paragraph::new(text).block(block);

    // Center the warning box.
    let popup = centered_rect(42, 8, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(paragraph, popup);
}

/// Placeholder detail view rendering (full implementation in Step 5.1).
fn render_detail_placeholder(
    frame: &mut ratatui::Frame,
    area: Rect,
    launch_id: &str,
    _app: &App,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Detail — {launch_id} "));

    let paragraph =
        Paragraph::new("Detail view placeholder. Press Esc to go back.").block(block);
    frame.render_widget(paragraph, area);
}

/// Render a help overlay (placeholder for Step 6.2).
fn render_help_overlay(frame: &mut ratatui::Frame, area: Rect) {
    let text = Text::from(vec![
        Line::raw(""),
        Line::raw("  Keyboard Shortcuts"),
        Line::raw("  ──────────────────"),
        Line::raw("  ↑/k      Move up"),
        Line::raw("  ↓/j      Move down"),
        Line::raw("  Enter    View details"),
        Line::raw("  Esc      Go back"),
        Line::raw("  ?        Toggle help"),
        Line::raw("  q        Quit"),
        Line::raw(""),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");

    let popup = centered_rect(30, 12, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(text).block(block), popup);
}

/// Render an error state indicator.
fn render_error(frame: &mut ratatui::Frame, area: Rect, error_state: &ErrorState) {
    let msg = match error_state {
        ErrorState::Transient { message, .. } => format!("Error: {message}"),
        ErrorState::RateLimited { available_at } => {
            format!(
                "Rate limited — resets at {}",
                available_at.format("%H:%M:%S UTC")
            )
        }
        ErrorState::Offline => "Network offline".to_string(),
    };

    let style = match error_state {
        ErrorState::Transient { .. } => Style::default().fg(Color::Yellow),
        ErrorState::RateLimited { .. } => Style::default().fg(Color::Red),
        ErrorState::Offline => Style::default().fg(Color::Red),
    };

    // Render at the bottom of the screen area.
    let error_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let paragraph = Paragraph::new(msg).style(style);
    frame.render_widget(paragraph, error_area);
}

/// Create a centered rectangle of the given size within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rate_limiter::RateLimiter;
    use crate::cache::CacheManager;
    use crate::clock::testing::FakeClock;
    use crate::config::CacheConfig;
    use crate::models::{
        LaunchStatus, LaunchSummary, LocationInfo, NetPrecision, PadInfo, Provider,
    };
    use chrono::{TimeZone, Utc};

    fn base_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
    }

    fn make_client() -> Ll2Client<FakeClock> {
        let clock = FakeClock::new(base_time());
        let limiter = RateLimiter::new(clock, false);
        Ll2Client::new(
            "http://localhost:9999".to_string(),
            None,
            limiter,
        )
        .unwrap()
    }

    fn make_cache_manager() -> (CacheManager<FakeClock>, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
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

    // --- handle_fetch_result tests ---

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
        assert!(!app.is_offline);
        assert!(app.error_state.is_none());
        assert!(app.cache_fetched_at.is_some());
        assert!(app.cache_expires_at.is_some());
        // Rate limit display should be updated.
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

        // Verify the cache was written to disk.
        let loaded = cm.load_launch_list().unwrap();
        assert!(loaded.is_some());
        let cached = loaded.unwrap();
        assert_eq!(cached.launches.len(), 1);
        assert_eq!(cached.total_count, 1);
        assert_eq!(cached.launches[0].name, "Launch 1");
    }

    #[test]
    fn fetch_result_resets_selection_when_out_of_bounds() {
        let client = make_client();
        let (cm, _tmp) = make_cache_manager();
        let config = CacheConfig::default();
        let mut app = App::new((120, 40));
        app.selected_index = 10; // Out of bounds for the result set.

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

        assert_eq!(app.selected_index, 0);
    }

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
        app.is_offline = true;
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

        assert!(!app.is_offline);
        assert!(app.error_state.is_none());
    }
}
