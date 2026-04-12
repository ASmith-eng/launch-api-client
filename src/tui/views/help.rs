//! Context-aware help overlay renderer.
//!
//! Renders a centered popup showing keyboard shortcuts relevant to the
//! current underlying view (list or detail). Dismissible with `?` or `Esc`.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::tui::app::AppScreen;

/// Which help content to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpContext {
    List,
    Detail,
}

impl HelpContext {
    /// Determine the help context from the screen that was active when help
    /// was opened.
    pub fn from_screen(screen: &AppScreen) -> Self {
        match screen {
            AppScreen::Detail(_) => Self::Detail,
            // List, FilterPanel, or nested Help all fall back to list help.
            _ => Self::List,
        }
    }
}

/// Render the help overlay centered within `area`.
pub fn render_help_overlay(frame: &mut ratatui::Frame, area: Rect, context: HelpContext) {
    let lines = build_help_lines(context);
    let height = lines.len() as u16 + 2; // +2 for top/bottom border
    let width = 39; // matches design doc box width

    let popup = centered_rect(width, height, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");

    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// Build the help text lines for the given context.
fn build_help_lines(context: HelpContext) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let key_style = Style::default().fg(Color::White);
    let heading = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let tip_style = Style::default().fg(Color::DarkGray);

    let mut lines = Vec::with_capacity(20);
    lines.push(Line::raw(""));

    match context {
        HelpContext::List => {
            lines.push(Line::from(Span::styled("  Navigation", heading)));
            lines.push(key_line("  \u{2191}/\u{2193}       ", "Navigate launch list", key_style, dim));
            lines.push(key_line("  Enter     ", "View launch details", key_style, dim));
            lines.push(key_line("  Home/End  ", "Jump to first/last", key_style, dim));
            lines.push(key_line("  n/p       ", "Next/previous page", key_style, dim));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  Actions", heading)));
            lines.push(key_line("  r         ", "Refresh (when stale)", key_style, dim));
            lines.push(key_line("  f         ", "Open filter panel", key_style, dim));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  General", heading)));
            lines.push(key_line("  ?         ", "Toggle this help", key_style, dim));
            lines.push(key_line("  q         ", "Quit application", key_style, dim));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("  Tip: ", tip_style),
                Span::styled(
                    "Create config.toml in your",
                    tip_style,
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "  config directory to customize.",
                tip_style,
            )));
        }
        HelpContext::Detail => {
            lines.push(Line::from(Span::styled("  Navigation", heading)));
            lines.push(key_line("  \u{2191}/\u{2193}/j/k   ", "Scroll content", key_style, dim));
            lines.push(key_line("  Esc        ", "Back to list", key_style, dim));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  Actions", heading)));
            lines.push(key_line("  r          ", "Refresh (when stale)", key_style, dim));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  General", heading)));
            lines.push(key_line("  ?          ", "Toggle this help", key_style, dim));
            lines.push(key_line("  q          ", "Quit application", key_style, dim));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  Press ", dim),
        Span::styled("?", key_style),
        Span::styled(" or ", dim),
        Span::styled("Esc", key_style),
        Span::styled(" to close", dim),
    ]));

    lines
}

/// Build a single key-binding line: key in white, description in dim.
fn key_line(key: &'static str, desc: &'static str, key_style: Style, desc_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(key, key_style),
        Span::styled(desc, desc_style),
    ])
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

    #[test]
    fn help_context_from_list_screen() {
        assert_eq!(HelpContext::from_screen(&AppScreen::List), HelpContext::List);
    }

    #[test]
    fn help_context_from_detail_screen() {
        assert_eq!(
            HelpContext::from_screen(&AppScreen::Detail("uuid".into())),
            HelpContext::Detail,
        );
    }

    #[test]
    fn help_context_from_filter_panel_is_list() {
        assert_eq!(
            HelpContext::from_screen(&AppScreen::FilterPanel),
            HelpContext::List,
        );
    }

    #[test]
    fn help_context_from_nested_help_is_list() {
        let nested = AppScreen::Help(Box::new(AppScreen::List));
        assert_eq!(HelpContext::from_screen(&nested), HelpContext::List);
    }

    #[test]
    fn list_help_contains_navigation_keys() {
        let lines = build_help_lines(HelpContext::List);
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Navigate launch list"));
        assert!(text.contains("Enter"));
        assert!(text.contains("Home/End"));
        assert!(text.contains("filter panel"));
    }

    #[test]
    fn list_help_contains_tip() {
        let lines = build_help_lines(HelpContext::List);
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("config.toml"));
    }

    #[test]
    fn detail_help_contains_scroll_keys() {
        let lines = build_help_lines(HelpContext::Detail);
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Scroll content"));
        assert!(text.contains("Back to list"));
    }

    #[test]
    fn detail_help_does_not_contain_filter() {
        let lines = build_help_lines(HelpContext::Detail);
        let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("filter"));
    }

    #[test]
    fn both_contexts_end_with_dismiss_hint() {
        for ctx in [HelpContext::List, HelpContext::Detail] {
            let lines = build_help_lines(ctx);
            let last_line = lines.last().unwrap();
            let text: String = last_line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("to close"), "context {ctx:?} missing dismiss hint");
        }
    }

    #[test]
    fn both_contexts_contain_quit_and_help_toggle() {
        for ctx in [HelpContext::List, HelpContext::Detail] {
            let lines = build_help_lines(ctx);
            let text: String = lines.iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
            assert!(text.contains("Quit application"), "context {ctx:?} missing quit");
            assert!(text.contains("Toggle this help"), "context {ctx:?} missing help toggle");
        }
    }

    #[test]
    fn list_help_has_more_lines_than_detail() {
        let list_lines = build_help_lines(HelpContext::List);
        let detail_lines = build_help_lines(HelpContext::Detail);
        assert!(list_lines.len() > detail_lines.len());
    }
}
