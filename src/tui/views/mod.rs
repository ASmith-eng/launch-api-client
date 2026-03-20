//! TUI view renderers (list, detail, filter, help).

pub mod list;
pub mod status_bar;

use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders};

/// Create a bordered block with rounded corners and `DarkGray` border.
///
/// All bordered UI components share this styling for consistency.
pub fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
}
