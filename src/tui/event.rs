//! Async event loop multiplexing terminal input and API responses.
//!
//! Uses `tokio::select!` to handle crossterm events alongside pending API
//! calls. Terminal resize events update the app's stored size. The loop
//! exits when `app.should_quit` is set.
//!
//! ## Fetch dispatch
//!
//! The event loop owns all async fetch decisions through a unified
//! state-driven check each iteration:
//!
//! - **List screen, empty data**: auto-fetch (first run / empty cache)
//! - **Detail screen, missing or stale cache**: auto-fetch on navigation
//! - **Manual refresh (`r` key)**: `app.refresh_requested` flag, validated
//!   against staleness and rate-limit before dispatching
//!
//! This avoids key handlers needing async access — they stay pure.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::signal;
use tokio::time::{interval, MissedTickBehavior};
use tracing::debug;

use crate::api::client::Ll2Client;
use crate::cache::CacheManager;
use crate::clock::Clock;
use crate::config::CacheConfig;
use crate::error::AppError;
use crate::tui::app::{App, AppScreen};
use crate::tui::fetch::{
    check_needs_fetch, check_needs_throttle_sync, dismiss_expired_errors, handle_fetch_result,
    maybe_load_detail_from_disk, spawn_fetch, FetchResult,
};
use crate::tui::keys::handle_key;
use crate::tui::render::render;
use crate::tui::terminal::Tui;
use crate::tui::views::splash;

/// Run the main event loop until the user quits.
///
/// The loop drives all data fetching based on the current app state —
/// callers only need to populate `app` with any cached data before entry.
pub async fn run_event_loop<C: Clock + Send + Sync + 'static>(
    terminal: &mut Tui,
    app: &mut App,
    client: Arc<Ll2Client<C>>,
    cache_manager: &CacheManager<C>,
    cache_config: &CacheConfig,
) -> Result<(), AppError> {
    let mut reader = EventStream::new();
    let mut pending: Option<Pin<Box<dyn Future<Output = FetchResult> + Send>>> = None;
    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    // 1-second tick for live countdown updates on the Detail screen.
    // `MissedTickBehavior::Skip` avoids bursts of redraws if a frame takes
    // longer than 1 s (e.g. during a slow API call).
    let mut tick = interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Frame timer for the splash animation. Gated below to the animated screen,
    // so once the splash exits the branch is never polled again.
    let mut frame_tick = interval(Duration::from_millis(splash::FRAME_MS));
    frame_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let splash_duration = splash::ColourTier::detect().duration();

    loop {
        // Auto-dismiss transient errors.
        dismiss_expired_errors(app);

        // Try loading a detail from disk cache before checking if we need a fetch.
        maybe_load_detail_from_disk(app, cache_manager).await;

        // State-driven fetch dispatch: check if the current screen needs data.
        // Proactive throttle sync takes priority (cheap, keeps rate limit accurate).
        if pending.is_none() {
            if let Some(kind) = check_needs_throttle_sync(app, &client).await {
                pending = Some(spawn_fetch(kind, &client, app));
            } else if let Some(kind) = check_needs_fetch(app) {
                app.refresh_requested = false;
                app.loading = true;
                pending = Some(spawn_fetch(kind, &client, app));
            } else if app.refresh_requested {
                // Request was set but conditions weren't met (fresh cache, rate limited).
                debug!("refresh requested but skipped (cache fresh or rate limited)");
                app.refresh_requested = false;
            }
        }

        // Render.
        render(terminal, app)?;

        // Multiplex input events, OS signals, and pending API calls.
        tokio::select! {
            biased;

            // OS-level SIGINT — crossterm may not deliver Ctrl+C as a key
            // event during long operations. This branch ensures graceful
            // shutdown with state persistence regardless.
            _ = &mut ctrl_c => {
                debug!("received SIGINT, shutting down");
                app.should_quit = true;
            }

            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => handle_key(app, key),
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
                if pending.is_some() => {
                pending = None;
                handle_fetch_result(app, &client, cache_manager, cache_config, result).await;
            }

            // Live countdown tick — re-render every second while on the
            // Detail screen so the countdown timer visibly decrements.
            // On other screens the tick fires but we skip the redraw.
            _ = tick.tick(), if matches!(app.screen, AppScreen::Detail(_)) => {
                // No state change needed — countdown is computed at render
                // time from the launch's `net` field. The next loop iteration
                // will call `render()`.
            }

            // Splash frame tick. The frame itself is derived from elapsed time
            // at render, so this only wakes the loop and retires the splash
            // once its sequence has run its full length.
            _ = frame_tick.tick(), if app.screen.is_animating() => {
                let finished = matches!(
                    &app.screen,
                    AppScreen::Splash { started_at } if started_at.elapsed() >= splash_duration
                );
                if finished {
                    app.screen = AppScreen::List;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
