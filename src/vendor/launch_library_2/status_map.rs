//! Launch status ID to display name, abbreviation, color, and style mappings.
//!
//! These mappings are specific to Launch Library 2 and are used by the TUI
//! to render status badges with appropriate colors.

use ratatui::style::{Color, Modifier, Style};

/// Visual representation of a launch status for TUI rendering.
#[derive(Debug, Clone)]
pub struct StatusStyle {
    pub name: &'static str,
    pub abbrev: &'static str,
    pub style: Style,
}

/// Look up the display style for a given LL2 status ID.
///
/// Unknown status IDs fall back to a gray, normal-weight style using the
/// raw status name from the API response.
pub fn status_style(id: u32) -> Option<StatusStyle> {
    let entry = match id {
        1 => StatusStyle {
            name: "Go for Launch",
            abbrev: "Go",
            style: Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        },
        2 => StatusStyle {
            name: "To Be Determined",
            abbrev: "TBD",
            style: Style::default().fg(Color::Yellow),
        },
        3 => StatusStyle {
            name: "Launch Successful",
            abbrev: "Success",
            style: Style::default().fg(Color::Green),
        },
        4 => StatusStyle {
            name: "Launch Failure",
            abbrev: "Failure",
            style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        },
        5 => StatusStyle {
            name: "On Hold",
            abbrev: "Hold",
            style: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        },
        6 => StatusStyle {
            name: "In Flight",
            abbrev: "In Flight",
            style: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        },
        7 => StatusStyle {
            name: "Partial Failure",
            abbrev: "P. Failure",
            style: Style::default().fg(Color::Red),
        },
        8 => StatusStyle {
            name: "To Be Confirmed",
            abbrev: "TBC",
            style: Style::default().fg(Color::Yellow),
        },
        _ => return None,
    };
    Some(entry)
}

/// Returns a fallback style for unknown status IDs.
pub fn unknown_status_style() -> Style {
    Style::default().fg(Color::Gray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_statuses_return_some() {
        for id in [1, 2, 3, 4, 5, 6, 7, 8] {
            assert!(status_style(id).is_some(), "status {id} should be known");
        }
    }

    #[test]
    fn unknown_status_returns_none() {
        assert!(status_style(0).is_none());
        assert!(status_style(99).is_none());
    }

    #[test]
    fn go_for_launch_is_green_bold() {
        let s = status_style(1).unwrap();
        assert_eq!(s.name, "Go for Launch");
        assert_eq!(s.abbrev, "Go");
        assert_eq!(s.style.fg, Some(Color::Green));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn tbd_is_yellow() {
        let s = status_style(2).unwrap();
        assert_eq!(s.name, "To Be Determined");
        assert_eq!(s.abbrev, "TBD");
        assert_eq!(s.style.fg, Some(Color::Yellow));
        assert!(!s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn launch_successful_is_green() {
        let s = status_style(3).unwrap();
        assert_eq!(s.name, "Launch Successful");
        assert_eq!(s.abbrev, "Success");
        assert_eq!(s.style.fg, Some(Color::Green));
        assert!(!s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn launch_failure_is_red_bold() {
        let s = status_style(4).unwrap();
        assert_eq!(s.name, "Launch Failure");
        assert_eq!(s.abbrev, "Failure");
        assert_eq!(s.style.fg, Some(Color::Red));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn on_hold_is_yellow_bold() {
        let s = status_style(5).unwrap();
        assert_eq!(s.name, "On Hold");
        assert_eq!(s.abbrev, "Hold");
        assert_eq!(s.style.fg, Some(Color::Yellow));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn in_flight_is_cyan_bold() {
        let s = status_style(6).unwrap();
        assert_eq!(s.name, "In Flight");
        assert_eq!(s.abbrev, "In Flight");
        assert_eq!(s.style.fg, Some(Color::Cyan));
        assert!(s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn partial_failure_is_red() {
        let s = status_style(7).unwrap();
        assert_eq!(s.name, "Partial Failure");
        assert_eq!(s.abbrev, "P. Failure");
        assert_eq!(s.style.fg, Some(Color::Red));
        assert!(!s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn tbc_is_yellow() {
        let s = status_style(8).unwrap();
        assert_eq!(s.name, "To Be Confirmed");
        assert_eq!(s.abbrev, "TBC");
        assert_eq!(s.style.fg, Some(Color::Yellow));
        assert!(!s.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn unknown_status_style_is_gray() {
        let s = unknown_status_style();
        assert_eq!(s.fg, Some(Color::Gray));
        assert!(!s.add_modifier.contains(Modifier::BOLD));
    }
}
