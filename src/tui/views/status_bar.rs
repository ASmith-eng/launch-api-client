//! Status bar renderer for the bottom of list and detail views.
//!
//! Shows cache staleness information on the left with four states:
//! fresh, stale, offline, and refreshing. The display respects the
//! `staleness_style` ("relative" / "absolute") and `time_format`
//! ("12h" / "24h") config options.

use chrono::{DateTime, Utc};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::tui::app::{App, AppScreen};
use crate::tui::time_fmt;

/// Resolve the cache timestamps relevant to the current screen.
///
/// On a detail screen, returns the detail entry's `fetched_at`/`expires_at`.
/// On the list screen (or any other), returns the list-level fields.
fn resolve_cache_times(app: &App) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    if let Some(id) = active_detail_id(&app.screen) {
        if let Some(cached) = app.detail_cache.get(id) {
            return (Some(cached.fetched_at), Some(cached.expires_at));
        }
    }
    (app.cache_fetched_at, app.cache_expires_at)
}

/// If the current screen (unwrapping any Help overlay) is a Detail, return its ID.
fn active_detail_id(screen: &AppScreen) -> Option<&str> {
    match screen {
        AppScreen::Detail(id) => Some(id.as_str()),
        AppScreen::Help(inner) => active_detail_id(inner),
        _ => None,
    }
}

/// Build the staleness status line for the status bar.
///
/// Returns a styled [`Line`] reflecting one of four states:
/// - **Refreshing**: `"Refreshing..."`
/// - **Offline**: `"Updated 2h ago (stale) • [OFFLINE]"`
/// - **Stale**: `"Updated 2h ago (stale) • Press 'r' to refresh"`
/// - **Fresh**: `"Updated 5m ago • Next refresh after 10:45 AM"`
///
/// On the detail screen the timestamps come from the detail cache entry;
/// on the list screen they come from the list-level fields.
pub fn build_status_line(app: &App) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);

    if app.loading {
        return Line::from(Span::styled("  Refreshing...", dim));
    }

    if app.is_offline() {
        return build_offline_line(app);
    }

    let (fetched_at, expires_at) = match resolve_cache_times(app) {
        (Some(f), Some(e)) => (f, e),
        _ => return Line::from(Span::styled("", dim)),
    };

    let now = Utc::now();
    let is_stale = now >= expires_at;

    if is_stale {
        build_stale_line(app, fetched_at)
    } else {
        build_fresh_line(app, fetched_at, expires_at)
    }
}

/// Build the status line for offline state.
fn build_offline_line(app: &App) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);

    let (fetched_at, _) = resolve_cache_times(app);
    let updated = match fetched_at {
        Some(fetched_at) => staleness_text(app, fetched_at),
        None => "No cached data".to_string(),
    };

    Line::from(vec![
        Span::styled(format!("  {updated} (stale) • "), dim),
        Span::styled("[OFFLINE]", Style::default().fg(Color::Red)),
    ])
}

/// Build the status line for stale cache.
fn build_stale_line(app: &App, fetched_at: chrono::DateTime<chrono::Utc>) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let updated = staleness_text(app, fetched_at);

    Line::from(Span::styled(
        format!("  {updated} (stale) • Press 'r' to refresh"),
        dim,
    ))
}

/// Build the status line for fresh cache.
fn build_fresh_line(
    app: &App,
    fetched_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let updated = staleness_text(app, fetched_at);
    let refresh_time = time_fmt::format_time_of_day(&expires_at, &app.ui_config.time_format);

    Line::from(Span::styled(
        format!("  {updated} • Next refresh after {refresh_time} UTC"),
        dim,
    ))
}

/// Format the "Updated ..." portion of the staleness text based on config.
///
/// - `"relative"` → `"Updated 5m ago"`
/// - `"absolute"` → `"Last updated 10:40 AM UTC"`
fn staleness_text(
    app: &App,
    fetched_at: chrono::DateTime<chrono::Utc>,
) -> String {
    if app.ui_config.staleness_style == "absolute" {
        let time_str = time_fmt::format_time_of_day(&fetched_at, &app.ui_config.time_format);
        format!("Last updated {time_str} UTC")
    } else {
        let relative = time_fmt::format_relative_time(fetched_at, Utc::now());
        format!("Updated {relative}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::models::{LaunchDetailCache, CACHE_VERSION};
    use crate::models::tests::dummy_launch_detail;
    use crate::tui::app::App;

    fn app_with_cache(fetched_ago_mins: i64, expires_in_mins: i64) -> App {
        let now = Utc::now();
        let mut app = App::new((120, 40));
        app.cache_fetched_at = Some(now - Duration::minutes(fetched_ago_mins));
        app.cache_expires_at = Some(now + Duration::minutes(expires_in_mins));
        app
    }

    /// Create an app on the Detail screen with a detail cache entry.
    /// List-level cache fields are left as `None`.
    fn app_on_detail(
        launch_id: &str,
        detail_fetched_ago_mins: i64,
        detail_expires_in_mins: i64,
    ) -> App {
        let now = Utc::now();
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail(launch_id.to_string());
        app.detail_cache.insert(
            launch_id.to_string(),
            LaunchDetailCache {
                version: CACHE_VERSION,
                launch_id: launch_id.to_string(),
                fetched_at: now - Duration::minutes(detail_fetched_ago_mins),
                expires_at: now + Duration::minutes(detail_expires_in_mins),
                ttl_strategy: "imminent".to_string(),
                data: dummy_launch_detail(),
            },
        );
        app
    }

    // ── Refreshing state ────────────────────────────────────────────────

    #[test]
    fn status_line_refreshing() {
        let mut app = App::new((120, 40));
        app.loading = true;
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Refreshing..."));
    }

    // ── Offline state ───────────────────────────────────────────────────

    #[test]
    fn status_line_offline_with_cache() {
        let mut app = app_with_cache(120, -10); // stale cache
        app.error_state = Some(crate::error::ErrorState::Offline);
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("(stale)"));
        assert!(text.contains("[OFFLINE]"));
    }

    #[test]
    fn status_line_offline_no_cache() {
        let mut app = App::new((120, 40));
        app.error_state = Some(crate::error::ErrorState::Offline);
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("No cached data"));
        assert!(text.contains("[OFFLINE]"));
    }

    // ── Stale state ─────────────────────────────────────────────────────

    #[test]
    fn status_line_stale_relative() {
        let app = app_with_cache(120, -10); // fetched 2h ago, expired 10m ago
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Updated 2h ago"));
        assert!(text.contains("(stale)"));
        assert!(text.contains("Press 'r' to refresh"));
    }

    #[test]
    fn status_line_stale_absolute() {
        let mut app = app_with_cache(120, -10);
        app.ui_config.staleness_style = "absolute".into();
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Last updated"));
        assert!(text.contains("UTC"));
        assert!(text.contains("(stale)"));
    }

    // ── Fresh state ─────────────────────────────────────────────────────

    #[test]
    fn status_line_fresh_relative() {
        let app = app_with_cache(5, 25); // fetched 5m ago, expires in 25m
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Updated 5m ago"));
        assert!(text.contains("Next refresh after"));
        assert!(!text.contains("(stale)"));
    }

    #[test]
    fn status_line_fresh_absolute() {
        let mut app = app_with_cache(5, 25);
        app.ui_config.staleness_style = "absolute".into();
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Last updated"));
        assert!(text.contains("Next refresh after"));
    }

    #[test]
    fn status_line_fresh_24h_format() {
        let mut app = app_with_cache(5, 25);
        app.ui_config.time_format = "24h".into();
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        // 24h format should NOT contain AM/PM
        assert!(!text.contains("AM") && !text.contains("PM"));
        assert!(text.contains("Next refresh after"));
    }

    #[test]
    fn status_line_fresh_12h_format() {
        let app = app_with_cache(5, 25); // default is 12h
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        // 12h format should contain AM or PM
        assert!(text.contains("AM") || text.contains("PM"));
    }

    // ── No cache metadata ───────────────────────────────────────────────

    #[test]
    fn status_line_no_cache_metadata() {
        let app = App::new((120, 40));
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        // Should be empty when no cache metadata is set
        assert!(text.trim().is_empty());
    }

    // ── Loading takes priority ──────────────────────────────────────────

    #[test]
    fn status_line_loading_overrides_stale() {
        let mut app = app_with_cache(120, -10);
        app.loading = true;
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Refreshing..."));
        assert!(!text.contains("(stale)"));
    }

    #[test]
    fn status_line_offline_overrides_stale() {
        let mut app = app_with_cache(120, -10);
        app.error_state = Some(crate::error::ErrorState::Offline);
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[OFFLINE]"));
        assert!(!text.contains("Press 'r' to refresh"));
    }

    // ── Detail screen uses detail cache timestamps ─────────────────────

    #[test]
    fn detail_screen_shows_detail_staleness_fresh() {
        let app = app_on_detail("abc-123", 5, 25); // fetched 5m ago, expires in 25m
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Next refresh after"));
        assert!(!text.contains("(stale)"));
    }

    #[test]
    fn detail_screen_shows_detail_staleness_stale() {
        let app = app_on_detail("abc-123", 120, -10); // fetched 2h ago, expired 10m ago
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("(stale)"));
        assert!(text.contains("Press 'r' to refresh"));
    }

    #[test]
    fn detail_screen_ignores_list_cache_timestamps() {
        // List cache is stale, but detail cache is fresh — should show fresh.
        let mut app = app_on_detail("abc-123", 5, 25);
        app.cache_fetched_at = Some(Utc::now() - Duration::hours(3));
        app.cache_expires_at = Some(Utc::now() - Duration::hours(1));
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Next refresh after"));
        assert!(!text.contains("(stale)"));
    }

    #[test]
    fn detail_screen_through_help_overlay() {
        let mut app = app_on_detail("abc-123", 120, -10); // stale detail
        app.screen = AppScreen::Help(Box::new(AppScreen::Detail("abc-123".into())));
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("(stale)"));
    }

    #[test]
    fn detail_screen_no_detail_cache_falls_back_to_list() {
        // On detail screen but detail not in cache — should fall back to list cache.
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("missing-id".into());
        app.cache_fetched_at = Some(Utc::now() - Duration::minutes(5));
        app.cache_expires_at = Some(Utc::now() + Duration::minutes(25));
        let line = build_status_line(&app);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Next refresh after"));
    }
}
