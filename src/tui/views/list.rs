//! List view renderer for the launch list screen.
//!
//! Renders a scrollable list of launches with status badges, provider/location
//! info, and time display. Each launch occupies two content lines plus one
//! blank separator line.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::models::LaunchSummary;
use crate::tui::app::App;
use crate::tui::style;
use crate::tui::time_fmt;
use crate::tui::views::status_bar;
use crate::tui::views::styled_block;
use crate::vendor::launch_library_2::status_map::{status_style, unknown_status_style};

/// Height of a single launch item in lines (name + provider + blank separator).
const ITEM_HEIGHT: usize = 3;

/// Render the full list view into the given area.
pub fn render_list(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    // Build the outer block with title and rate limit info.
    let title = build_title(app);
    let block = styled_block(&title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.loading && app.launches.is_empty() {
        let loading = Paragraph::new("  Fetching launches...").style(style::label());
        frame.render_widget(loading, inner);
        return;
    }

    if app.launches.is_empty() {
        let empty = Paragraph::new("  No launches to display.").style(style::label());
        frame.render_widget(empty, inner);
        return;
    }

    // Split inner area: launch list + bottom hint bar (2 lines).
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);

    let list_area = chunks[0];
    let hint_area = chunks[1];

    render_launch_items(frame, list_area, app);
    render_hint_bar(frame, hint_area, app);
}

/// Build the title string: "Launches — Showing N of M — Page X of Y" with optional rate limit.
fn build_title(app: &App) -> String {
    let filtered = if app.filter_state.has_active_filters() {
        " [Filtered]"
    } else {
        ""
    };
    let left = if app.loading {
        " Fetching launches... ".to_string()
    } else {
        let page_info = if app.total_pages() > 1 {
            format!(" — Page {} of {}", app.current_page + 1, app.total_pages())
        } else {
            String::new()
        };
        format!(
            " Launches{filtered} — Showing {} of {}{page_info} ",
            app.launches.len(),
            app.total_count,
        )
    };

    match (app.rate_limit_remaining, app.rate_limit_total) {
        (Some(remaining), Some(total)) => {
            format!("{left}── {remaining}/{total} reqs ")
        }
        _ => left,
    }
}

/// Render the scrollable launch items into the given area.
fn render_launch_items(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let visible_height = area.height as usize;
    let max_visible = visible_height / ITEM_HEIGHT;

    // Compute the scroll window.
    let scroll_offset = compute_scroll_offset(
        app.selected_index,
        app.list_scroll_offset,
        max_visible,
        app.launches.len(),
    );

    let end = (scroll_offset + max_visible).min(app.launches.len());
    let visible_launches = &app.launches[scroll_offset..end];

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(visible_height);

    for (i, launch) in visible_launches.iter().enumerate() {
        let abs_index = scroll_offset + i;
        let is_selected = abs_index == app.selected_index;

        // Line 1: marker + name (left) ... datetime + status badge (right).
        lines.push(build_name_line(launch, is_selected, area.width));

        // Line 2: provider | location.
        lines.push(build_provider_line(launch, is_selected));

        // Blank separator (except after the last visible item).
        if i + 1 < visible_launches.len() {
            lines.push(Line::raw(""));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Build the first line of a launch item: `▸ Name ... datetime [Status]`.
fn build_name_line(launch: &LaunchSummary, selected: bool, width: u16) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let name = launch.name.clone();

    // Format time based on net_precision.
    let time_str = time_fmt::format_net_time(
        &launch.net,
        launch.net_precision.as_ref(),
        &launch.pad.location.timezone_name,
    );

    // Status badge: [Go], [TBD], etc.
    let (badge_text, badge_style) = build_status_badge(&launch.status);

    // Calculate available space for name (pad to push time+badge to the right).
    let right_part = format!("{time_str} {badge_text}");
    let right_len = right_part.len();
    let available_name_width = (width as usize)
        .saturating_sub(right_len)
        .saturating_sub(marker.len())
        .saturating_sub(2); // some padding

    let truncated_name = truncate_str(&name, available_name_width);

    // Pad between name and right side.
    let pad_len = (width as usize)
        .saturating_sub(marker.len())
        .saturating_sub(truncated_name.len())
        .saturating_sub(right_len)
        .saturating_sub(1);
    let padding = " ".repeat(pad_len);

    let name_style = if selected {
        style::primary()
    } else {
        style::secondary()
    };

    Line::from(vec![
        Span::styled(marker.to_string(), name_style),
        Span::styled(truncated_name, name_style),
        Span::raw(padding),
        Span::styled(time_str, style::primary()),
        Span::raw(" "),
        Span::styled(badge_text, badge_style),
    ])
}

/// Build the second line: `  Provider | Location`.
fn build_provider_line(launch: &LaunchSummary, _selected: bool) -> Line<'static> {
    let provider = launch.launch_service_provider.name.clone();
    let location = launch.pad.location.name.clone();

    Line::from(vec![
        Span::styled("    ", style::secondary()),
        Span::styled(provider, style::secondary()),
        Span::styled(" | ", style::label()),
        Span::styled(location, style::secondary()),
    ])
}

/// Build the status badge text and style: `[Go]`, `[TBD]`, etc.
fn build_status_badge(status: &crate::models::LaunchStatus) -> (String, Style) {
    match status_style(status.id) {
        Some(ss) => (format!("[{}]", ss.abbrev), ss.style),
        None => (format!("[{}]", status.abbrev), unknown_status_style()),
    }
}

/// Truncate a string to fit within `max_width` characters, appending `…` if needed.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width > 1 {
        let mut result: String = s.chars().take(max_width - 1).collect();
        result.push('…');
        result
    } else {
        String::new()
    }
}

/// Render the bottom hint bar with keybinding shortcuts and status info.
fn render_hint_bar(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let status_line = status_bar::build_status_line(app);

    let key = style::secondary();
    let desc = style::label();

    let mut spans = vec![
        Span::styled("  ↑/↓", key),
        Span::styled(": Navigate · ", desc),
        Span::styled("Enter", key),
        Span::styled(": Details · ", desc),
    ];

    if app.total_pages() > 1 {
        spans.extend([
            Span::styled("n/p", key),
            Span::styled(": Page · ", desc),
        ]);
    }

    spans.extend([
        Span::styled("f", key),
        Span::styled(": Filters · ", desc),
        Span::styled("r", key),
        Span::styled(": Refresh · ", desc),
        Span::styled("?", key),
        Span::styled(": Help · ", desc),
        Span::styled("q", key),
        Span::styled(": Quit", desc),
    ]);

    let hints = Line::from(spans);

    let paragraph = Paragraph::new(vec![status_line, hints]);
    frame.render_widget(paragraph, area);
}

/// Compute the correct scroll offset to keep the selected item visible.
///
/// Returns the new offset. If the selected item is already visible within
/// `[offset, offset + max_visible)`, the offset is unchanged. Otherwise it
/// adjusts to bring the selected item into view.
pub fn compute_scroll_offset(
    selected: usize,
    current_offset: usize,
    max_visible: usize,
    total_items: usize,
) -> usize {
    if total_items == 0 || max_visible == 0 {
        return 0;
    }

    let mut offset = current_offset;

    // If selected is above the visible window, scroll up.
    if selected < offset {
        offset = selected;
    }

    // If selected is below the visible window, scroll down.
    if max_visible > 0 && selected >= offset + max_visible {
        offset = selected - max_visible + 1;
    }

    // Clamp so we don't scroll past the end.
    let max_offset = total_items.saturating_sub(max_visible);
    offset.min(max_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_empty_list() {
        assert_eq!(compute_scroll_offset(0, 0, 5, 0), 0);
    }

    #[test]
    fn scroll_offset_no_scroll_needed() {
        // 10 items, viewing 5, selected index 2, offset 0 → stays at 0.
        assert_eq!(compute_scroll_offset(2, 0, 5, 10), 0);
    }

    #[test]
    fn scroll_offset_selected_below_window() {
        // Selected at 7, window shows [0..5), should scroll to 3.
        assert_eq!(compute_scroll_offset(7, 0, 5, 10), 3);
    }

    #[test]
    fn scroll_offset_selected_above_window() {
        // Selected at 1, window shows [3..8), should scroll to 1.
        assert_eq!(compute_scroll_offset(1, 3, 5, 10), 1);
    }

    #[test]
    fn scroll_offset_clamped_at_end() {
        // Selected at 9 (last), 10 items, 5 visible → offset = 5.
        assert_eq!(compute_scroll_offset(9, 0, 5, 10), 5);
    }

    #[test]
    fn scroll_offset_all_items_visible() {
        // 3 items, 5 visible slots → offset always 0.
        assert_eq!(compute_scroll_offset(2, 0, 5, 3), 0);
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_fit() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_needs_ellipsis() {
        assert_eq!(truncate_str("hello world", 6), "hello…");
    }

    #[test]
    fn status_badge_known() {
        let status = crate::models::LaunchStatus {
            id: 1,
            name: "Go for Launch".into(),
            abbrev: "Go".into(),
        };
        let (text, style) = build_status_badge(&status);
        assert_eq!(text, "[Go]");
        assert_eq!(style.fg, Some(Color::Green));
    }

    #[test]
    fn status_badge_unknown() {
        let status = crate::models::LaunchStatus {
            id: 99,
            name: "Unknown".into(),
            abbrev: "UNK".into(),
        };
        let (text, style) = build_status_badge(&status);
        assert_eq!(text, "[UNK]");
        assert_eq!(style.fg, Some(Color::Gray));
    }

    // --- TestBackend rendering tests ---

    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;
    use crate::models::{
        LaunchStatus, LocationInfo, PadInfo, Provider, NetPrecision,
    };

    fn make_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        Terminal::new(backend).unwrap()
    }

    fn sample_launch(name: &str, status_id: u32, status_abbrev: &str) -> LaunchSummary {
        LaunchSummary {
            id: "test-id".into(),
            name: name.into(),
            net: chrono::DateTime::parse_from_rfc3339("2026-06-01T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            net_precision: Some(NetPrecision {
                id: 1,
                name: "Day".into(),
                abbrev: "Day".into(),
            }),
            window_start: None,
            window_end: None,
            status: LaunchStatus {
                id: status_id,
                name: "Status".into(),
                abbrev: status_abbrev.into(),
            },
            launch_service_provider: Provider {
                name: "SpaceX".into(),
                provider_type: None,
            },
            pad: PadInfo {
                name: None,
                location: LocationInfo {
                    name: "KSC, Florida".into(),
                    timezone_name: Some("America/New_York".into()),
                    country: None,
                },
            },
            mission: None,
        }
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn render_empty_list_shows_no_launches_message() {
        let mut terminal = make_test_terminal(100, 30);
        let app = App::new((100, 30));

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("No launches to display"),
            "empty list should show 'No launches' message, got:\n{text}"
        );
    }

    #[test]
    fn render_loading_empty_list_shows_fetching_message() {
        let mut terminal = make_test_terminal(100, 30);
        let mut app = App::new((100, 30));
        app.loading = true;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("Fetching launches"),
            "loading empty list should show 'Fetching launches' message, got:\n{text}"
        );
    }

    #[test]
    fn render_list_shows_status_badge_text() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Falcon 9 • Starlink", 1, "Go")];
        app.total_count = 1;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("[Go]"),
            "list should render status badge [Go], got:\n{text}"
        );
        assert!(
            text.contains("Falcon 9"),
            "list should render launch name, got:\n{text}"
        );
        assert!(
            text.contains("SpaceX"),
            "list should render provider name, got:\n{text}"
        );
    }

    #[test]
    fn render_list_shows_tbd_status_badge() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Ariane 6", 2, "TBD")];
        app.total_count = 1;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("[TBD]"),
            "list should render status badge [TBD], got:\n{text}"
        );
    }

    #[test]
    fn render_list_shows_staleness_indicator_when_stale() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Test Launch", 1, "Go")];
        app.total_count = 1;
        // Set cache metadata to expired time.
        app.cache_fetched_at = Some(
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        app.cache_expires_at = Some(
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("stale"),
            "hint bar should show 'stale' indicator when cache expired, got:\n{text}"
        );
    }

    #[test]
    fn render_list_title_shows_count() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![
            sample_launch("Launch 1", 1, "Go"),
            sample_launch("Launch 2", 2, "TBD"),
        ];
        app.total_count = 50;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("Showing 2 of 50"),
            "title should show 'Showing 2 of 50', got:\n{text}"
        );
    }

    #[test]
    fn render_list_with_unicode_launch_names() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        let mut launch = sample_launch("长征五号 CZ-5 • 嫦娥六号 🚀", 1, "Go");
        launch.pad.location.name = "文昌航天发射场, 海南".into();
        launch.launch_service_provider.name = "中国航天科技集团".into();
        app.launches = vec![launch];
        app.total_count = 1;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("长征五号") || text.contains("CZ-5"),
            "unicode launch name should render, got:\n{text}"
        );
    }

    #[test]
    fn render_list_title_shows_page_indicator() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Launch 1", 1, "Go")];
        app.total_count = 75;
        app.launches_per_page = 25;
        app.current_page = 1; // page 2 of 3

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("Page 2 of 3"),
            "title should show page indicator 'Page 2 of 3', got:\n{text}"
        );
    }

    #[test]
    fn render_list_title_hides_page_indicator_single_page() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Launch 1", 1, "Go")];
        app.total_count = 10;
        app.launches_per_page = 25;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            !text.contains("Page"),
            "title should not show page indicator for single page, got:\n{text}"
        );
    }

    #[test]
    fn render_list_hint_bar_shows_page_nav_when_multipage() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Launch 1", 1, "Go")];
        app.total_count = 50;
        app.launches_per_page = 25;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("n/p"),
            "hint bar should show 'n/p' for multi-page results, got:\n{text}"
        );
    }

    #[test]
    fn render_list_hint_bar_hides_page_nav_single_page() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Launch 1", 1, "Go")];
        app.total_count = 10;
        app.launches_per_page = 25;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            !text.contains("n/p"),
            "hint bar should not show 'n/p' for single page, got:\n{text}"
        );
    }

    #[test]
    fn render_list_title_shows_filtered_indicator() {
        let mut terminal = make_test_terminal(120, 30);
        let mut app = App::new((120, 30));
        app.launches = vec![sample_launch("Launch 1", 1, "Go")];
        app.total_count = 1;
        app.filter_state.status = crate::tui::filter::StatusFilter::GoForLaunch;

        terminal
            .draw(|frame| {
                render_list(frame, frame.area(), &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("[Filtered]"),
            "title should show [Filtered] when filters active, got:\n{text}"
        );
    }
}
