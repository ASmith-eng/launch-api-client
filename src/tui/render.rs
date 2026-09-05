//! Render dispatch and overlay helpers.
//!
//! The [`render`] function is called once per event loop iteration and
//! delegates to the appropriate view renderer based on the current screen.
//! Overlays (size warning, error indicator) are rendered on top.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::error::{AppError, ErrorState};
use crate::tui::app::{App, AppScreen, MIN_COLS, MIN_ROWS};
use crate::tui::terminal::Tui;
use crate::tui::views::detail;
use crate::tui::views::filter;
use crate::tui::views::help::{self, HelpContext};
use crate::tui::views::list;
use crate::tui::views::splash;

/// Render the current app state to the terminal.
pub fn render(terminal: &mut Tui, app: &App) -> Result<(), AppError> {
    terminal
        .draw(|frame| {
            let area = frame.area();

            if app.is_terminal_too_small() {
                render_size_warning(frame, area);
                return;
            }

            match &app.screen {
                AppScreen::Splash { started_at } => {
                    splash::render_splash(
                        frame,
                        area,
                        started_at.elapsed(),
                        splash::ColourTier::detect(),
                    );
                }
                AppScreen::List => list::render_list(frame, area, app),
                AppScreen::Detail(id) => detail::render_detail(frame, area, id, app),
                AppScreen::Help(prev) => {
                    // Render the underlying screen first, then overlay help.
                    match prev.as_ref() {
                        AppScreen::List => list::render_list(frame, area, app),
                        AppScreen::Detail(id) => {
                            detail::render_detail(frame, area, id, app);
                        }
                        _ => {}
                    }
                    let context = HelpContext::from_screen(prev);
                    help::render_help_overlay(frame, area, context);
                }
                AppScreen::FilterPanel => {
                    list::render_list(frame, area, app);
                    if let Some(ref editing) = app.editing_filter {
                        filter::render_filter_panel(frame, area, editing);
                    }
                }
            }

            // Render error state overlay if present.
            if let Some(error_state) = &app.error_state {
                render_error(frame, area, error_state);
            }
        })
        .map_err(AppError::Io)?;

    Ok(())
}

/// Render a warning when the terminal is too small.
fn render_size_warning(frame: &mut ratatui::Frame, area: Rect) {
    let text = Text::from(vec![
        Line::raw(""),
        Line::raw("  Terminal window too small."),
        Line::raw(""),
        Line::raw(format!("  Please resize to at least {MIN_COLS}x{MIN_ROWS}")),
        Line::raw("  to start seeing rockets!"),
        Line::raw(""),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Warning ");

    let paragraph = Paragraph::new(text).block(block);

    // Center the warning box.
    let popup = centered_rect(42, 8, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(paragraph, popup);
}

/// Render an error state indicator.
fn render_error(frame: &mut ratatui::Frame, area: Rect, error_state: &ErrorState) {
    let msg = match error_state {
        ErrorState::Transient { message, .. } => format!("Error: {message}"),
        ErrorState::RateLimited { available_at } => {
            format!("Rate limited — resets at {}", available_at.format("%H:%M:%S UTC"))
        }
        ErrorState::Offline => "Network offline".to_string(),
        ErrorState::RequestCapExceeded => {
            "Safety limit reached — too many requests. Please restart the app.".to_string()
        }
    };

    let style = match error_state {
        ErrorState::Transient { .. } => Style::default().fg(Color::Yellow),
        ErrorState::RateLimited { .. } | ErrorState::Offline | ErrorState::RequestCapExceeded => {
            Style::default().fg(Color::Red)
        }
    };

    // Render at the bottom of the screen area.
    let error_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let paragraph = Paragraph::new(msg).style(style);
    frame.render_widget(paragraph, error_area);
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
