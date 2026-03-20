//! Application state machine and core types for the TUI.
//!
//! The [`App`] struct holds all runtime state. [`AppScreen`] encodes which view
//! is active, and [`FilterState`] tracks the inline filter panel.

use std::collections::HashMap;

use crate::models::{LaunchDetailCache, LaunchSummary};
use crate::error::ErrorState;

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

/// Filter state for the inline filter panel.
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Which filter category is currently focused.
    pub active_category: usize,
    /// Selected status IDs to filter by.
    pub status: Option<Vec<u32>>,
    /// Selected region (`pad__location` value).
    pub region: Option<String>,
    /// Whether to filter for crewed missions.
    pub is_crewed: Option<bool>,
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
    /// In-memory detail cache (loaded on demand).
    pub detail_cache: HashMap<String, LaunchDetailCache>,
    /// Whether an API request is currently in-flight.
    pub loading: bool,
    /// Current error state shown to the user (if any).
    pub error_state: Option<ErrorState>,
    /// Whether the app believes the network is unreachable.
    pub is_offline: bool,
    /// Filter panel state.
    pub filter_state: FilterState,
    /// Current terminal dimensions (columns, rows).
    pub terminal_size: (u16, u16),
    /// Total count of launches from the API (may differ from `launches.len()`).
    pub total_count: u32,
    /// Remaining API requests in the current rate limit window.
    pub rate_limit_remaining: Option<u32>,
    /// Total API request limit for the current window.
    pub rate_limit_total: Option<u32>,
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
            detail_cache: HashMap::new(),
            loading: false,
            error_state: None,
            is_offline: false,
            filter_state: FilterState::default(),
            terminal_size,
            total_count: 0,
            rate_limit_remaining: None,
            rate_limit_total: None,
            should_quit: false,
        }
    }

    /// Whether the terminal is too small to render the full UI.
    pub fn is_terminal_too_small(&self) -> bool {
        self.terminal_size.0 < MIN_COLS || self.terminal_size.1 < MIN_ROWS
    }
}
