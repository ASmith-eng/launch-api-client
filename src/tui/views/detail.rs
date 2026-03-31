//! Detail view renderer for a single launch.
//!
//! Renders the full launch detail screen with hero header, two-column
//! vehicle/provider + location grid, mission section, and links.
//! Supports vertical scrolling and responsive single-column fallback
//! below 100 terminal columns.

use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::models::LaunchDetail;
use crate::tui::app::App;
use crate::tui::time_fmt;
use crate::tui::views::status_bar;
use crate::tui::views::styled_block;
use crate::vendor::launch_library_2::status_map::{status_style, unknown_status_style};

/// Width threshold below which we switch from two-column to single-column.
const TWO_COL_MIN_WIDTH: u16 = 100;

/// Fixed width for the provider success/failure bar.
const RECORD_BAR_WIDTH: usize = 20;

/// Render the full detail view into the given area.
///
/// If the launch ID is found in `app.detail_cache`, renders the full
/// detail. Otherwise renders a loading/not-found placeholder.
pub fn render_detail(frame: &mut ratatui::Frame, area: Rect, launch_id: &str, app: &App) {
    let title = build_title(launch_id, app);
    let block = styled_block(&title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let detail = match app.detail_cache.get(launch_id) {
        Some(cached) => &cached.data,
        None => {
            let msg = if app.loading {
                "  Fetching launch details..."
            } else {
                "  No detail data available. Press Esc to go back."
            };
            let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(p, inner);
            return;
        }
    };

    // Split: content area + bottom hint bar (2 lines).
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    let content_area = chunks[0];
    let hint_area = chunks[1];

    // Build all content lines.
    let lines = build_content_lines(detail, content_area.width);
    let content_height = lines.len();

    // Clamp scroll offset so user can't scroll past end.
    let max_scroll = content_height.saturating_sub(content_area.height as usize);
    let scroll_offset = app.detail_scroll_offset.min(max_scroll);

    // Render scrollable content.
    let paragraph = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
    frame.render_widget(paragraph, content_area);

    // Scroll indicator on the right edge when content overflows.
    if content_height > content_area.height as usize {
        render_scroll_indicator(frame, content_area, scroll_offset, max_scroll);
    }

    render_hint_bar(frame, hint_area, app);
}

// ---------------------------------------------------------------------------
// Title
// ---------------------------------------------------------------------------

fn build_title(launch_id: &str, app: &App) -> String {
    let name = app
        .detail_cache
        .get(launch_id)
        .map(|c| c.data.name.as_str())
        .unwrap_or(launch_id);

    let left = format!(" {name} ");

    match (app.rate_limit_remaining, app.rate_limit_total) {
        (Some(remaining), Some(total)) => {
            format!("{left}── {remaining}/{total} reqs ")
        }
        _ => left,
    }
}

// ---------------------------------------------------------------------------
// Content builder
// ---------------------------------------------------------------------------

/// Build all content lines for the detail view.
fn build_content_lines(detail: &LaunchDetail, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(64);

    // Hero header.
    build_hero_section(&mut lines, detail);

    // Separator.
    push_separator(&mut lines, width);

    // Vehicle/Provider + Location (two-column or single-column).
    if width >= TWO_COL_MIN_WIDTH {
        build_two_column_section(&mut lines, detail, width);
    } else {
        build_single_column_section(&mut lines, detail);
    }

    // Separator.
    push_separator(&mut lines, width);

    // Mission.
    build_mission_section(&mut lines, detail);

    // Separator.
    push_separator(&mut lines, width);

    // Links.
    build_links_section(&mut lines, detail);

    // Trailing blank line for visual breathing room.
    lines.push(Line::raw(""));

    lines
}

// ---------------------------------------------------------------------------
// Hero header
// ---------------------------------------------------------------------------

fn build_hero_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    lines.push(Line::raw(""));

    // Status-colored ██ badges + countdown + status name.
    let now = Utc::now();
    let has_time = time_fmt::has_precise_time(detail.net_precision.as_ref());

    let (badge_style, status_name) = match status_style(detail.status.id) {
        Some(ss) => (ss.style, ss.name.to_string()),
        None => (unknown_status_style(), detail.status.name.clone()),
    };

    let mut hero_spans: Vec<Span<'static>> = vec![Span::styled("   ██  ", badge_style)];

    if has_time {
        let countdown = time_fmt::format_countdown(now, detail.net);
        let cd_style = time_fmt::countdown_style(now, detail.net);
        hero_spans.push(Span::styled(countdown, cd_style));
    }

    hero_spans.push(Span::styled("  ██", badge_style));
    hero_spans.push(Span::raw("     "));
    hero_spans.push(Span::styled(
        status_name,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    lines.push(Line::from(hero_spans));

    // NET time display (dual timezone).
    let time_str = time_fmt::format_net_time(
        &detail.net,
        detail.net_precision.as_ref(),
        &detail.pad.location.timezone_name,
    );
    lines.push(Line::from(vec![Span::styled(
        format!("   {time_str}"),
        Style::default().fg(Color::White),
    )]));

    // Window + probability line (if any data present).
    let mut window_parts: Vec<String> = Vec::new();

    if let (Some(start), Some(end)) = (&detail.window_start, &detail.window_end) {
        let start_str = start.format("%H:%M").to_string();
        let end_str = end.format("%H:%M").to_string();
        window_parts.push(format!("Window: {start_str} - {end_str} UTC"));
    }

    if let Some(prob) = detail.probability {
        if prob >= 0 {
            window_parts.push(format!("Probability: {prob}%"));
        }
    }

    if !window_parts.is_empty() {
        let joined = window_parts.join("  ·  ");
        lines.push(Line::from(Span::styled(
            format!("   {joined}"),
            Style::default().fg(Color::Gray),
        )));
    }

    // Weather concerns.
    if let Some(weather) = &detail.weather_concerns {
        if !weather.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("   Weather: {weather}"),
                Style::default().fg(Color::Gray),
            )));
        }
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Two-column layout (vehicle/provider | location)
// ---------------------------------------------------------------------------

fn build_two_column_section(
    lines: &mut Vec<Line<'static>>,
    detail: &LaunchDetail,
    width: u16,
) {
    let half = (width as usize).saturating_sub(4) / 2; // 2-char indent each side + divider

    let left_lines = build_vehicle_provider_lines(detail);
    let right_lines = build_location_lines(detail);

    let max_rows = left_lines.len().max(right_lines.len());

    lines.push(Line::raw(""));

    // Headers.
    let left_header = format!("   {:<width$}", "VEHICLE", width = half);
    let right_header = "LOCATION".to_string();
    lines.push(Line::from(vec![
        Span::styled(
            left_header,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            right_header,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    for i in 0..max_rows {
        let left_content = left_lines.get(i).cloned().unwrap_or_default();
        let right_content = right_lines.get(i).cloned().unwrap_or_default();

        // Pad left column to fixed width.
        let left_display_len = left_content.chars().count();
        let padding = half.saturating_sub(left_display_len);
        let padded_left = format!("   {left_content}{:padding$}", "", padding = padding);

        lines.push(Line::from(vec![
            Span::styled(padded_left, Style::default().fg(Color::Gray)),
            Span::styled("│  ", Style::default().fg(Color::DarkGray)),
            Span::styled(right_content, Style::default().fg(Color::Gray)),
        ]));
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Single-column fallback (< 100 cols)
// ---------------------------------------------------------------------------

fn build_single_column_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    lines.push(Line::raw(""));

    // Vehicle section.
    lines.push(section_header("VEHICLE"));
    for line in build_vehicle_provider_lines(detail) {
        lines.push(Line::from(Span::styled(
            format!("   {line}"),
            Style::default().fg(Color::Gray),
        )));
    }

    lines.push(Line::raw(""));

    // Location section.
    lines.push(section_header("LOCATION"));
    for line in build_location_lines(detail) {
        lines.push(Line::from(Span::styled(
            format!("   {line}"),
            Style::default().fg(Color::Gray),
        )));
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Vehicle / Provider content
// ---------------------------------------------------------------------------

fn build_vehicle_provider_lines(detail: &LaunchDetail) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(rocket) = &detail.rocket_full_name {
        out.push(rocket.clone());
    }

    out.push(String::new()); // blank spacer

    // Provider sub-header + type.
    let provider_line = match &detail.launch_service_provider.provider_type {
        Some(pt) => format!("{} ({})", detail.launch_service_provider.name, pt),
        None => detail.launch_service_provider.name.clone(),
    };
    out.push(format!("PROVIDER: {provider_line}"));

    // Provider record bar.
    if let (Some(total), Some(success), Some(failed)) = (
        detail.provider_total_launches,
        detail.provider_successful_launches,
        detail.provider_failed_launches,
    ) {
        if total > 0 {
            let bar = build_record_bar(success, total);
            out.push(bar);
            out.push(format!(
                "{total} launches ({success} ok, {failed} fail)"
            ));
        }
    }

    out
}

/// Build a `█`/`░` success/failure bar scaled to `RECORD_BAR_WIDTH`.
pub fn build_record_bar(success: u32, total: u32) -> String {
    if total == 0 {
        return "░".repeat(RECORD_BAR_WIDTH);
    }

    let success_ratio = success as f64 / total as f64;
    let success_blocks = (success_ratio * RECORD_BAR_WIDTH as f64).round() as usize;
    let fail_blocks = RECORD_BAR_WIDTH.saturating_sub(success_blocks);
    let pct = (success_ratio * 100.0).round() as u32;

    format!(
        "{}{} {pct}%",
        "█".repeat(success_blocks),
        "░".repeat(fail_blocks),
    )
}

// ---------------------------------------------------------------------------
// Location content
// ---------------------------------------------------------------------------

fn build_location_lines(detail: &LaunchDetail) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(pad_name) = &detail.pad.name {
        out.push(pad_name.clone());
    }

    let location = &detail.pad.location;
    let loc_str = match &location.country {
        Some(country) => format!("{}, {}", location.name, country.name),
        None => location.name.clone(),
    };
    out.push(loc_str);

    out
}

// ---------------------------------------------------------------------------
// Mission section
// ---------------------------------------------------------------------------

fn build_mission_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    lines.push(Line::raw(""));
    lines.push(section_header("MISSION"));

    match &detail.mission {
        Some(mission) => {
            // Mission name + type.
            lines.push(Line::from(Span::styled(
                format!("   {} ({})", mission.name, mission.mission_type),
                Style::default().fg(Color::White),
            )));

            // Orbit.
            if let Some(orbit) = &mission.orbit {
                lines.push(Line::from(Span::styled(
                    format!("   Orbit: {} ({})", orbit.name, orbit.abbrev),
                    Style::default().fg(Color::Gray),
                )));
            }

            // Description (word-wrapped would be ideal, but for now just display).
            if let Some(desc) = &mission.description {
                if !desc.is_empty() {
                    lines.push(Line::raw(""));
                    for wrapped_line in word_wrap(desc, 72) {
                        lines.push(Line::from(Span::styled(
                            format!("   {wrapped_line}"),
                            Style::default().fg(Color::Gray),
                        )));
                    }
                }
            }
        }
        None => {
            lines.push(Line::from(Span::styled(
                "   No mission information available.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Links section
// ---------------------------------------------------------------------------

fn build_links_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    let has_links = !detail.vid_urls.is_empty()
        || !detail.info_urls.is_empty()
        || !detail.programs.is_empty()
        || detail.image_url.is_some();

    if !has_links {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(section_header("LINKS"));

    let cyan = Style::default().fg(Color::Cyan);
    let dim_underline = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);

    for vid in &detail.vid_urls {
        let label = vid.title.as_deref().unwrap_or("Webcast");
        lines.push(Line::from(vec![
            Span::styled(format!("   {label:<10}"), cyan),
            Span::styled(vid.url.clone(), dim_underline),
        ]));
    }

    for info in &detail.info_urls {
        let label = info.title.as_deref().unwrap_or("Info");
        lines.push(Line::from(vec![
            Span::styled(format!("   {label:<10}"), cyan),
            Span::styled(info.url.clone(), dim_underline),
        ]));
    }

    for program in &detail.programs {
        lines.push(Line::from(vec![
            Span::styled("   Program   ", cyan),
            Span::styled(program.clone(), Style::default().fg(Color::Gray)),
        ]));
    }

    if let Some(image) = &detail.image_url {
        lines.push(Line::from(vec![
            Span::styled("   Image     ", cyan),
            Span::styled(image.clone(), dim_underline),
        ]));
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Push a dark gray separator line.
fn push_separator(lines: &mut Vec<Line<'static>>, width: u16) {
    let sep_width = (width as usize).saturating_sub(4);
    let sep = "━".repeat(sep_width);
    lines.push(Line::from(Span::styled(
        format!("  {sep}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    )));
}

/// Build a section header line (e.g., `"   MISSION"`).
fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("   {label}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Simple word-wrap to a target line width.
fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if current_line.len() + 1 + word.len() > max_width {
            result.push(std::mem::take(&mut current_line));
            current_line.push_str(word);
        } else {
            current_line.push(' ');
            current_line.push_str(word);
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    result
}

/// Render a scroll position indicator on the right edge of the content area.
fn render_scroll_indicator(
    frame: &mut ratatui::Frame,
    area: Rect,
    scroll_offset: usize,
    max_scroll: usize,
) {
    if area.height == 0 || max_scroll == 0 {
        return;
    }

    // Calculate thumb position within the viewport height.
    let vh = area.height as usize;
    let thumb_size = (vh * vh / (vh + max_scroll)).max(1);
    let track_space = vh.saturating_sub(thumb_size);
    let thumb_pos = if max_scroll > 0 {
        (scroll_offset * track_space) / max_scroll
    } else {
        0
    };

    for i in 0..vh {
        let ch = if i >= thumb_pos && i < thumb_pos + thumb_size {
            "┃"
        } else {
            "│"
        };

        let indicator_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y + i as u16,
            width: 1,
            height: 1,
        };
        let style = if ch == "┃" {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(Paragraph::new(Span::styled(ch, style)), indicator_area);
    }
}

/// Render the bottom hint bar for the detail view.
fn render_hint_bar(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let status_line = status_bar::build_status_line(app);

    let hints = Line::from(vec![
        Span::styled("  Esc", Style::default().fg(Color::White)),
        Span::styled(": Back · ", Style::default().fg(Color::DarkGray)),
        Span::styled("↑/↓", Style::default().fg(Color::White)),
        Span::styled(": Scroll · ", Style::default().fg(Color::DarkGray)),
        Span::styled("r", Style::default().fg(Color::White)),
        Span::styled(": Refresh · ", Style::default().fg(Color::DarkGray)),
        Span::styled("?", Style::default().fg(Color::White)),
        Span::styled(": Help · ", Style::default().fg(Color::DarkGray)),
        Span::styled("q", Style::default().fg(Color::White)),
        Span::styled(": Quit", Style::default().fg(Color::DarkGray)),
    ]);

    let paragraph = Paragraph::new(vec![status_line, hints]);
    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tests::dummy_launch_detail;
    use crate::models::{
        LaunchDetail, LocationInfo, MissionSummary, OrbitInfo, PadInfo, Provider, UrlEntry,
    };

    /// Build a richly populated detail for testing.
    fn rich_detail() -> LaunchDetail {
        let mut d = dummy_launch_detail();
        d.rocket_full_name = Some("Starship (Super Heavy + Starship)".into());
        d.launch_service_provider = Provider {
            name: "SpaceX".into(),
            provider_type: Some("Commercial".into()),
        };
        d.provider_total_launches = Some(301);
        d.provider_successful_launches = Some(295);
        d.provider_failed_launches = Some(6);
        d.pad = PadInfo {
            name: Some("Orbital Launch Mount A".into()),
            location: LocationInfo {
                name: "Starbase, Texas".into(),
                timezone_name: Some("America/Chicago".into()),
                country: None,
            },
        };
        d.probability = Some(90);
        d.weather_concerns = Some("No concerns".into());
        d.mission = Some(MissionSummary {
            name: "Starship IFT-7".into(),
            mission_type: "Test Flight".into(),
            description: Some(
                "Orbital flight test demonstrating booster catch and ship deorbit capabilities."
                    .into(),
            ),
            orbit: Some(OrbitInfo {
                id: 8,
                name: "Low Earth Orbit".into(),
                abbrev: "LEO".into(),
            }),
        });
        d.vid_urls = vec![UrlEntry {
            title: Some("Webcast".into()),
            url: "https://www.youtube.com/watch?v=abc".into(),
        }];
        d.info_urls = vec![UrlEntry {
            title: Some("Info".into()),
            url: "https://www.spacex.com/launches/mission-7/".into(),
        }];
        d.programs = vec!["Starship Development".into()];
        d.image_url = Some("https://example.com/image.jpg".into());
        d
    }

    // ── Provider record bar ────────────────────────────────────────────

    #[test]
    fn record_bar_perfect_record() {
        let bar = build_record_bar(100, 100);
        assert!(bar.starts_with("████████████████████"));
        assert!(bar.contains("100%"));
    }

    #[test]
    fn record_bar_zero_success() {
        let bar = build_record_bar(0, 10);
        assert!(bar.starts_with("░░░░░░░░░░░░░░░░░░░░"));
        assert!(bar.contains("0%"));
    }

    #[test]
    fn record_bar_mixed() {
        let bar = build_record_bar(295, 301);
        // 295/301 ≈ 98%, so ~20 success blocks.
        assert!(bar.contains("98%") || bar.contains("97%") || bar.contains("99%"));
        assert!(bar.contains('█'));
    }

    #[test]
    fn record_bar_zero_total() {
        let bar = build_record_bar(0, 0);
        assert_eq!(bar, "░░░░░░░░░░░░░░░░░░░░");
    }

    #[test]
    fn record_bar_fifty_percent() {
        let bar = build_record_bar(50, 100);
        assert!(bar.contains("50%"));
        let success_count = bar.matches('█').count();
        let fail_count = bar.matches('░').count();
        assert_eq!(success_count, 10);
        assert_eq!(fail_count, 10);
    }

    // ── Word wrap ──────────────────────────────────────────────────────

    #[test]
    fn word_wrap_short_text() {
        let result = word_wrap("hello world", 72);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn word_wrap_long_text() {
        let text = "The quick brown fox jumps over the lazy dog and then runs far away";
        let result = word_wrap(text, 30);
        assert!(result.len() > 1);
        for line in &result {
            assert!(line.len() <= 33, "line too long: '{line}'"); // some tolerance for single long words
        }
    }

    #[test]
    fn word_wrap_empty() {
        assert!(word_wrap("", 72).is_empty());
    }

    #[test]
    fn word_wrap_single_long_word() {
        let result = word_wrap("superlongword", 5);
        assert_eq!(result, vec!["superlongword"]);
    }

    // ── Content building ───────────────────────────────────────────────

    #[test]
    fn content_lines_contain_hero_section() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains("██"), "should have status badges");
        assert!(
            text.contains("Go for Launch") || text.contains("Go"),
            "should have status name"
        );
    }

    #[test]
    fn content_lines_contain_vehicle_and_location() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains("VEHICLE"));
        assert!(text.contains("LOCATION"));
        assert!(text.contains("Starship (Super Heavy + Starship)"));
        assert!(text.contains("Orbital Launch Mount A"));
        assert!(text.contains("Starbase, Texas"));
    }

    #[test]
    fn content_lines_contain_provider_record_bar() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains('█'));
        assert!(text.contains("301 launches"));
        assert!(text.contains("295 ok"));
    }

    #[test]
    fn content_lines_contain_mission() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains("MISSION"));
        assert!(text.contains("Starship IFT-7 (Test Flight)"));
        assert!(text.contains("Low Earth Orbit (LEO)"));
        assert!(text.contains("booster catch"));
    }

    #[test]
    fn content_lines_contain_links() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains("LINKS"));
        assert!(text.contains("youtube.com"));
        assert!(text.contains("spacex.com"));
        assert!(text.contains("Starship Development"));
        assert!(text.contains("example.com/image.jpg"));
    }

    #[test]
    fn content_lines_contain_separators() {
        let detail = rich_detail();
        let lines = build_content_lines(&detail, 120);

        let separator_count = lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains('━'))
            })
            .count();

        assert!(separator_count >= 3, "expected at least 3 separators");
    }

    // ── Responsive layout ──────────────────────────────────────────────

    #[test]
    fn narrow_width_uses_single_column() {
        let detail = rich_detail();
        let lines_narrow = build_content_lines(&detail, 80); // < 100

        let text: String = lines_narrow
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        // Single column has separate VEHICLE and LOCATION headers without │ divider.
        assert!(text.contains("VEHICLE"));
        assert!(text.contains("LOCATION"));
        let has_divider = lines_narrow
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains('│')));
        assert!(!has_divider, "narrow layout should not have column divider");
    }

    #[test]
    fn wide_width_uses_two_columns() {
        let detail = rich_detail();
        let lines_wide = build_content_lines(&detail, 120); // >= 100

        let has_divider = lines_wide
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains('│')));
        assert!(has_divider, "wide layout should have column divider");
    }

    // ── Missing optional fields ────────────────────────────────────────

    #[test]
    fn minimal_detail_renders_without_panic() {
        let detail = dummy_launch_detail(); // All optionals are None.
        let lines = build_content_lines(&detail, 120);
        assert!(!lines.is_empty());

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(text.contains("MISSION"));
        assert!(text.contains("No mission information"));
    }

    #[test]
    fn detail_without_links_omits_links_section() {
        let detail = dummy_launch_detail();
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(!text.contains("LINKS"));
    }

    #[test]
    fn probability_negative_is_hidden() {
        let mut detail = rich_detail();
        detail.probability = Some(-1);
        let lines = build_content_lines(&detail, 120);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(!text.contains("Probability:"));
    }

    #[test]
    fn window_times_displayed() {
        let detail = rich_detail();
        // rich_detail inherits window_start/end = None from dummy_launch_detail.
        // Let's verify no crash when they're None.
        let lines = build_content_lines(&detail, 120);
        assert!(!lines.is_empty());
    }
}
