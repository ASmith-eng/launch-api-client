//! Time formatting utilities shared across TUI views.
//!
//! Handles dual-timezone display, net precision awareness, countdown timers
//! with proximity-based styling, and relative time formatting for staleness.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use ratatui::style::{Color, Modifier, Style};

use crate::models::NetPrecision;

// ---------------------------------------------------------------------------
// Net precision–aware time display
// ---------------------------------------------------------------------------

/// Format a launch NET time according to its precision level.
///
/// - **Hour/Minute** (or unknown): dual timezone (`Feb 28 03:00 CST / 09:00 UTC`)
/// - **Day**: date only (`Feb 28, 2026`)
/// - **Month**: month and year (`Mar 2026`)
/// - **Year**: year only (`2026`)
pub fn format_net_time(
    net: &DateTime<Utc>,
    precision: Option<&NetPrecision>,
    timezone_name: &Option<String>,
) -> String {
    let abbrev = precision.map(|p| p.abbrev.as_str()).unwrap_or("");

    match abbrev {
        "Month" => net.format("%b %Y").to_string(),
        "Year" => net.format("%Y").to_string(),
        "Day" => net.format("%b %d, %Y").to_string(),
        _ => format_dual_timezone(net, timezone_name),
    }
}

/// Format a datetime as dual timezone: `Feb 28 03:00 CST / 09:00 UTC`.
///
/// If `timezone_name` is `None` or unparseable, falls back to UTC only.
pub fn format_dual_timezone(net: &DateTime<Utc>, timezone_name: &Option<String>) -> String {
    let utc_str = net.format("%H:%M UTC").to_string();

    if let Some(tz_name) = timezone_name {
        if let Ok(tz) = tz_name.parse::<Tz>() {
            let local = net.with_timezone(&tz);
            let tz_abbrev = local.format("%Z").to_string();
            let local_str = local.format("%b %d %H:%M").to_string();
            return format!("{local_str} {tz_abbrev} / {utc_str}");
        }
    }

    // Fallback: just UTC.
    net.format("%b %d %H:%M UTC").to_string()
}

/// Whether the precision is fine enough to display a countdown.
///
/// Countdowns are only meaningful when the NET is known to at least day
/// precision. Month and year precision are too coarse.
pub fn has_precise_time(precision: Option<&NetPrecision>) -> bool {
    let abbrev = precision.map(|p| p.abbrev.as_str()).unwrap_or("");
    !matches!(abbrev, "Month" | "Year")
}

// ---------------------------------------------------------------------------
// Countdown timer
// ---------------------------------------------------------------------------

/// Format a countdown from `now` to `target`.
///
/// Returns `T-dd days, hh:mm:ss` for future launches and
/// `T+dd days, hh:mm:ss` for past/in-flight launches.
/// When the remaining time is less than 1 day, the days component is omitted:
/// `T-hh:mm:ss` / `T+hh:mm:ss`.
pub fn format_countdown(now: DateTime<Utc>, target: DateTime<Utc>) -> String {
    let diff = target.signed_duration_since(now);
    let total_secs = diff.num_seconds();
    let prefix = if total_secs >= 0 { "T-" } else { "T+" };
    let abs_secs = total_secs.unsigned_abs();

    let days = abs_secs / 86_400;
    let hours = (abs_secs % 86_400) / 3_600;
    let minutes = (abs_secs % 3_600) / 60;
    let seconds = abs_secs % 60;

    if days > 0 {
        format!(
            "{prefix}{days:02} days, {hours:02}:{minutes:02}:{seconds:02}"
        )
    } else {
        format!("{prefix}{hours:02}:{minutes:02}:{seconds:02}")
    }
}

/// Style for the countdown timer based on proximity to launch.
///
/// | Time to Launch | Style            |
/// |----------------|------------------|
/// | > 7 days       | Dim white        |
/// | 1–7 days       | Normal white     |
/// | < 24 hours     | Yellow bold      |
/// | < 1 hour       | Red bold         |
/// | T+ (past/in-flight) | Cyan bold   |
pub fn countdown_style(now: DateTime<Utc>, target: DateTime<Utc>) -> Style {
    let diff = target.signed_duration_since(now);
    let total_secs = diff.num_seconds();

    if total_secs < 0 {
        // Past launch / in-flight.
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if total_secs < 3_600 {
        // < 1 hour.
        Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else if total_secs < 86_400 {
        // < 24 hours.
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if total_secs < 7 * 86_400 {
        // 1–7 days.
        Style::default().fg(Color::White)
    } else {
        // > 7 days.
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::DIM)
    }
}

// ---------------------------------------------------------------------------
// Absolute time formatting (for status bar)
// ---------------------------------------------------------------------------

/// Format a UTC datetime as a local-looking time string.
///
/// - `"12h"` → `"10:45 AM"`
/// - `"24h"` → `"10:45"`
///
/// Uses UTC directly (the app doesn't track user timezone — consistent with
/// dual-timezone display elsewhere).
pub fn format_time_of_day(time: &DateTime<Utc>, time_format: &str) -> String {
    match time_format {
        "24h" => time.format("%H:%M").to_string(),
        _ => time.format("%l:%M %p").to_string().trim_start().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Relative time formatting (for staleness display)
// ---------------------------------------------------------------------------

/// Format a duration between `then` and `now` as a human-readable relative
/// time string (e.g., `"5m ago"`, `"2h ago"`, `"3d ago"`).
///
/// `then` is expected to be in the past relative to `now`. If `then` is in
/// the future, returns `"just now"`.
pub fn format_relative_time(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = now.signed_duration_since(then);
    let total_secs = diff.num_seconds();

    if total_secs < 0 {
        return "just now".to_string();
    }

    let total_secs = total_secs as u64;

    if total_secs < 60 {
        "just now".to_string()
    } else if total_secs < 3_600 {
        let mins = total_secs / 60;
        format!("{mins}m ago")
    } else if total_secs < 86_400 {
        let hours = total_secs / 3_600;
        format!("{hours}h ago")
    } else {
        let days = total_secs / 86_400;
        format!("{days}d ago")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn net_precision(abbrev: &str) -> NetPrecision {
        NetPrecision {
            id: 0,
            name: abbrev.to_string(),
            abbrev: abbrev.to_string(),
        }
    }

    // ── format_net_time ─────────────────────────────────────────────────

    #[test]
    fn net_time_month_precision() {
        let net: DateTime<Utc> = "2026-09-15T00:00:00Z".parse().unwrap();
        assert_eq!(
            format_net_time(&net, Some(&net_precision("Month")), &None),
            "Sep 2026"
        );
    }

    #[test]
    fn net_time_year_precision() {
        let net: DateTime<Utc> = "2027-01-01T00:00:00Z".parse().unwrap();
        assert_eq!(
            format_net_time(&net, Some(&net_precision("Year")), &None),
            "2027"
        );
    }

    #[test]
    fn net_time_day_precision() {
        let net: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(
            format_net_time(&net, Some(&net_precision("Day")), &None),
            "Feb 28, 2026"
        );
    }

    #[test]
    fn net_time_hour_precision_with_timezone() {
        let net: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        let tz = Some("America/Chicago".to_string());
        let result = format_net_time(&net, Some(&net_precision("Hour")), &tz);
        // CST = UTC-6, so 09:00 UTC = 03:00 CST.
        assert_eq!(result, "Feb 28 03:00 CST / 09:00 UTC");
    }

    #[test]
    fn net_time_minute_precision_with_timezone() {
        let net: DateTime<Utc> = "2026-03-02T14:30:00Z".parse().unwrap();
        let tz = Some("America/New_York".to_string());
        let result = format_net_time(&net, Some(&net_precision("Minute")), &tz);
        // EST = UTC-5, so 14:30 UTC = 09:30 EST.
        assert_eq!(result, "Mar 02 09:30 EST / 14:30 UTC");
    }

    #[test]
    fn net_time_no_precision_fallback_utc() {
        let net: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(
            format_net_time(&net, None, &None),
            "Feb 28 09:00 UTC"
        );
    }

    #[test]
    fn net_time_invalid_timezone_falls_back_to_utc() {
        let net: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        let tz = Some("Not/A/Timezone".to_string());
        assert_eq!(
            format_net_time(&net, None, &tz),
            "Feb 28 09:00 UTC"
        );
    }

    #[test]
    fn net_time_tokyo_timezone() {
        let net: DateTime<Utc> = "2026-06-15T03:00:00Z".parse().unwrap();
        let tz = Some("Asia/Tokyo".to_string());
        let result = format_net_time(&net, Some(&net_precision("Hour")), &tz);
        // JST = UTC+9, so 03:00 UTC = 12:00 JST.
        assert_eq!(result, "Jun 15 12:00 JST / 03:00 UTC");
    }

    #[test]
    fn net_time_utc_timezone_explicit() {
        let net: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        let tz = Some("UTC".to_string());
        let result = format_net_time(&net, Some(&net_precision("Hour")), &tz);
        assert_eq!(result, "Feb 28 09:00 UTC / 09:00 UTC");
    }

    // ── has_precise_time ────────────────────────────────────────────────

    #[test]
    fn precise_time_hour() {
        assert!(has_precise_time(Some(&net_precision("Hour"))));
    }

    #[test]
    fn precise_time_minute() {
        assert!(has_precise_time(Some(&net_precision("Minute"))));
    }

    #[test]
    fn precise_time_day() {
        assert!(has_precise_time(Some(&net_precision("Day"))));
    }

    #[test]
    fn imprecise_time_month() {
        assert!(!has_precise_time(Some(&net_precision("Month"))));
    }

    #[test]
    fn imprecise_time_year() {
        assert!(!has_precise_time(Some(&net_precision("Year"))));
    }

    #[test]
    fn precise_time_none() {
        // No precision info → treated as precise (default to showing time).
        assert!(has_precise_time(None));
    }

    // ── format_countdown ────────────────────────────────────────────────

    #[test]
    fn countdown_future_with_days() {
        let now: DateTime<Utc> = "2026-02-27T12:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-03-01T02:23:45Z".parse().unwrap();
        assert_eq!(
            format_countdown(now, target),
            "T-01 days, 14:23:45"
        );
    }

    #[test]
    fn countdown_future_no_days() {
        let now: DateTime<Utc> = "2026-02-28T07:36:15Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_countdown(now, target), "T-01:23:45");
    }

    #[test]
    fn countdown_past_in_flight() {
        let now: DateTime<Utc> = "2026-02-28T09:05:30Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_countdown(now, target), "T+00:05:30");
    }

    #[test]
    fn countdown_past_with_days() {
        let now: DateTime<Utc> = "2026-03-02T10:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(
            format_countdown(now, target),
            "T+02 days, 01:00:00"
        );
    }

    #[test]
    fn countdown_exact_zero() {
        let t: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_countdown(t, t), "T-00:00:00");
    }

    #[test]
    fn countdown_exactly_one_day() {
        let now: DateTime<Utc> = "2026-02-27T09:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(
            format_countdown(now, target),
            "T-01 days, 00:00:00"
        );
    }

    #[test]
    fn countdown_large_value() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-12-31T23:59:59Z".parse().unwrap();
        let result = format_countdown(now, target);
        assert!(result.starts_with("T-"));
        assert!(result.contains("days,"));
    }

    // ── countdown_style ─────────────────────────────────────────────────

    #[test]
    fn style_more_than_7_days() {
        let now: DateTime<Utc> = "2026-02-20T00:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-03-01T00:00:00Z".parse().unwrap();
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::White));
        assert!(style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn style_1_to_7_days() {
        let now: DateTime<Utc> = "2026-02-26T00:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T00:00:00Z".parse().unwrap();
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::White));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_less_than_24_hours() {
        let now: DateTime<Utc> = "2026-02-28T00:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T12:00:00Z".parse().unwrap();
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_less_than_1_hour() {
        let now: DateTime<Utc> = "2026-02-28T08:30:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::Red));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_past_in_flight() {
        let now: DateTime<Utc> = "2026-02-28T09:10:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::Cyan));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_exactly_7_days() {
        let now: DateTime<Utc> = "2026-02-21T09:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        // Exactly 7 days → 604800 seconds, which is NOT < 7*86400, so → dim white.
        let style = countdown_style(now, target);
        assert!(style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn style_exactly_24_hours() {
        let now: DateTime<Utc> = "2026-02-27T09:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        // Exactly 24h → 86400 seconds, which is NOT < 86400, so → normal white (1-7 day tier).
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::White));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn style_exactly_1_hour() {
        let now: DateTime<Utc> = "2026-02-28T08:00:00Z".parse().unwrap();
        let target: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        // Exactly 1h → 3600 seconds, which is NOT < 3600, so → yellow bold (< 24h tier).
        let style = countdown_style(now, target);
        assert_eq!(style.fg, Some(Color::Yellow));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    // ── format_relative_time ────────────────────────────────────────────

    #[test]
    fn relative_just_now_under_60s() {
        let now: DateTime<Utc> = "2026-02-28T09:00:30Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "just now");
    }

    #[test]
    fn relative_minutes() {
        let now: DateTime<Utc> = "2026-02-28T09:05:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "5m ago");
    }

    #[test]
    fn relative_hours() {
        let now: DateTime<Utc> = "2026-02-28T11:00:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "2h ago");
    }

    #[test]
    fn relative_days() {
        let now: DateTime<Utc> = "2026-03-02T09:00:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "2d ago");
    }

    #[test]
    fn relative_future_returns_just_now() {
        let now: DateTime<Utc> = "2026-02-28T08:00:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "just now");
    }

    #[test]
    fn relative_exactly_60_seconds() {
        let now: DateTime<Utc> = "2026-02-28T09:01:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "1m ago");
    }

    #[test]
    fn relative_exactly_1_hour() {
        let now: DateTime<Utc> = "2026-02-28T10:00:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "1h ago");
    }

    #[test]
    fn relative_exactly_1_day() {
        let now: DateTime<Utc> = "2026-03-01T09:00:00Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "1d ago");
    }

    #[test]
    fn relative_59_minutes() {
        let now: DateTime<Utc> = "2026-02-28T09:59:59Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "59m ago");
    }

    #[test]
    fn relative_23_hours() {
        let now: DateTime<Utc> = "2026-03-01T08:59:59Z".parse().unwrap();
        let then: DateTime<Utc> = "2026-02-28T09:00:00Z".parse().unwrap();
        assert_eq!(format_relative_time(then, now), "23h ago");
    }

    // ── format_time_of_day ──────────────────────────────────────────────

    #[test]
    fn time_of_day_12h_morning() {
        let t: DateTime<Utc> = "2026-02-28T09:45:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "12h"), "9:45 AM");
    }

    #[test]
    fn time_of_day_12h_afternoon() {
        let t: DateTime<Utc> = "2026-02-28T14:30:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "12h"), "2:30 PM");
    }

    #[test]
    fn time_of_day_12h_midnight() {
        let t: DateTime<Utc> = "2026-02-28T00:05:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "12h"), "12:05 AM");
    }

    #[test]
    fn time_of_day_12h_noon() {
        let t: DateTime<Utc> = "2026-02-28T12:00:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "12h"), "12:00 PM");
    }

    #[test]
    fn time_of_day_24h_morning() {
        let t: DateTime<Utc> = "2026-02-28T09:45:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "24h"), "09:45");
    }

    #[test]
    fn time_of_day_24h_afternoon() {
        let t: DateTime<Utc> = "2026-02-28T14:30:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "24h"), "14:30");
    }

    #[test]
    fn time_of_day_24h_midnight() {
        let t: DateTime<Utc> = "2026-02-28T00:05:00Z".parse().unwrap();
        assert_eq!(format_time_of_day(&t, "24h"), "00:05");
    }

    #[test]
    fn time_of_day_unknown_format_defaults_to_12h() {
        let t: DateTime<Utc> = "2026-02-28T14:30:00Z".parse().unwrap();
        // Unknown format falls through to the 12h branch.
        assert_eq!(format_time_of_day(&t, "bogus"), "2:30 PM");
    }
}
