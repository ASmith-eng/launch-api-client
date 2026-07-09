//! Detail view renderer for a single launch.
//!
//! Renders the full launch detail screen with hero header, two-column
//! vehicle/provider + location grid, mission section, and links.
//! Supports vertical scrolling and responsive single-column fallback
//! below 100 terminal columns.

use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::models::LaunchDetail;
use crate::tui::app::App;
use crate::tui::style;
use crate::tui::time_fmt;
use crate::tui::views::status_bar;
use crate::tui::views::{rate_limit_title, TitleBar};
use crate::vendor::launch_library_2::status_map::{status_style, unknown_status_style};

/// Width threshold below which we switch from two-column to single-column.
const TWO_COL_MIN_WIDTH: u16 = 100;

/// Fixed width for the provider success/failure bar.
const RECORD_BAR_WIDTH: usize = 20;

/// Column widths for the crew table (name, then role; agency fills the rest).
const CREW_NAME_WIDTH: usize = 20;
const CREW_ROLE_WIDTH: usize = 18;

/// Column widths for the landing table (stage, then landing type; location fills the rest).
const LANDING_STAGE_WIDTH: usize = 13;
const LANDING_TYPE_WIDTH: usize = 7;

/// Render the full detail view into the given area.
///
/// If the launch ID is found in `app.detail_cache`, renders the full
/// detail. Otherwise renders a loading/not-found placeholder.
pub fn render_detail(frame: &mut ratatui::Frame, area: Rect, launch_id: &str, app: &App) {
    let block = build_title_block(launch_id, app);
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
            let p = Paragraph::new(msg).style(style::label());
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

fn build_title_block(launch_id: &str, app: &App) -> ratatui::widgets::Block<'static> {
    let name = app
        .detail_cache
        .get(launch_id)
        .map(|c| c.data.name.clone())
        .unwrap_or_else(|| launch_id.to_string());

    let mut bar = TitleBar::new(Line::raw(format!(" {name} ")));

    if let Some(rl) = rate_limit_title(app.rate_limit_remaining, app.rate_limit_total) {
        bar = bar.right(rl);
    }

    bar.build()
}

// ---------------------------------------------------------------------------
// Content builder
// ---------------------------------------------------------------------------

/// Build all content lines for the detail view.
fn build_content_lines(detail: &LaunchDetail, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(64);

    // Hero header.
    build_hero_section(&mut lines, detail, width);

    // Separator.
    push_separator(&mut lines, width);

    // Vehicle/Provider + Location (two-column or single-column).
    if width >= TWO_COL_MIN_WIDTH {
        build_two_column_section(&mut lines, detail, width);
    } else {
        build_single_column_section(&mut lines, detail, width);
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

fn build_hero_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail, width: u16) {
    // Extra vertical padding before hero content.
    lines.push(Line::raw(""));
    lines.push(Line::raw(""));

    // Status-colored ██ badges + countdown + status name.
    let now = Utc::now();
    let has_time = time_fmt::has_precise_time(detail.net_precision.as_ref());

    let (badge_style, status_name) = match status_style(detail.status.id) {
        Some(ss) => (ss.style, ss.name.to_string()),
        None => (unknown_status_style(), detail.status.name.clone()),
    };

    let mut hero_spans: Vec<Span<'static>> = vec![Span::styled("██  ", badge_style)];

    if has_time {
        let countdown = time_fmt::format_countdown(now, detail.net);
        let cd_style = time_fmt::countdown_style(now, detail.net);
        hero_spans.push(Span::styled(countdown, cd_style));
    }

    hero_spans.push(Span::styled("  ██", badge_style));
    hero_spans.push(Span::raw("     "));
    hero_spans.push(Span::styled(status_name, style::primary()));

    lines.push(Line::from(hero_spans).centered());

    // NET time display (dual timezone).
    let time_str = time_fmt::format_net_time(
        &detail.net,
        detail.net_precision.as_ref(),
        &detail.pad.location.timezone_name,
    );
    lines.push(Line::from(Span::styled(time_str, style::primary())).centered());

    // Window + probability line (if any data present).
    let has_window = detail.window_start.is_some() && detail.window_end.is_some();
    let has_probability = detail.probability.is_some();

    if has_window || has_probability {
        let mut spans: Vec<Span<'static>> = Vec::new();

        if let (Some(start), Some(end)) = (&detail.window_start, &detail.window_end) {
            let start_str = start.format("%H:%M").to_string();
            let end_str = end.format("%H:%M").to_string();
            spans.push(Span::styled(
                format!("Window: {start_str} - {end_str} UTC"),
                style::label(),
            ));
        }

        if has_window && has_probability {
            spans.push(Span::styled("  ·  ", style::label()));
        }

        if let Some(prob) = detail.probability {
            spans.push(Span::styled("Probability: ", style::label()));
            spans.push(Span::styled(
                format!("{prob}"),
                style::probability_style(Some(prob)),
            ));
        }

        lines.push(Line::from(spans).centered());
    }

    // Weather concerns — wrap to fit within the view width.
    if let Some(weather) = &detail.weather_concerns {
        if !weather.is_empty() {
            let prefix = "Weather: ";
            let wrap_width = (width as usize).saturating_sub(prefix.len() + 4);
            let wrapped = word_wrap(weather, wrap_width);
            for (i, line) in wrapped.into_iter().enumerate() {
                if i == 0 {
                    lines.push(
                        Line::from(vec![
                            Span::styled(prefix, style::label()),
                            Span::styled(line, style::secondary()),
                        ])
                        .centered(),
                    );
                } else {
                    lines.push(
                        Line::from(Span::styled(line, style::secondary())).centered(),
                    );
                }
            }
        }
    }

    // Fail reason — shown below weather on a failed launch, wrapped to width
    // and styled as a warning. (Conversion normalises "" to None already;
    // the emptiness guard mirrors the weather block for safety.)
    if let Some(reason) = &detail.failreason {
        if !reason.is_empty() {
            let prefix = "Failure: ";
            let wrap_width = (width as usize).saturating_sub(prefix.len() + 4);
            let wrapped = word_wrap(reason, wrap_width);
            for (i, line) in wrapped.into_iter().enumerate() {
                if i == 0 {
                    lines.push(
                        Line::from(vec![
                            Span::styled(prefix, style::label()),
                            Span::styled(line, style::warning()),
                        ])
                        .centered(),
                    );
                } else {
                    lines.push(
                        Line::from(Span::styled(line, style::warning())).centered(),
                    );
                }
            }
        }
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Updates section (conditional — reverse-chronological status timeline)
// ---------------------------------------------------------------------------

/// Build the conditional UPDATES section: up to the 5 most recent updates,
/// each shown as a Tier 3 timestamp label followed by its Tier 2 comment.
///
/// Renders nothing when `detail.updates` is empty. Updates arrive
/// reverse-chronological from the API, so the first 5 are the most recent.
#[allow(dead_code)]
fn build_updates_section(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail, width: u16) {
    if detail.updates.is_empty() {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(section_header("UPDATES"));
    lines.push(Line::raw(""));

    // 3-char indent + 3-char right margin, matching the other sections.
    let wrap_width = (width as usize).saturating_sub(6);

    for update in detail.updates.iter().take(5) {
        // Timestamp — Tier 3 label, e.g. "Mar 21, 02:31 UTC".
        let timestamp = update.created_on.format("%b %d, %H:%M UTC").to_string();
        lines.push(Line::from(Span::styled(
            format!("   {timestamp}"),
            style::label(),
        )));

        // Comment — Tier 2 secondary, word-wrapped to the available width.
        for wrapped in word_wrap(&update.comment, wrap_width) {
            lines.push(Line::from(Span::styled(
                format!("   {wrapped}"),
                style::secondary(),
            )));
        }

        // Blank line between entries.
        lines.push(Line::raw(""));
    }
}

// ---------------------------------------------------------------------------
// Two-column layout (vehicle/provider | location)
// ---------------------------------------------------------------------------

fn build_two_column_section(
    lines: &mut Vec<Line<'static>>,
    detail: &LaunchDetail,
    width: u16,
) {
    let half = (width as usize).saturating_sub(7) / 2; // 3-char indent + 3-char divider + margin

    let vehicle_lines = build_vehicle_lines(detail, half);
    let location_lines = build_location_lines(detail, half);

    // Two-column grid: vehicle (left) │ location (right).
    let grid_rows = vehicle_lines.len().max(location_lines.len());

    lines.push(Line::raw(""));

    // Headers.
    let left_header = format!("   {:<width$}", "VEHICLE", width = half);
    let right_header = "LOCATION".to_string();
    lines.push(Line::from(vec![
        Span::styled(left_header, style::section_heading()),
        Span::styled("│  ", style::label()),
        Span::styled(right_header, style::section_heading()),
    ]));

    for i in 0..grid_rows {
        let left_line = vehicle_lines.get(i).cloned().unwrap_or_default();
        let right_line = location_lines.get(i).cloned().unwrap_or_default();

        let left_text_len: usize = left_line.spans.iter().map(|s| s.content.len()).sum();
        let padding = half.saturating_sub(left_text_len);

        let mut spans = Vec::with_capacity(left_line.spans.len() + right_line.spans.len() + 3);
        spans.push(Span::raw("   "));
        spans.extend(left_line.spans);
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled("│  ", style::label()));
        spans.extend(right_line.spans);

        lines.push(Line::from(spans));
    }

    // Provider section — full-width below the grid, no divider.
    let provider_width = (width as usize).saturating_sub(6);
    let provider_lines = build_provider_lines(detail, provider_width);
    if !provider_lines.is_empty() {
        lines.push(Line::raw(""));
        for line in provider_lines {
            lines.push(indent_line(line));
        }
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Single-column fallback (< 100 cols)
// ---------------------------------------------------------------------------

fn build_single_column_section(
    lines: &mut Vec<Line<'static>>,
    detail: &LaunchDetail,
    width: u16,
) {
    let col_width = (width as usize).saturating_sub(6); // 3-char indent + margin

    lines.push(Line::raw(""));

    // Vehicle section.
    lines.push(section_header("VEHICLE"));
    for line in build_vehicle_lines(detail, col_width) {
        lines.push(indent_line(line));
    }

    // Provider section.
    let provider_lines = build_provider_lines(detail, col_width);
    if !provider_lines.is_empty() {
        lines.push(Line::raw(""));
        for line in provider_lines {
            lines.push(indent_line(line));
        }
    }

    lines.push(Line::raw(""));

    // Location section.
    lines.push(section_header("LOCATION"));
    for line in build_location_lines(detail, col_width) {
        lines.push(indent_line(line));
    }

    lines.push(Line::raw(""));
}

// ---------------------------------------------------------------------------
// Vehicle content (rocket name only — used in the two-column grid)
// ---------------------------------------------------------------------------

fn build_vehicle_lines(detail: &LaunchDetail, max_width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

    if let Some(rocket) = &detail.rocket_full_name {
        for wrapped in word_wrap(rocket, max_width) {
            out.push(Line::from(Span::styled(wrapped, style::secondary())));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Provider content (name, record bar — rendered full-width below the grid)
// ---------------------------------------------------------------------------

fn build_provider_lines(detail: &LaunchDetail, max_width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

    // Provider heading + name underneath.
    out.push(Line::from(Span::styled("PROVIDER", style::section_heading())));
    let provider_text = match &detail.launch_service_provider.provider_type {
        Some(pt) => format!("{} ({})", detail.launch_service_provider.name, pt),
        None => detail.launch_service_provider.name.clone(),
    };
    for wrapped in word_wrap(&provider_text, max_width) {
        out.push(Line::from(Span::styled(wrapped, style::secondary())));
    }

    // Provider record bar.
    if let (Some(total), Some(success), Some(failed)) = (
        detail.provider_total_launches,
        detail.provider_successful_launches,
        detail.provider_failed_launches,
    ) {
        if total > 0 {
            out.push(build_record_bar_line(success, total));
            out.push(Line::from(Span::styled(
                format!("{total} launches ({success} ok, {failed} fail)"),
                style::label(),
            )));
        }
    }

    out
}

/// Build a coloured `█`/`░` record bar as a [`Line`].
///
/// Success blocks are green, failure blocks are red, percentage is Tier 3.
pub fn build_record_bar_line(success: u32, total: u32) -> Line<'static> {
    if total == 0 {
        return Line::from(Span::styled(
            "░".repeat(RECORD_BAR_WIDTH),
            style::record_failure(),
        ));
    }

    let success_ratio = success as f64 / total as f64;
    let success_blocks = (success_ratio * RECORD_BAR_WIDTH as f64).round() as usize;
    let fail_blocks = RECORD_BAR_WIDTH.saturating_sub(success_blocks);
    let pct = (success_ratio * 100.0).round() as u32;

    Line::from(vec![
        Span::styled("█".repeat(success_blocks), style::record_success()),
        Span::styled("░".repeat(fail_blocks), style::record_failure()),
        Span::styled(format!(" {pct}%"), style::label()),
    ])
}


// ---------------------------------------------------------------------------
// Location content
// ---------------------------------------------------------------------------

fn build_location_lines(detail: &LaunchDetail, max_width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();

    if let Some(pad_name) = &detail.pad.name {
        for wrapped in word_wrap(pad_name, max_width) {
            out.push(Line::from(Span::styled(wrapped, style::secondary())));
        }
    }

    let location = &detail.pad.location;
    let loc_str = match &location.country {
        Some(country) => format!("{}, {}", location.name, country.name),
        None => location.name.clone(),
    };
    for wrapped in word_wrap(&loc_str, max_width) {
        out.push(Line::from(Span::styled(wrapped, style::secondary())));
    }

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
            // Mission name + type (Tier 1 — primary scanned value).
            lines.push(Line::from(Span::styled(
                format!("   {} ({})", mission.name, mission.mission_type),
                style::primary(),
            )));

            // Orbit (label is Tier 3, value is Tier 2).
            if let Some(orbit) = &mission.orbit {
                lines.push(Line::from(vec![
                    Span::styled("   Orbit: ", style::label()),
                    Span::styled(
                        format!("{} ({})", orbit.name, orbit.abbrev),
                        style::secondary(),
                    ),
                ]));
            }

            // Description (Tier 2 — supporting context).
            if let Some(desc) = &mission.description {
                if !desc.is_empty() {
                    lines.push(Line::raw(""));
                    for wrapped_line in word_wrap(desc, 72) {
                        lines.push(Line::from(Span::styled(
                            format!("   {wrapped_line}"),
                            style::secondary(),
                        )));
                    }
                }
            }
        }
        None => {
            lines.push(Line::from(Span::styled(
                "   No mission information available.",
                style::label(),
            )));
        }
    }

    // Conditional sub-sections: only present on crewed / recoverable flights.
    build_crew_subsection(lines, detail);
    build_landing_subsection(lines, detail);

    lines.push(Line::raw(""));
}

/// Append the CREW sub-section when the launch has assigned crew.
///
/// Renders a three-column table (name, role, agency) with fixed column
/// widths. Names longer than the column are not truncated — they push the
/// following columns right rather than being clipped. Does nothing for
/// uncrewed flights.
fn build_crew_subsection(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    if detail.crew.is_empty() {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(section_header("CREW"));

    for member in &detail.crew {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!("{:<w$}", member.name, w = CREW_NAME_WIDTH),
                style::secondary(),
            ),
            Span::styled(
                format!("{:<w$}", member.role, w = CREW_ROLE_WIDTH),
                style::label(),
            ),
            Span::styled(member.agency.clone(), style::label()),
        ]));
    }
}

/// Append the LANDING sub-section when at least one stage attempts a landing.
///
/// One row per attempted landing: stage type, landing type abbreviation, and
/// landing location. Any of these may be `None` (the API often omits the type
/// or location before a landing is finalised); missing values are left blank
/// rather than shown as a placeholder. Stages with no landing attempt are
/// skipped entirely.
fn build_landing_subsection(lines: &mut Vec<Line<'static>>, detail: &LaunchDetail) {
    if !detail.landings.iter().any(|l| l.landing_attempt) {
        return;
    }

    lines.push(Line::raw(""));
    lines.push(section_header("LANDING"));

    for landing in detail.landings.iter().filter(|l| l.landing_attempt) {
        let stage = landing.stage_type.as_deref().unwrap_or("");
        let landing_type = landing.landing_type.as_deref().unwrap_or("");

        let mut spans = vec![
            Span::raw("   "),
            Span::styled(
                format!("{:<w$}", stage, w = LANDING_STAGE_WIDTH),
                style::secondary(),
            ),
            Span::styled(
                format!("{:<w$}", landing_type, w = LANDING_TYPE_WIDTH),
                style::label(),
            ),
        ];

        if let Some(location) = &landing.landing_location {
            spans.push(Span::styled(location.clone(), style::secondary()));
        }

        lines.push(Line::from(spans));
    }
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

    for vid in &detail.vid_urls {
        let label = vid.title.as_deref().unwrap_or("Webcast");
        lines.push(Line::from(vec![
            Span::styled(format!("   {label:<10}"), style::link_label()),
            Span::styled(vid.url.clone(), style::link_url()),
        ]));
    }

    for info in &detail.info_urls {
        let label = info.title.as_deref().unwrap_or("Info");
        lines.push(Line::from(vec![
            Span::styled(format!("   {label:<10}"), style::link_label()),
            Span::styled(info.url.clone(), style::link_url()),
        ]));
    }

    for program in &detail.programs {
        lines.push(Line::from(vec![
            Span::styled("   Program   ", style::link_label()),
            Span::styled(program.clone(), style::secondary()),
        ]));
    }

    if let Some(image) = &detail.image_url {
        lines.push(Line::from(vec![
            Span::styled("   Image     ", style::link_label()),
            Span::styled(image.clone(), style::link_url()),
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
        style::separator(),
    )));
}

/// Build a section header line (e.g., `"   MISSION"`).
fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("   {label}"),
        style::section_heading(),
    ))
}

/// Prepend a 3-space indent to a line, preserving its spans.
fn indent_line(line: Line<'static>) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    spans.push(Span::raw("   "));
    spans.extend(line.spans);
    Line::from(spans)
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

    let key = style::secondary();
    let desc = style::label();

    // Left group: contextual actions.
    let left_hints = Line::from(vec![
        Span::styled("  Esc", key),
        Span::styled(": Back · ", desc),
        Span::styled("↑/↓", key),
        Span::styled(": Scroll · ", desc),
        Span::styled("r", key),
        Span::styled(": Refresh", desc),
    ]);

    // Right group: meta actions (help, quit).
    let right_hints = Line::from(vec![
        Span::styled("?", key),
        Span::styled(": Help · ", desc),
        Span::styled("q", key),
        Span::styled(": Quit  ", desc),
    ]);

    // Row 0: status line, Row 1: hints (left + right aligned).
    let status_area = Rect { height: 1, ..area };
    let hints_area = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    frame.render_widget(Paragraph::new(status_line), status_area);
    frame.render_widget(Paragraph::new(left_hints), hints_area);
    frame.render_widget(
        Paragraph::new(right_hints).alignment(ratatui::layout::Alignment::Right),
        hints_area,
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheStrategy;
    use crate::models::tests::dummy_launch_detail;
    use crate::models::{
        CrewMember, LaunchDetail, LaunchUpdate, LocationInfo, MissionSummary, OrbitInfo, PadInfo,
        Probability, Provider, StageLanding, UrlEntry,
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
        d.probability = Probability::new(90);
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

    /// Extract concatenated text from a Line's spans.
    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn record_bar_perfect_record() {
        let bar = build_record_bar_line(100, 100);
        let text = line_text(&bar);
        assert!(text.contains("████████████████████"));
        assert!(text.contains("100%"));
    }

    #[test]
    fn record_bar_zero_success() {
        let bar = build_record_bar_line(0, 10);
        let text = line_text(&bar);
        assert!(text.contains("░░░░░░░░░░░░░░░░░░░░"));
        assert!(text.contains("0%"));
    }

    #[test]
    fn record_bar_mixed() {
        let bar = build_record_bar_line(295, 301);
        let text = line_text(&bar);
        assert!(text.contains("98%") || text.contains("97%") || text.contains("99%"));
        assert!(text.contains('█'));
    }

    #[test]
    fn record_bar_zero_total() {
        let bar = build_record_bar_line(0, 0);
        let text = line_text(&bar);
        assert!(text.contains("░░░░░░░░░░░░░░░░░░░░"));
    }

    #[test]
    fn record_bar_fifty_percent() {
        let bar = build_record_bar_line(50, 100);
        let text = line_text(&bar);
        assert!(text.contains("50%"));
        let success_count = text.matches('█').count();
        let fail_count = text.matches('░').count();
        assert_eq!(success_count, 10);
        assert_eq!(fail_count, 10);
    }

    #[test]
    fn record_bar_success_is_green() {
        let bar = build_record_bar_line(10, 10);
        // First span should be the success blocks, coloured green.
        let first = &bar.spans[0];
        assert!(first.content.contains('█'));
        assert_eq!(first.style.fg, Some(Color::Green));
    }

    #[test]
    fn record_bar_failure_is_red() {
        let bar = build_record_bar_line(0, 10);
        // With 0 success, the failure span should be first styled span with ░.
        let fail_span = bar.spans.iter().find(|s| s.content.contains('░')).unwrap();
        assert_eq!(fail_span.style.fg, Some(Color::Red));
    }

    // ── Hero fail reason ──────────────────────────────────────────────

    #[test]
    fn hero_shows_fail_reason_when_present() {
        let mut detail = dummy_launch_detail();
        detail.failreason = Some("Second stage engine failed to ignite.".into());
        let mut lines = Vec::new();
        build_hero_section(&mut lines, &detail, 100);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Failure: "));
        assert!(text.contains("Second stage engine failed to ignite."));

        // Reason text carries the warning (red) colour.
        let has_red = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("Second stage") && s.style.fg == Some(Color::Red))
        });
        assert!(has_red, "fail reason should use the warning colour");
    }

    #[test]
    fn hero_omits_fail_reason_when_absent() {
        let mut detail = dummy_launch_detail();
        detail.failreason = None;
        let mut lines = Vec::new();
        build_hero_section(&mut lines, &detail, 100);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(!text.contains("Failure:"));
    }

    #[test]
    fn hero_wraps_long_fail_reason() {
        let mut detail = dummy_launch_detail();
        let reason = "The second stage engine failed to ignite after stage separation \
                      due to a liquid oxygen feedline pressure anomaly detected shortly \
                      before the scheduled relight sequence.";
        detail.failreason = Some(reason.into());
        let mut lines = Vec::new();
        build_hero_section(&mut lines, &detail, 60);

        let start = lines
            .iter()
            .position(|l| line_text(l).contains("The second stage"))
            .expect("first fragment present");
        let end = lines
            .iter()
            .position(|l| line_text(l).contains("relight sequence"))
            .expect("last fragment present");
        assert!(end > start, "long fail reason should wrap onto multiple lines");
    }

    // ── Updates section ───────────────────────────────────────────────

    /// Build a `LaunchUpdate` at a fixed 2026 date for timestamp assertions.
    fn make_update(month: u32, day: u32, hour: u32, minute: u32, comment: &str) -> LaunchUpdate {
        use chrono::TimeZone;
        LaunchUpdate {
            comment: comment.into(),
            created_on: Utc.with_ymd_and_hms(2026, month, day, hour, minute, 0).unwrap(),
            info_url: None,
        }
    }

    #[test]
    fn updates_empty_produces_no_lines() {
        let detail = dummy_launch_detail();
        let mut lines = Vec::new();
        build_updates_section(&mut lines, &detail, 100);
        assert!(lines.is_empty(), "empty updates should render nothing");
    }

    #[test]
    fn updates_three_render_with_timestamp_and_comment() {
        let mut detail = dummy_launch_detail();
        detail.updates = vec![
            make_update(3, 21, 2, 31, "Delayed to March 25."),
            make_update(3, 20, 19, 8, "Launch pad assigned."),
            make_update(3, 19, 10, 41, "Added daily launch window."),
        ];
        let mut lines = Vec::new();
        build_updates_section(&mut lines, &detail, 100);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("UPDATES"));
        assert!(text.contains("Mar 21, 02:31 UTC"));
        assert!(text.contains("Delayed to March 25."));
        assert!(text.contains("Mar 20, 19:08 UTC"));
        assert!(text.contains("Launch pad assigned."));
        assert!(text.contains("Mar 19, 10:41 UTC"));
        assert!(text.contains("Added daily launch window."));
    }

    #[test]
    fn updates_cap_at_five_most_recent() {
        let mut detail = dummy_launch_detail();
        detail.updates = (0..8)
            .map(|i| make_update(3, 1 + i as u32, 12, 0, &format!("Update number {i}")))
            .collect();
        let mut lines = Vec::new();
        build_updates_section(&mut lines, &detail, 100);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        for i in 0..5 {
            assert!(text.contains(&format!("Update number {i}")), "update {i} should render");
        }
        for i in 5..8 {
            assert!(
                !text.contains(&format!("Update number {i}")),
                "update {i} beyond the first 5 should be hidden"
            );
        }
    }

    #[test]
    fn updates_long_comment_wraps() {
        let mut detail = dummy_launch_detail();
        let comment = "Flight readiness review complete and all systems reported nominal \
                       following an extended review of second stage telemetry captured \
                       during the most recent static fire test campaign at the launch site.";
        detail.updates = vec![make_update(3, 12, 14, 22, comment)];
        let mut lines = Vec::new();
        build_updates_section(&mut lines, &detail, 60);

        let start = lines
            .iter()
            .position(|l| line_text(l).contains("Flight readiness"))
            .expect("first fragment present");
        let end = lines
            .iter()
            .position(|l| line_text(l).contains("launch site"))
            .expect("last fragment present");
        assert!(end > start, "long comment should wrap onto multiple lines");
    }

    // ── Mission crew + landing sub-sections ───────────────────────────

    fn make_crew(name: &str, role: &str, agency: &str) -> CrewMember {
        CrewMember {
            name: name.into(),
            role: role.into(),
            agency: agency.into(),
        }
    }

    fn make_landing(
        stage: Option<&str>,
        attempt: bool,
        landing_type: Option<&str>,
        location: Option<&str>,
    ) -> StageLanding {
        StageLanding {
            stage_type: stage.map(Into::into),
            landing_attempt: attempt,
            landing_success: None,
            landing_type: landing_type.map(Into::into),
            landing_location: location.map(Into::into),
        }
    }

    #[test]
    fn mission_without_crew_or_landings_omits_subsections() {
        let mut detail = dummy_launch_detail();
        detail.mission = Some(MissionSummary {
            name: "Crew-10".into(),
            mission_type: "Crew Rotation".into(),
            description: None,
            orbit: None,
        });
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("MISSION"));
        assert!(text.contains("Crew-10 (Crew Rotation)"));
        assert!(!text.contains("CREW"));
        assert!(!text.contains("LANDING"));
    }

    #[test]
    fn mission_renders_crew_table() {
        let mut detail = dummy_launch_detail();
        detail.crew = vec![
            make_crew("Anne McClain", "Commander", "NASA"),
            make_crew("Akihiko Hoshide", "Pilot", "JAXA"),
            make_crew("Thomas Pesquet", "Mission Spc.", "ESA"),
            make_crew("Megan McArthur", "Mission Spc.", "NASA"),
        ];
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("CREW"));
        for (name, role, agency) in [
            ("Anne McClain", "Commander", "NASA"),
            ("Akihiko Hoshide", "Pilot", "JAXA"),
            ("Thomas Pesquet", "Mission Spc.", "ESA"),
            ("Megan McArthur", "Mission Spc.", "NASA"),
        ] {
            assert!(text.contains(name), "crew name {name} should render");
            assert!(text.contains(role), "crew role {role} should render");
            assert!(text.contains(agency), "crew agency {agency} should render");
        }

        // Name column is padded so the role column aligns.
        let row = lines
            .iter()
            .find(|l| line_text(l).contains("Anne McClain"))
            .expect("crew row present");
        let name_span = &row.spans[1];
        assert!(name_span.content.starts_with("Anne McClain"));
        assert_eq!(name_span.content.chars().count(), CREW_NAME_WIDTH);
    }

    #[test]
    fn mission_renders_landing_row_when_attempted() {
        let mut detail = dummy_launch_detail();
        detail.landings = vec![make_landing(
            Some("Booster"),
            true,
            Some("RTLS"),
            Some("Landing Zone 1"),
        )];
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("LANDING"));
        assert!(text.contains("Booster"));
        assert!(text.contains("RTLS"));
        assert!(text.contains("Landing Zone 1"));
    }

    #[test]
    fn mission_omits_landing_when_no_attempt() {
        let mut detail = dummy_launch_detail();
        detail.landings = vec![make_landing(Some("Booster"), false, None, None)];
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(!text.contains("LANDING"));
    }

    #[test]
    fn mission_renders_all_landing_rows_multi_stage() {
        let mut detail = dummy_launch_detail();
        detail.landings = vec![
            make_landing(
                Some("Core"),
                true,
                Some("ASDS"),
                Some("Of Course I Still Love You"),
            ),
            make_landing(Some("Side Booster"), true, Some("RTLS"), Some("Landing Zone 1")),
            make_landing(Some("Side Booster"), true, Some("RTLS"), Some("Landing Zone 2")),
        ];
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let heading = lines
            .iter()
            .position(|l| line_text(l).contains("LANDING"))
            .expect("landing heading present");
        let row_count = lines[heading + 1..]
            .iter()
            .filter(|l| {
                let t = line_text(l);
                t.contains("ASDS") || t.contains("RTLS")
            })
            .count();
        assert_eq!(row_count, 3, "all three recoverable stages should render");

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Of Course I Still Love You"));
        assert!(text.contains("Landing Zone 1"));
        assert!(text.contains("Landing Zone 2"));
    }

    #[test]
    fn mission_landing_row_omits_missing_columns() {
        let mut detail = dummy_launch_detail();
        detail.landings = vec![make_landing(Some("Booster"), true, None, None)];
        let mut lines = Vec::new();
        build_mission_section(&mut lines, &detail);

        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("LANDING"));
        assert!(text.contains("Booster"));
        assert!(!text.to_lowercase().contains("unknown"));
        assert!(!text.contains("None"));
    }

    // ── Probability colouring ─────────────────────────────────────────

    #[test]
    fn probability_high_renders_green() {
        let mut detail = rich_detail();
        detail.probability = Probability::new(90);
        let lines = build_content_lines(&detail, 120);
        let prob_line = lines.iter().find(|l| {
            l.spans.iter().any(|s| s.content.contains("90%"))
        }).expect("should contain probability");
        let pct_span = prob_line.spans.iter().find(|s| s.content.contains("90%")).unwrap();
        assert_eq!(pct_span.style.fg, Some(Color::Green));
    }

    #[test]
    fn probability_medium_renders_yellow() {
        let mut detail = rich_detail();
        detail.probability = Probability::new(60);
        let lines = build_content_lines(&detail, 120);
        let prob_line = lines.iter().find(|l| {
            l.spans.iter().any(|s| s.content.contains("60%"))
        }).expect("should contain probability");
        let pct_span = prob_line.spans.iter().find(|s| s.content.contains("60%")).unwrap();
        assert_eq!(pct_span.style.fg, Some(Color::Yellow));
    }

    #[test]
    fn probability_low_renders_red() {
        let mut detail = rich_detail();
        detail.probability = Probability::new(30);
        let lines = build_content_lines(&detail, 120);
        let prob_line = lines.iter().find(|l| {
            l.spans.iter().any(|s| s.content.contains("30%"))
        }).expect("should contain probability");
        let pct_span = prob_line.spans.iter().find(|s| s.content.contains("30%")).unwrap();
        assert_eq!(pct_span.style.fg, Some(Color::Red));
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
    fn probability_none_is_hidden() {
        let mut detail = rich_detail();
        detail.probability = None;
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

    // --- TestBackend rendering tests ---

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use crate::tui::app::App;
    use crate::models::LaunchDetailCache;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn render_detail_loading_shows_fetching_message() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new((120, 30));
        app.loading = true;

        terminal
            .draw(|frame| {
                render_detail(frame, frame.area(), "some-id", &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("Fetching launch details"),
            "loading detail should show 'Fetching launch details', got:\n{text}"
        );
    }

    #[test]
    fn render_detail_not_found_shows_back_hint() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new((120, 30));

        terminal
            .draw(|frame| {
                render_detail(frame, frame.area(), "missing-id", &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("No detail data available"),
            "missing detail should show not-available message, got:\n{text}"
        );
        assert!(
            text.contains("Esc"),
            "missing detail should hint about Esc key, got:\n{text}"
        );
    }

    #[test]
    fn render_large_description_does_not_corrupt_layout() {
        // 10KB+ mission description — edge case from design doc.
        let mut detail = rich_detail();
        // 10KB+ description with spaces so word-wrap actually wraps.
        let large_desc = "The quick brown fox jumps over the lazy dog. "
            .repeat(250); // ~11KB
        detail.mission = Some(MissionSummary {
            name: "Big Mission".into(),
            mission_type: "Test".into(),
            description: Some(large_desc),
            orbit: None,
        });

        // Build content lines — should not panic or produce zero lines.
        let lines = build_content_lines(&detail, 120);
        assert!(
            lines.len() > 50,
            "10KB description should produce many wrapped lines, got {}",
            lines.len()
        );

        // Also test rendering to TestBackend with scrolling.
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new((120, 30));
        let launch_id = detail.id.clone();
        app.detail_cache.insert(
            launch_id.clone(),
            LaunchDetailCache {
                version: 1,
                launch_id: launch_id.clone(),
                fetched_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
                ttl_strategy: CacheStrategy::ShortTerm,
                data: detail,
            },
        );
        app.detail_scroll_offset = 20; // Scroll down into the description.

        terminal
            .draw(|frame| {
                render_detail(frame, frame.area(), &launch_id, &app);
            })
            .unwrap();

        // Should render without panic. Content should exist.
        let text = buffer_text(&terminal);
        assert!(!text.is_empty());
    }

    #[test]
    fn render_unicode_launch_name_does_not_panic() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new((120, 30));
        let mut detail = rich_detail();
        detail.name = "长征五号 Ŷ CZ-5 • 嫦娥六号 🚀".into();
        detail.pad.location.name = "文昌航天发射场, 海南, 中国".into();
        let launch_id = detail.id.clone();
        app.detail_cache.insert(
            launch_id.clone(),
            LaunchDetailCache {
                version: 1,
                launch_id: launch_id.clone(),
                fetched_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
                ttl_strategy: CacheStrategy::ShortTerm,
                data: detail,
            },
        );

        terminal
            .draw(|frame| {
                render_detail(frame, frame.area(), &launch_id, &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("长征五号") || text.contains("CZ-5"),
            "unicode characters should render, got:\n{text}"
        );
    }

    #[test]
    fn render_detail_with_cached_data_shows_launch_name() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new((120, 30));
        let detail = rich_detail();
        let launch_id = detail.id.clone();
        app.detail_cache.insert(
            launch_id.clone(),
            LaunchDetailCache {
                version: 1,
                launch_id: launch_id.clone(),
                fetched_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
                ttl_strategy: CacheStrategy::ShortTerm,
                data: detail,
            },
        );

        terminal
            .draw(|frame| {
                render_detail(frame, frame.area(), &launch_id, &app);
            })
            .unwrap();

        let text = buffer_text(&terminal);
        assert!(
            text.contains("Starship IFT-7"),
            "detail with cached data should show launch name, got:\n{text}"
        );
        assert!(
            text.contains("SpaceX"),
            "detail should show provider, got:\n{text}"
        );
    }
}
