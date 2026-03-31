//! Key event handlers for the TUI.
//!
//! All handlers are pure functions on `&mut App` — they require no async
//! or external resource access. The event loop calls [`handle_key`] once
//! per key event.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppScreen};
use crate::tui::views::list::compute_scroll_offset;

/// Process a key event.
pub fn handle_key(app: &mut App, key: KeyEvent) {
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
        KeyCode::Char('f') => {
            app.editing_filter = Some(app.filter_state.clone());
            app.screen = AppScreen::FilterPanel;
        }
        KeyCode::Char('r') => {
            if !app.loading {
                app.refresh_requested = true;
            }
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
        KeyCode::Char('r') => {
            if !app.loading {
                app.refresh_requested = true;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // Upper bound is enforced at render time in render_detail() via
            // `app.detail_scroll_offset.min(max_scroll)`, so over-incrementing
            // here is visually harmless.
            app.detail_scroll_offset += 1;
        }
        _ => {}
    }
}

/// Handle keys in the filter panel.
fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            // Cancel: discard edits, return to list.
            app.editing_filter = None;
            app.screen = AppScreen::List;
        }
        KeyCode::Enter => {
            // Apply: promote editing filter to active, clear list to trigger re-fetch.
            if let Some(edited) = app.editing_filter.take() {
                app.filter_state = edited;
                app.launches.clear();
                app.selected_index = 0;
                app.list_scroll_offset = 0;
                app.total_count = 0;
                app.cache_fetched_at = None;
                app.cache_expires_at = None;
            }
            app.screen = AppScreen::List;
        }
        KeyCode::Tab => {
            if let Some(ref mut f) = app.editing_filter {
                f.next_category();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut f) = app.editing_filter {
                f.cycle_prev();
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut f) = app.editing_filter {
                f.cycle_next();
            }
        }
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}
