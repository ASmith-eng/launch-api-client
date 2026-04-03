//! Semantic colour palette and text hierarchy constants.
//!
//! Centralises the 5-colour palette and 3-tier text hierarchy from the
//! design doc so that all views reference the same constants. This avoids
//! scattered `Style::default().fg(Color::…)` calls and makes palette-wide
//! changes a single-point edit.

use ratatui::style::{Color, Modifier, Style};

// ---------------------------------------------------------------------------
// 5-colour semantic palette
// ---------------------------------------------------------------------------

/// Green — positive status (Go, Success), high probability, success bar.
pub const GREEN: Color = Color::Green;
/// Yellow — uncertain status (TBD, TBC, Hold), medium probability, imminent countdown.
pub const YELLOW: Color = Color::Yellow;
/// Red — negative status (Failure), low probability, very imminent countdown, failure bar.
pub const RED: Color = Color::Red;
/// Cyan — In Flight status, link labels.
pub const CYAN: Color = Color::Cyan;
/// Dark Gray — labels, separators, structural chrome.
pub const CHROME: Color = Color::DarkGray;

// ---------------------------------------------------------------------------
// 3-tier text hierarchy
// ---------------------------------------------------------------------------

/// Tier 1 — Primary: values the user is scanning for.
///
/// Bold white. Used for: countdown, launch name, status text, NET datetime.
pub fn primary() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

/// Tier 2 — Secondary: supporting context.
///
/// Normal weight, white/default. Used for: provider name, rocket name,
/// mission description, pad name, location, link URLs.
pub fn secondary() -> Style {
    Style::default().fg(Color::White)
}

/// Tier 3 — Labels & Chrome: structural, not content.
///
/// Dim, gray. Used for: section headings, field labels, separator lines,
/// `·` delimiters, status bar text.
pub fn label() -> Style {
    Style::default().fg(CHROME)
}

/// Dim separator line style (Tier 3 with DIM modifier).
pub fn separator() -> Style {
    Style::default()
        .fg(CHROME)
        .add_modifier(Modifier::DIM)
}

/// Section heading style (Tier 3 with BOLD modifier).
///
/// Used for `VEHICLE`, `LOCATION`, `MISSION`, `LINKS` headings.
pub fn section_heading() -> Style {
    Style::default()
        .fg(CHROME)
        .add_modifier(Modifier::BOLD)
}

// ---------------------------------------------------------------------------
// Probability colouring
// ---------------------------------------------------------------------------

/// Style for a launch probability value.
///
/// | Range    | Color  |
/// |----------|--------|
/// | 80–100%  | Green  |
/// | 50–79%   | Yellow |
/// | 0–49%    | Red    |
/// | None     | Dim gray ("N/A") |
pub fn probability_style(probability: Option<crate::models::Probability>) -> Style {
    match probability.map(|p| p.value()) {
        Some(p) if p >= 80 => Style::default().fg(GREEN),
        Some(p) if p >= 50 => Style::default().fg(YELLOW),
        Some(_) => Style::default().fg(RED),
        None => label(),
    }
}

// ---------------------------------------------------------------------------
// Record bar colours
// ---------------------------------------------------------------------------

/// Style for the success portion of a provider record bar.
pub fn record_success() -> Style {
    Style::default().fg(GREEN)
}

/// Style for the failure portion of a provider record bar.
pub fn record_failure() -> Style {
    Style::default().fg(RED)
}

// ---------------------------------------------------------------------------
// Link styles
// ---------------------------------------------------------------------------

/// Link label style (Cyan, per design doc).
pub fn link_label() -> Style {
    Style::default().fg(CYAN)
}

/// Link URL style (Dim + underline).
pub fn link_url() -> Style {
    Style::default()
        .fg(CHROME)
        .add_modifier(Modifier::DIM)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Probability;

    fn prob(v: u8) -> Option<Probability> {
        Probability::new(v)
    }

    #[test]
    fn primary_is_bold_white() {
        let s = primary();
        assert_eq!(s.fg, Some(Color::White));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn secondary_is_normal_white() {
        let s = secondary();
        assert_eq!(s.fg, Some(Color::White));
        assert!(!s.add_modifier.contains(Modifier::BOLD));
        assert!(!s.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn label_is_dark_gray() {
        let s = label();
        assert_eq!(s.fg, Some(Color::DarkGray));
    }

    #[test]
    fn separator_is_dim_dark_gray() {
        let s = separator();
        assert_eq!(s.fg, Some(Color::DarkGray));
        assert!(s.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn section_heading_is_bold_dark_gray() {
        let s = section_heading();
        assert_eq!(s.fg, Some(Color::DarkGray));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn probability_high_is_green() {
        assert_eq!(probability_style(prob(80)).fg, Some(Color::Green));
        assert_eq!(probability_style(prob(100)).fg, Some(Color::Green));
    }

    #[test]
    fn probability_medium_is_yellow() {
        assert_eq!(probability_style(prob(50)).fg, Some(Color::Yellow));
        assert_eq!(probability_style(prob(79)).fg, Some(Color::Yellow));
    }

    #[test]
    fn probability_low_is_red() {
        assert_eq!(probability_style(prob(0)).fg, Some(Color::Red));
        assert_eq!(probability_style(prob(49)).fg, Some(Color::Red));
    }

    #[test]
    fn probability_none_is_dim_gray() {
        assert_eq!(probability_style(None).fg, Some(Color::DarkGray));
    }

    #[test]
    fn record_success_is_green() {
        assert_eq!(record_success().fg, Some(Color::Green));
    }

    #[test]
    fn record_failure_is_red() {
        assert_eq!(record_failure().fg, Some(Color::Red));
    }

    #[test]
    fn link_label_is_cyan() {
        assert_eq!(link_label().fg, Some(Color::Cyan));
    }

    #[test]
    fn link_url_is_dim() {
        let s = link_url();
        assert_eq!(s.fg, Some(Color::DarkGray));
        assert!(s.add_modifier.contains(Modifier::DIM));
    }
}
