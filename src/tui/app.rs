//! Application state machine and core types for the TUI.
//!
//! The [`App`] struct holds all runtime state. [`AppScreen`] encodes which view
//! is active. Filter types live in [`super::filter`].

use std::cell::Cell;
use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::config::UiConfig;
use crate::error::ErrorState;
use crate::models::{LaunchDetailCache, LaunchSummary};
use crate::tui::filter::FilterState;

/// Minimum terminal width required for rendering.
pub const MIN_COLS: u16 = 80;
/// Minimum terminal height required for rendering.
pub const MIN_ROWS: u16 = 24;

/// Which screen the TUI is currently displaying.
#[derive(Debug, Clone)]
pub enum AppScreen {
    /// Main launch list view.
    List,
    /// Detail view for a specific launch (by UUID).
    Detail(String),
    /// Context-aware help overlay, wrapping the screen that was active.
    Help(Box<AppScreen>),
    /// Inline filter panel.
    FilterPanel,
}

/// All runtime state for the TUI application.
#[derive(Debug)]
pub struct App {
    /// Current screen.
    pub screen: AppScreen,
    /// Loaded launch summaries for the list view.
    pub launches: Vec<LaunchSummary>,
    /// Index of the currently selected launch in the list.
    pub selected_index: usize,
    /// Scroll offset for the list view.
    pub list_scroll_offset: usize,
    /// Scroll offset for the detail view.
    pub detail_scroll_offset: usize,
    /// Maximum valid scroll offset for the detail view, cached from the most
    /// recent render. `render_detail` is the only place that knows the content
    /// height (and thus the true maximum), but it borrows `&App`, so the value
    /// is stashed here through a `Cell` for the key handler to clamp against.
    /// Without this, holding Down at the bottom would "wind up" the offset far
    /// past the end, forcing an equal number of Up presses to unwind.
    pub detail_max_scroll: Cell<usize>,
    /// In-memory detail cache (loaded on demand).
    pub detail_cache: HashMap<String, LaunchDetailCache>,
    /// In-memory page cache for the list view.
    ///
    /// Keyed by zero-based page index. Populated when a page is fetched and
    /// when navigating away from a page. Cleared when filters change.
    pub page_cache: HashMap<u32, Vec<LaunchSummary>>,
    /// Whether an API request is currently in-flight.
    pub loading: bool,
    /// Current error state shown to the user (if any).
    pub error_state: Option<ErrorState>,
    /// Applied filter state (used for fetch dispatch).
    pub filter_state: FilterState,
    /// In-progress filter edits (populated only while FilterPanel is open).
    pub editing_filter: Option<FilterState>,
    /// Current terminal dimensions (columns, rows).
    pub terminal_size: (u16, u16),
    /// Total count of launches from the API (may differ from `launches.len()`).
    pub total_count: u32,
    /// Remaining API requests in the current rate limit window.
    pub rate_limit_remaining: Option<u32>,
    /// Total API request limit for the current window.
    pub rate_limit_total: Option<u32>,
    /// When the current launch list cache was fetched.
    pub cache_fetched_at: Option<DateTime<Utc>>,
    /// When the current launch list cache expires.
    pub cache_expires_at: Option<DateTime<Utc>>,
    /// UI display configuration (time format, staleness style).
    pub ui_config: UiConfig,
    /// Number of launches per page for API requests.
    pub launches_per_page: u32,
    /// Current page index (zero-based). Displayed as `current_page + 1`.
    pub current_page: u32,
    /// Whether the user has requested a manual refresh (`r` key).
    pub refresh_requested: bool,
    /// Whether the app should exit on the next loop iteration.
    pub should_quit: bool,
}

impl App {
    /// Create a new `App` with default state and the given terminal size.
    pub fn new(terminal_size: (u16, u16)) -> Self {
        Self {
            screen: AppScreen::List,
            launches: Vec::new(),
            selected_index: 0,
            list_scroll_offset: 0,
            detail_scroll_offset: 0,
            detail_max_scroll: Cell::new(0),
            detail_cache: HashMap::new(),
            page_cache: HashMap::new(),
            loading: false,
            error_state: None,
            filter_state: FilterState::default(),
            editing_filter: None,
            terminal_size,
            total_count: 0,
            rate_limit_remaining: None,
            rate_limit_total: None,
            cache_fetched_at: None,
            cache_expires_at: None,
            ui_config: UiConfig::default(),
            launches_per_page: 25,
            current_page: 0,
            refresh_requested: false,
            should_quit: false,
        }
    }

    /// Whether the terminal is too small to render the full UI.
    pub fn is_terminal_too_small(&self) -> bool {
        self.terminal_size.0 < MIN_COLS || self.terminal_size.1 < MIN_ROWS
    }

    /// Whether the app believes the network is unreachable.
    ///
    /// Derived from `error_state` — there is no separate boolean flag.
    pub fn is_offline(&self) -> bool {
        matches!(self.error_state, Some(ErrorState::Offline))
    }

    /// Total number of pages, derived from `total_count` and `launches_per_page`.
    ///
    /// Returns at least 1 so page display is never "Page 0 of 0".
    pub fn total_pages(&self) -> u32 {
        if self.total_count == 0 || self.launches_per_page == 0 {
            return 1;
        }
        self.total_count.div_ceil(self.launches_per_page)
    }

    /// API offset for the current page.
    pub fn page_offset(&self) -> u32 {
        self.current_page * self.launches_per_page
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_pages_zero_count() {
        let app = App::new((120, 40));
        assert_eq!(app.total_pages(), 1);
    }

    #[test]
    fn total_pages_exact_multiple() {
        let mut app = App::new((120, 40));
        app.total_count = 50;
        app.launches_per_page = 25;
        assert_eq!(app.total_pages(), 2);
    }

    #[test]
    fn total_pages_with_remainder() {
        let mut app = App::new((120, 40));
        app.total_count = 51;
        app.launches_per_page = 25;
        assert_eq!(app.total_pages(), 3);
    }

    #[test]
    fn total_pages_fewer_than_page_size() {
        let mut app = App::new((120, 40));
        app.total_count = 10;
        app.launches_per_page = 25;
        assert_eq!(app.total_pages(), 1);
    }

    #[test]
    fn total_pages_single_item() {
        let mut app = App::new((120, 40));
        app.total_count = 1;
        app.launches_per_page = 25;
        assert_eq!(app.total_pages(), 1);
    }

    #[test]
    fn page_offset_first_page() {
        let mut app = App::new((120, 40));
        app.current_page = 0;
        app.launches_per_page = 25;
        assert_eq!(app.page_offset(), 0);
    }

    #[test]
    fn page_offset_second_page() {
        let mut app = App::new((120, 40));
        app.current_page = 1;
        app.launches_per_page = 25;
        assert_eq!(app.page_offset(), 25);
    }

    #[test]
    fn page_offset_third_page() {
        let mut app = App::new((120, 40));
        app.current_page = 2;
        app.launches_per_page = 10;
        assert_eq!(app.page_offset(), 20);
    }
}
