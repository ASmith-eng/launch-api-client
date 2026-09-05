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
        // Any key skips, and is consumed rather than falling through to List.
        AppScreen::Splash { .. } => app.screen = AppScreen::List,
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
        KeyCode::Char('n') => {
            // Next page — only if not on the last page and not loading.
            if !app.loading && app.current_page + 1 < app.total_pages() {
                navigate_page(app, app.current_page + 1);
            }
        }
        KeyCode::Char('p') => {
            // Previous page — only if not on the first page and not loading.
            if !app.loading && app.current_page > 0 {
                navigate_page(app, app.current_page - 1);
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

/// Switch to `target_page`, stashing the current page's data in the
/// in-memory cache and restoring the target page from it if available.
///
/// If the target page is not cached, `launches` is cleared so that
/// `check_needs_fetch` triggers an API request on the next event loop tick.
fn navigate_page(app: &mut App, target_page: u32) {
    // Stash the current page before leaving it (skip if empty — nothing to save).
    if !app.launches.is_empty() {
        app.page_cache
            .insert(app.current_page, app.launches.clone());
    }

    app.current_page = target_page;
    app.selected_index = 0;
    app.list_scroll_offset = 0;

    if let Some(cached) = app.page_cache.get(&target_page) {
        app.launches = cached.clone();
    } else {
        app.launches.clear();
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
            // Clamp to the maximum offset cached by the last render. Without the
            // clamp the offset winds up past the end, and the user then has to
            // press Up an equal number of times before scrolling visibly moves.
            let max_scroll = app.detail_max_scroll.get();
            app.detail_scroll_offset = (app.detail_scroll_offset + 1).min(max_scroll);
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
                app.page_cache.clear();
                app.launches.clear();
                app.selected_index = 0;
                app.list_scroll_offset = 0;
                app.current_page = 0;
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
        KeyCode::Char('x') => {
            if let Some(ref mut f) = app.editing_filter {
                f.clear_all();
            }
        }
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn sample_launch(id: &str) -> crate::models::LaunchSummary {
        use crate::models::*;
        use chrono::Utc;
        LaunchSummary {
            id: id.to_string(),
            name: "Test".into(),
            net: Utc::now(),
            net_precision: None,
            window_start: None,
            window_end: None,
            status: LaunchStatus {
                id: 1,
                name: "Go".into(),
                abbrev: "Go".into(),
            },
            launch_service_provider: Provider {
                name: "SpaceX".into(),
                provider_type: None,
            },
            pad: PadInfo {
                name: None,
                location: LocationInfo {
                    name: "KSC".into(),
                    timezone_name: None,
                    country: None,
                },
            },
            mission: None,
        }
    }

    // --- Pagination key tests ---

    #[test]
    fn next_page_advances_and_clears_launches_when_uncached() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 50; // 2 pages at 25 per page
        app.selected_index = 5;
        app.current_page = 0;

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert_eq!(app.current_page, 1);
        assert!(app.launches.is_empty()); // page 1 not cached yet — triggers fetch
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.list_scroll_offset, 0);
    }

    #[test]
    fn next_page_serves_from_cache_when_available() {
        let mut app = App::new((120, 40));
        app.total_count = 50;
        app.current_page = 0;
        app.launches = vec![sample_launch("page0")];
        app.page_cache
            .insert(1, vec![sample_launch("page1-cached")]);

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert_eq!(app.current_page, 1);
        assert_eq!(app.launches.len(), 1);
        assert_eq!(app.launches[0].id, "page1-cached");
    }

    #[test]
    fn next_page_stashes_current_page_into_cache() {
        let mut app = App::new((120, 40));
        app.total_count = 50;
        app.current_page = 0;
        app.launches = vec![sample_launch("page0")];

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert!(app.page_cache.contains_key(&0));
        assert_eq!(app.page_cache[&0][0].id, "page0");
    }

    #[test]
    fn next_page_noop_on_last_page() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 50;
        app.current_page = 1; // last page (50/25 = 2 pages, 0-indexed: 0 and 1)

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert_eq!(app.current_page, 1); // unchanged
        assert!(!app.launches.is_empty()); // not cleared
    }

    #[test]
    fn prev_page_goes_back_and_clears_launches_when_uncached() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 75;
        app.current_page = 2;
        app.selected_index = 3;

        handle_key(&mut app, press(KeyCode::Char('p')));

        assert_eq!(app.current_page, 1);
        assert!(app.launches.is_empty()); // page 1 not cached yet — triggers fetch
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn prev_page_serves_from_cache_when_available() {
        let mut app = App::new((120, 40));
        app.total_count = 75;
        app.current_page = 2;
        app.launches = vec![sample_launch("page2")];
        app.page_cache
            .insert(1, vec![sample_launch("page1-cached")]);

        handle_key(&mut app, press(KeyCode::Char('p')));

        assert_eq!(app.current_page, 1);
        assert_eq!(app.launches.len(), 1);
        assert_eq!(app.launches[0].id, "page1-cached");
    }

    #[test]
    fn prev_page_noop_on_first_page() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 50;
        app.current_page = 0;

        handle_key(&mut app, press(KeyCode::Char('p')));

        assert_eq!(app.current_page, 0);
        assert!(!app.launches.is_empty());
    }

    #[test]
    fn next_page_noop_when_loading() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 50;
        app.current_page = 0;
        app.loading = true;

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert_eq!(app.current_page, 0);
    }

    #[test]
    fn prev_page_noop_when_loading() {
        let mut app = App::new((120, 40));
        app.total_count = 50;
        app.current_page = 1;
        app.loading = true;

        handle_key(&mut app, press(KeyCode::Char('p')));

        assert_eq!(app.current_page, 1);
    }

    #[test]
    fn next_page_noop_single_page() {
        let mut app = App::new((120, 40));
        app.launches = vec![sample_launch("1")];
        app.total_count = 10; // 10 < 25, only 1 page
        app.current_page = 0;

        handle_key(&mut app, press(KeyCode::Char('n')));

        assert_eq!(app.current_page, 0);
    }

    #[test]
    fn filter_apply_resets_page() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::FilterPanel;
        app.editing_filter = Some(crate::tui::filter::FilterState::default());
        app.current_page = 3;

        handle_key(&mut app, press(KeyCode::Enter));

        assert_eq!(app.current_page, 0);
    }

    #[test]
    fn filter_clear_all_resets_editing_filter() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::FilterPanel;
        let mut f = crate::tui::filter::FilterState::default();
        f.status = crate::tui::filter::StatusFilter::GoForLaunch;
        f.region = crate::tui::filter::RegionFilter::Europe;
        f.is_crewed = crate::tui::filter::CrewedFilter::CrewedOnly;
        f.date_range = crate::tui::filter::DateRangeFilter::Next30Days;
        app.editing_filter = Some(f);

        handle_key(&mut app, press(KeyCode::Char('x')));

        let ef = app.editing_filter.as_ref().unwrap();
        assert!(!ef.has_active_filters());
        // Still in filter panel — not applied yet.
        assert!(matches!(app.screen, AppScreen::FilterPanel));
    }

    #[test]
    fn filter_apply_clears_page_cache() {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::FilterPanel;
        app.editing_filter = Some(crate::tui::filter::FilterState::default());
        app.page_cache.insert(0, vec![sample_launch("a")]);
        app.page_cache.insert(1, vec![sample_launch("b")]);

        handle_key(&mut app, press(KeyCode::Enter));

        assert!(app.page_cache.is_empty());
    }

    // --- Detail scroll key tests ---

    fn detail_app(max_scroll: usize) -> App {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Detail("some-id".into());
        app.detail_max_scroll.set(max_scroll);
        app
    }

    #[test]
    fn detail_scroll_down_stops_at_max() {
        let mut app = detail_app(3);

        // Press Down well past the bottom.
        for _ in 0..10 {
            handle_key(&mut app, press(KeyCode::Down));
        }

        assert_eq!(app.detail_scroll_offset, 3, "offset must saturate at max, not wind up");
    }

    #[test]
    fn detail_scroll_up_moves_immediately_after_hitting_bottom() {
        // The regression: over-pressing Down used to require an equal number of
        // Up presses before scrolling visibly reversed.
        let mut app = detail_app(3);
        for _ in 0..10 {
            handle_key(&mut app, press(KeyCode::Down));
        }
        assert_eq!(app.detail_scroll_offset, 3);

        // A single Up must move straight away.
        handle_key(&mut app, press(KeyCode::Up));
        assert_eq!(app.detail_scroll_offset, 2);
    }

    #[test]
    fn detail_scroll_up_saturates_at_top() {
        let mut app = detail_app(3);
        app.detail_scroll_offset = 1;

        handle_key(&mut app, press(KeyCode::Up));
        handle_key(&mut app, press(KeyCode::Up));
        handle_key(&mut app, press(KeyCode::Up));

        assert_eq!(app.detail_scroll_offset, 0);
    }

    #[test]
    fn detail_scroll_down_does_nothing_when_content_fits() {
        // max_scroll of 0 means everything fits; Down should stay put.
        let mut app = detail_app(0);

        handle_key(&mut app, press(KeyCode::Down));
        handle_key(&mut app, press(KeyCode::Down));

        assert_eq!(app.detail_scroll_offset, 0);
    }

    // --- Splash tests ---

    fn splash_app() -> App {
        let mut app = App::new((120, 40));
        app.screen = AppScreen::Splash {
            started_at: std::time::Instant::now(),
        };
        app
    }

    #[test]
    fn any_key_skips_the_splash() {
        for code in [
            KeyCode::Char('j'),
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Char(' '),
        ] {
            let mut app = splash_app();
            handle_key(&mut app, press(code));
            assert!(matches!(app.screen, AppScreen::List), "{code:?} did not dismiss the splash");
        }
    }

    #[test]
    fn skip_key_is_consumed_rather_than_run_as_a_list_command() {
        let mut app = splash_app();
        app.launches = vec![sample_launch("1"), sample_launch("2")];

        // 'j' would move the list selection; from the splash it must only skip.
        handle_key(&mut app, press(KeyCode::Char('j')));

        assert!(matches!(app.screen, AppScreen::List));
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn quit_key_skips_the_splash_without_quitting() {
        let mut app = splash_app();

        handle_key(&mut app, press(KeyCode::Char('q')));

        assert!(matches!(app.screen, AppScreen::List));
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_c_still_quits_from_the_splash() {
        let mut app = splash_app();

        handle_key(
            &mut app,
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
        );

        assert!(app.should_quit);
    }
}
