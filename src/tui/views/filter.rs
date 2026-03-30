//! Filter panel overlay renderer.
//!
//! Renders an inline filter bar as a small overlay near the top of the list
//! view. The active category is highlighted with arrow indicators to show
//! left/right cycling is available.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::tui::filter::FilterState;

/// Height of the filter panel overlay (top border + categories + hints + bottom border).
const PANEL_HEIGHT: u16 = 4;

/// Render the filter panel overlay near the top of the given area.
pub fn render_filter_panel(frame: &mut ratatui::Frame, area: Rect, filter: &FilterState) {
    if area.height < PANEL_HEIGHT + 2 || area.width < 40 {
        return;
    }

    let width = area.width.saturating_sub(4).max(40);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + 2; // just below the list title bar

    let panel_area = Rect {
        x,
        y,
        width,
        height: PANEL_HEIGHT,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Filters ");

    let inner = block.inner(panel_area);
    frame.render_widget(Clear, panel_area);
    frame.render_widget(block, panel_area);

    // Line 1: filter categories with values.
    let categories_line = build_categories_line(filter);
    frame.render_widget(
        Paragraph::new(categories_line),
        Rect {
            height: 1,
            ..inner
        },
    );

    // Line 2: key hints.
    let hints_line = build_hints_line();
    frame.render_widget(
        Paragraph::new(hints_line),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );
}

/// Build the categories line showing all four filters with their current values.
fn build_categories_line(filter: &FilterState) -> Line<'static> {
    let categories = filter.category_labels();

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(16);
    spans.push(Span::raw(" "));

    for (i, (name, value)) in categories.iter().enumerate() {
        let is_active = i == filter.active_category;

        if is_active {
            spans.push(Span::styled(
                format!("\u{25C0} {name}: {value} \u{25B6}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!("{name}: "),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                value.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        if i < categories.len() - 1 {
            spans.push(Span::styled("   ", Style::default()));
        }
    }

    Line::from(spans)
}

/// Build the hints line for the filter panel.
fn build_hints_line() -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab", Style::default().fg(Color::White)),
        Span::styled(": Next \u{00B7} ", Style::default().fg(Color::DarkGray)),
        Span::styled("\u{2190}/\u{2192}", Style::default().fg(Color::White)),
        Span::styled(": Change \u{00B7} ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::White)),
        Span::styled(": Apply \u{00B7} ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::White)),
        Span::styled(": Cancel", Style::default().fg(Color::DarkGray)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::filter::{
        CrewedFilter, DateRangeFilter, RegionFilter, StatusFilter,
    };

    #[test]
    fn categories_line_highlights_active() {
        let filter = FilterState {
            active_category: 0,
            ..Default::default()
        };
        let line = build_categories_line(&filter);
        // The first span (after leading space) should contain the active marker.
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("\u{25C0} Status: All \u{25B6}"));
    }

    #[test]
    fn categories_line_shows_non_default_values() {
        let filter = FilterState {
            active_category: 1,
            status: StatusFilter::GoForLaunch,
            region: RegionFilter::US,
            is_crewed: CrewedFilter::CrewedOnly,
            date_range: DateRangeFilter::Next30Days,
        };
        let line = build_categories_line(&filter);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("Go for Launch"));
        assert!(text.contains("US"));
        assert!(text.contains("Crewed"));
        assert!(text.contains("30 days"));
    }

    #[test]
    fn categories_line_cycles_active_category() {
        for cat in 0..4 {
            let filter = FilterState {
                active_category: cat,
                ..Default::default()
            };
            let line = build_categories_line(&filter);
            let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
            // The active category should have arrow markers.
            assert!(text.contains('\u{25C0}'), "cat {cat} missing left arrow");
            assert!(text.contains('\u{25B6}'), "cat {cat} missing right arrow");
        }
    }

    #[test]
    fn hints_line_contains_all_keys() {
        let line = build_hints_line();
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("Tab"));
        assert!(text.contains("Enter"));
        assert!(text.contains("Esc"));
    }
}
