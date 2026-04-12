//! TUI view renderers (list, detail, filter, help).

pub mod detail;
pub mod filter;
pub mod help;
pub mod list;
pub mod status_bar;

use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

/// Builder for a consistently styled title bar block with optional
/// left, center, and right title segments.
///
/// All bordered view components should use this to ensure consistent
/// rounded borders and `DarkGray` styling.
pub struct TitleBar {
    left: Option<Line<'static>>,
    center: Option<Line<'static>>,
    right: Option<Line<'static>>,
}

impl TitleBar {
    /// Start building a title bar with a left-aligned title.
    pub fn new(left: Line<'static>) -> Self {
        Self {
            left: Some(left),
            center: None,
            right: None,
        }
    }

    /// Set the centered title segment.
    pub fn center(mut self, center: Line<'static>) -> Self {
        self.center = Some(center);
        self
    }

    /// Set the right-aligned title segment.
    pub fn right(mut self, right: Line<'static>) -> Self {
        self.right = Some(right);
        self
    }

    /// Build the final [`Block`] with all configured title segments.
    pub fn build(self) -> Block<'static> {
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray));

        if let Some(left) = self.left {
            block = block.title(left);
        }
        if let Some(center) = self.center {
            block = block.title_top(center.alignment(Alignment::Center));
        }
        if let Some(right) = self.right {
            block = block.title_top(right.alignment(Alignment::Right));
        }

        block
    }
}

/// Build a right-aligned rate limit title segment, if data is available.
pub fn rate_limit_title(
    remaining: Option<u32>,
    total: Option<u32>,
) -> Option<Line<'static>> {
    match (remaining, total) {
        (Some(r), Some(t)) => Some(Line::from(Span::raw(format!(" {r}/{t} reqs ")))),
        _ => None,
    }
}
