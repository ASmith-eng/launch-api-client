//! Async event loop multiplexing terminal input and API responses.
//!
//! Uses `tokio::select!` to handle crossterm events alongside pending API
//! calls. Terminal resize events update the app's stored size. The loop
//! exits when `app.should_quit` is set.

use std::pin::Pin;
use std::future::Future;
use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, EventStream};
use futures::StreamExt;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::error::{AppError, ErrorState};
use crate::tui::app::{App, AppScreen, MIN_COLS, MIN_ROWS};
use crate::tui::terminal::Tui;

/// Duration after which a transient error is auto-dismissed (10 seconds).
const TRANSIENT_DISMISS_SECS: u64 = 10;

/// Run the main event loop until the user quits.
///
/// `pending_fetch` allows the caller to pass in an already-in-flight API
/// call (e.g. the initial launch list fetch started during startup).
pub async fn run_event_loop(
    terminal: &mut Tui,
    app: &mut App,
    pending_fetch: Option<Pin<Box<dyn Future<Output = FetchResult> + Send>>>,
) -> Result<(), AppError> {
    let mut reader = EventStream::new();
    let mut pending: Option<Pin<Box<dyn Future<Output = FetchResult> + Send>>> = pending_fetch;

    loop {
        // Auto-dismiss transient errors.
        dismiss_expired_errors(app);

        // Render.
        render(terminal, app)?;

        // Multiplex input events and pending API calls.
        tokio::select! {
            biased;

            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => handle_key(app, key),
                    Some(Ok(Event::Resize(cols, rows))) => {
                        app.terminal_size = (cols, rows);
                    }
                    Some(Err(_)) | None => {
                        // Stream ended or error — treat as quit.
                        app.should_quit = true;
                    }
                    _ => {}
                }
            }

            result = async { pending.as_mut().unwrap().as_mut().await },
                if pending.is_some() && app.loading => {
                pending = None;
                handle_fetch_result(app, result);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Result from an async fetch operation dispatched from the event loop.
#[derive(Debug)]
pub enum FetchResult {
    /// Launch list fetched successfully.
    LaunchList {
        launches: Vec<crate::models::LaunchSummary>,
        total_count: u32,
    },
    /// Launch detail fetched successfully.
    LaunchDetail(Box<crate::models::LaunchDetail>),
    /// Fetch failed.
    Error(AppError),
}

/// Handle the result of a completed fetch.
fn handle_fetch_result(app: &mut App, result: FetchResult) {
    app.loading = false;

    match result {
        FetchResult::LaunchList { launches, total_count } => {
            app.is_offline = false;
            app.error_state = None;
            app.launches = launches;
            app.total_count = total_count;
            // Reset selection if it's now out of bounds.
            if app.selected_index >= app.launches.len() && !app.launches.is_empty() {
                app.selected_index = app.launches.len() - 1;
            }
        }
        FetchResult::LaunchDetail(detail) => {
            app.is_offline = false;
            app.error_state = None;
            // Detail caching and screen transition will be handled in Step 5.2.
            let _ = detail;
        }
        FetchResult::Error(err) => {
            if err.is_offline_signal() {
                app.is_offline = true;
                app.error_state = Some(ErrorState::Offline);
            } else if let AppError::RateLimited(available_at) = err {
                app.error_state = Some(ErrorState::RateLimited { available_at });
            } else {
                app.error_state = Some(ErrorState::Transient {
                    message: err.to_string(),
                    dismiss_at: Instant::now()
                        + std::time::Duration::from_secs(TRANSIENT_DISMISS_SECS),
                });
            }
        }
    }
}

/// Auto-dismiss expired transient errors.
fn dismiss_expired_errors(app: &mut App) {
    if let Some(ErrorState::Transient { dismiss_at, .. }) = &app.error_state {
        if Instant::now() >= *dismiss_at {
            app.error_state = None;
        }
    }
}

/// Process a key event.
fn handle_key(app: &mut App, key: KeyEvent) {
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
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.launches.is_empty() && app.selected_index < app.launches.len() - 1 {
                app.selected_index += 1;
            }
        }
        KeyCode::Home => {
            app.selected_index = 0;
        }
        KeyCode::End => {
            if !app.launches.is_empty() {
                app.selected_index = app.launches.len() - 1;
            }
        }
        KeyCode::Enter => {
            // Transition to detail view (fetch wired in Step 5.2).
            if let Some(launch) = app.launches.get(app.selected_index) {
                let id = launch.id.clone();
                app.detail_scroll_offset = 0;
                app.screen = AppScreen::Detail(id);
            }
        }
        _ => {}
    }
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
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll_offset += 1;
        }
        _ => {}
    }
}

/// Handle keys in the filter panel (placeholder for Step 6.1).
fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.screen = AppScreen::List;
        }
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the current app state to the terminal.
fn render(terminal: &mut Tui, app: &App) -> Result<(), AppError> {
    terminal
        .draw(|frame| {
            let area = frame.area();

            if app.is_terminal_too_small() {
                render_size_warning(frame, area);
                return;
            }

            match &app.screen {
                AppScreen::List => render_list_placeholder(frame, area, app),
                AppScreen::Detail(id) => render_detail_placeholder(frame, area, id, app),
                AppScreen::Help(prev) => {
                    // Render the underlying screen first, then overlay help.
                    match prev.as_ref() {
                        AppScreen::List => render_list_placeholder(frame, area, app),
                        AppScreen::Detail(id) => {
                            render_detail_placeholder(frame, area, id, app);
                        }
                        _ => {}
                    }
                    render_help_overlay(frame, area);
                }
                AppScreen::FilterPanel => render_list_placeholder(frame, area, app),
            }

            // Render error state overlay if present.
            if let Some(error_state) = &app.error_state {
                render_error(frame, area, error_state);
            }
        })
        .map_err(|e| AppError::CacheIo(e.into()))?;

    Ok(())
}

/// Render a warning when the terminal is too small.
fn render_size_warning(frame: &mut ratatui::Frame, area: Rect) {
    let text = Text::from(vec![
        Line::raw(""),
        Line::raw("  Terminal window too small."),
        Line::raw(""),
        Line::raw(format!(
            "  Please resize to at least {MIN_COLS}x{MIN_ROWS}"
        )),
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

/// Placeholder list view rendering (full implementation in Step 4.2).
fn render_list_placeholder(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let title = if app.loading {
        " Fetching launches... "
    } else {
        &format!(
            " Launches — Showing {} of {} ",
            app.launches.len(),
            app.total_count
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title);

    let content = if app.loading {
        "Fetching launches...".to_string()
    } else if app.launches.is_empty() {
        "No launches to display.".to_string()
    } else {
        app.launches
            .iter()
            .enumerate()
            .map(|(i, launch)| {
                let marker = if i == app.selected_index { "▸ " } else { "  " };
                format!("{marker}{}", launch.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let paragraph = Paragraph::new(content).block(block);
    frame.render_widget(paragraph, area);
}

/// Placeholder detail view rendering (full implementation in Step 5.1).
fn render_detail_placeholder(
    frame: &mut ratatui::Frame,
    area: Rect,
    launch_id: &str,
    _app: &App,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Detail — {launch_id} "));

    let paragraph =
        Paragraph::new("Detail view placeholder. Press Esc to go back.").block(block);
    frame.render_widget(paragraph, area);
}

/// Render a help overlay (placeholder for Step 6.2).
fn render_help_overlay(frame: &mut ratatui::Frame, area: Rect) {
    let text = Text::from(vec![
        Line::raw(""),
        Line::raw("  Keyboard Shortcuts"),
        Line::raw("  ──────────────────"),
        Line::raw("  ↑/k      Move up"),
        Line::raw("  ↓/j      Move down"),
        Line::raw("  Enter    View details"),
        Line::raw("  Esc      Go back"),
        Line::raw("  ?        Toggle help"),
        Line::raw("  q        Quit"),
        Line::raw(""),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");

    let popup = centered_rect(30, 12, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(text).block(block), popup);
}

/// Render an error state indicator.
fn render_error(frame: &mut ratatui::Frame, area: Rect, error_state: &ErrorState) {
    let msg = match error_state {
        ErrorState::Transient { message, .. } => format!("Error: {message}"),
        ErrorState::RateLimited { available_at } => {
            format!("Rate limited — resets at {}", available_at.format("%H:%M:%S UTC"))
        }
        ErrorState::Offline => "Network offline".to_string(),
    };

    let style = match error_state {
        ErrorState::Transient { .. } => Style::default().fg(Color::Yellow),
        ErrorState::RateLimited { .. } => Style::default().fg(Color::Red),
        ErrorState::Offline => Style::default().fg(Color::Red),
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
