use std::collections::VecDeque;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    color::parse_sgr_parameters,
    state::{AppState, OutputCategory, OutputDisplayMode, OutputLine, OutputStyle, plain_text},
    ui::{
        panels::panel,
        theme::{Theme, parse_color},
    },
};

const MIN_OUTPUT_WRAP_WIDTH: usize = 20;

mod tables;

pub fn render_output(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    title: &str,
) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let visible = output_lines(state, inner_height, inner_width, theme);

    let title = output_title(state, title);
    Paragraph::new(visible)
        .alignment(theme.alignment)
        .block(panel(&title, theme))
        .render(area, buf);
}

fn output_title(state: &AppState, base_title: &str) -> String {
    let status = if state.output_view.status_ticks_remaining > 0 {
        let mode = match state.output_view.display_mode {
            OutputDisplayMode::Styled => "Styled",
            OutputDisplayMode::Plain => "Plain",
            OutputDisplayMode::Debug => "Debug",
        };
        let follow = if state.output_view.follow_newest {
            "Follow"
        } else {
            "Scroll"
        };
        format!(" [{mode}/{follow}]")
    } else {
        String::new()
    };
    if state.output_view.search_active {
        format!(
            "{base_title}{status} Search: {}",
            state.output_view.search_input
        )
    } else if state.output_view.search_query.is_empty() {
        format!("{base_title}{status}")
    } else {
        format!(
            "{base_title}{status} {}/{}",
            state
                .output_view
                .active_match
                .map(|index| index + 1)
                .unwrap_or(0),
            state.output_view.search_matches.len()
        )
    }
}

fn output_lines(
    state: &AppState,
    inner_height: usize,
    inner_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if inner_height == 0 {
        return Vec::new();
    }
    let (offset, hidden_rows) = clamped_scroll_position(state, inner_height, inner_width, theme);
    let end = state.output.len().saturating_sub(offset);
    let rows = rendered_rows(
        state,
        end.saturating_sub(inner_height),
        end,
        inner_width,
        theme,
    )
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let end = rows.len().saturating_sub(hidden_rows);
    rows.into_iter()
        .take(end)
        .skip(end.saturating_sub(inner_height))
        .collect()
}

fn heading_follows(state: &AppState, source_index: usize) -> bool {
    let all_lines = &state.output;
    let Some(source) = all_lines.get(source_index) else {
        return false;
    };
    state.output_view.display_mode == OutputDisplayMode::Styled
        && !plain_text(&source.normalized).trim().is_empty()
        && all_lines.get(source_index + 1).is_some_and(|next| {
            next.category == OutputCategory::System
                && markdown_heading(&plain_text(&next.normalized)).is_some()
        })
}

// Keep a logical-line anchor plus a row offset within that line. New output can
// preserve the anchor without knowing the terminal width or rewrapping history.
fn rendered_rows(
    state: &AppState,
    start: usize,
    end: usize,
    width: usize,
    theme: &Theme,
) -> Vec<Vec<Line<'static>>> {
    let all_lines = &state.output;
    let lines = match state.output_view.display_mode {
        OutputDisplayMode::Styled => ansi_lines(all_lines, start, end, state, theme),
        OutputDisplayMode::Plain => plain_lines(all_lines, start, end, state, theme, false),
        OutputDisplayMode::Debug => plain_lines(all_lines, start, end, state, theme, true),
    };
    let mut tables = if state.output_view.display_mode == OutputDisplayMode::Styled {
        tables::render(state, start, end, width, theme)
    } else {
        Default::default()
    };
    lines
        .into_iter()
        .zip(all_lines.range(start..end))
        .enumerate()
        .map(|(offset, (line, source))| {
            let source_index = start + offset;
            let mut rows = if let Some(rows) = tables.remove(&source_index) {
                rows
            } else if source.category == OutputCategory::Snapshot {
                vec![line.left_aligned()]
            } else if width < MIN_OUTPUT_WRAP_WIDTH {
                vec![line]
            } else {
                wrap_line(line, width)
            };
            if heading_follows(state, source_index) {
                rows.push(Line::from(""));
            }
            rows
        })
        .collect()
}

pub(crate) fn max_scroll_position(
    state: &AppState,
    height: usize,
    width: usize,
    theme: &Theme,
) -> (usize, usize) {
    position_for_line(state, height, width, theme, 0)
}

/// Position a retained logical line at the top, filling the page when possible.
pub(crate) fn position_for_line(
    state: &AppState,
    height: usize,
    width: usize,
    theme: &Theme,
    start: usize,
) -> (usize, usize) {
    let height = height.max(1);
    let start = start.min(state.output.len());
    let mut rows = 0usize;
    for (index, line) in rendered_rows(
        state,
        start,
        start.saturating_add(height).min(state.output.len()),
        width,
        theme,
    )
    .into_iter()
    .enumerate()
    {
        rows = rows.saturating_add(line.len());
        if rows >= height {
            return (state.output.len() - start - index - 1, rows - height);
        }
    }
    (0, 0)
}

fn clamped_scroll_position(
    state: &AppState,
    height: usize,
    width: usize,
    theme: &Theme,
) -> (usize, usize) {
    let position = (
        state.output_view.scroll_offset,
        state.output_view.wrapped_row_offset,
    );
    if position == (0, 0) || state.output.is_empty() {
        return (0, 0);
    }
    let (offset, rows) = position.min(max_scroll_position(state, height, width, theme));
    if rows == 0 {
        return (offset, 0);
    }
    let end = state.output.len() - offset;
    // Resizing or changing display mode can shorten the anchored line.
    let line_height = rendered_rows(state, end - 1, end, width, theme)[0].len();
    (offset, rows.min(line_height.saturating_sub(1)))
}

pub(crate) fn scroll_position(
    state: &AppState,
    height: usize,
    width: usize,
    theme: &Theme,
    amount: usize,
    up: bool,
) -> (usize, usize) {
    if state.output.is_empty() {
        return (0, 0);
    }
    let (offset, row_offset) = clamped_scroll_position(state, height, width, theme);
    let end = state.output.len() - offset;
    // Each logical line has at least one row, so only nearby lines are needed.
    let start = if up {
        end.saturating_sub(amount.saturating_add(1))
    } else {
        end - 1
    };
    let range_end = if up {
        end
    } else {
        end.saturating_add(amount).min(state.output.len())
    };
    let lines = rendered_rows(state, start, range_end, width, theme);
    let hidden = lines
        .iter()
        .skip(end - start)
        .map(Vec::len)
        .sum::<usize>()
        .saturating_add(row_offset);
    let mut remaining = if up {
        hidden.saturating_add(amount)
    } else {
        hidden.saturating_sub(amount)
    };
    let mut position = (state.output.len() - start, 0);
    for (index, rows) in lines.iter().enumerate().rev() {
        if remaining < rows.len() {
            position = (state.output.len() - start - index - 1, remaining);
            break;
        }
        remaining -= rows.len();
    }
    position.min(max_scroll_position(state, height, width, theme))
}

fn ansi_lines(
    lines: &VecDeque<OutputLine>,
    visible_start: usize,
    visible_end: usize,
    state: &AppState,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let default_fg = theme.foreground;
    let mut mud_style = Style::new().fg(default_fg);
    let mut rendered = Vec::with_capacity(visible_end.saturating_sub(visible_start));
    // Earlier styles cannot cross an explicit reset boundary. A prompt resets
    // after its text, so a hidden prompt itself need not be parsed.
    let parse_start = lines
        .range(..visible_start)
        .rposition(|line| {
            line.starts_new_output
                || matches!(
                    line.category,
                    OutputCategory::Prompt | OutputCategory::Error
                )
        })
        .map_or(0, |index| {
            if matches!(
                lines[index].category,
                OutputCategory::Prompt | OutputCategory::Error
            ) {
                index + 1
            } else {
                index
            }
        });

    for (offset, line) in lines.range(parse_start..visible_end).enumerate() {
        let index = parse_start + offset;
        if line.starts_new_output {
            mud_style = Style::new().fg(default_fg);
        }
        if let Some(prefix) = &line.source_prefix {
            AnsiParser {
                remaining: prefix,
                style: &mut mud_style,
                default_fg,
                theme: Some(theme),
            }
            .scan(false);
        }
        if index < visible_start {
            if matches!(
                line.category,
                OutputCategory::Normal
                    | OutputCategory::Combat
                    | OutputCategory::Communication
                    | OutputCategory::Triggered
            ) {
                AnsiParser {
                    remaining: &line.normalized,
                    style: &mut mud_style,
                    default_fg,
                    theme: Some(theme),
                }
                .scan(false);
            }
            continue;
        }
        let rendered_line = match line.category {
            OutputCategory::Snapshot => ansi_line_for_theme(&line.normalized, default_fg, theme),
            OutputCategory::Normal
            | OutputCategory::Combat
            | OutputCategory::Communication
            | OutputCategory::Triggered => {
                ansi_line_with_style_for_theme(&line.normalized, &mut mud_style, default_fg, theme)
            }
            OutputCategory::Prompt => {
                let rendered_line = ansi_line_with_style_for_theme(
                    &line.normalized,
                    &mut mud_style,
                    default_fg,
                    theme,
                );
                mud_style = Style::new().fg(default_fg);
                rendered_line
            }
            OutputCategory::System => markdown_system_line(&line.normalized, theme),
            OutputCategory::Error => {
                mud_style = Style::new().fg(default_fg);
                ansi_line_for_theme(
                    &line.normalized,
                    color_for(line.category.clone(), theme),
                    theme,
                )
            }
        };
        let rendered_line = apply_output_style(rendered_line, line.style.as_ref(), theme);
        rendered.push(if line.category == OutputCategory::Snapshot {
            rendered_line
        } else {
            mark_search_match(rendered_line, index, state, theme)
        });
    }

    rendered
}

fn markdown_system_line(raw: &str, theme: &Theme) -> Line<'static> {
    let text = plain_text(raw);
    if text.trim().is_empty() {
        return Line::from("");
    }
    if let Some((level, title)) = markdown_heading(&text) {
        return Line::from(Span::styled(
            title.to_string(),
            Style::new()
                .fg(if level == 1 {
                    theme.title
                } else {
                    theme.accent
                })
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(item) = text.strip_prefix("- ") {
        let mut spans = vec![Span::styled("• ", Style::new().fg(theme.accent))];
        spans.extend(inline_code_spans(item, theme));
        return Line::from(spans);
    }
    Line::from(inline_code_spans(&text, theme))
}

fn markdown_heading(text: &str) -> Option<(usize, &str)> {
    let level = text.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) || text.as_bytes().get(level) != Some(&b' ') {
        return None;
    }
    Some((level, &text[level + 1..]))
}

fn inline_code_spans(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    let mut in_code = false;
    while let Some(index) = rest.find('`') {
        let (before, after_tick) = rest.split_at(index);
        if !before.is_empty() {
            spans.push(Span::styled(
                before.to_string(),
                Style::new().fg(if in_code {
                    theme.warning
                } else {
                    theme.foreground
                }),
            ));
        }
        in_code = !in_code;
        rest = &after_tick[1..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(
            rest.to_string(),
            Style::new().fg(if in_code {
                theme.warning
            } else {
                theme.foreground
            }),
        ));
    }
    spans
}

fn plain_lines(
    lines: &VecDeque<OutputLine>,
    visible_start: usize,
    visible_end: usize,
    state: &AppState,
    theme: &Theme,
    debug: bool,
) -> Vec<Line<'static>> {
    lines
        .range(visible_start..visible_end)
        .enumerate()
        .map(|(index, line)| {
            let index = visible_start + index;
            let value = plain_text(&line.normalized);
            let content = if debug && line.category != OutputCategory::Snapshot {
                format!("[{:?}] {}", line.category, value)
            } else {
                value
            };
            let rendered_line = apply_output_style(
                Line::from(Span::styled(
                    content,
                    Style::new().fg(color_for(line.category.clone(), theme)),
                )),
                line.style.as_ref(),
                theme,
            );
            if line.category == OutputCategory::Snapshot {
                rendered_line
            } else {
                mark_search_match(rendered_line, index, state, theme)
            }
        })
        .collect()
}

fn apply_output_style(
    line: Line<'static>,
    style: Option<&OutputStyle>,
    theme: &Theme,
) -> Line<'static> {
    let Some(style) = style else {
        return line;
    };
    let mut extra = Style::new();
    if let Some(color) = style.foreground.as_deref().and_then(parse_color) {
        extra = extra.fg(color);
    }
    if let Some(color) = style.background.as_deref().and_then(parse_color) {
        extra = extra.bg(color);
    }
    if style.bold {
        extra = extra.add_modifier(Modifier::BOLD);
    }
    if style.dim {
        extra = extra.add_modifier(Modifier::DIM);
    }
    if style.italic {
        extra = extra.add_modifier(Modifier::ITALIC);
    }
    if style.underline {
        extra = extra.add_modifier(Modifier::UNDERLINED);
    }
    if style.reverse {
        extra = extra.add_modifier(Modifier::REVERSED);
    }
    Line::from(
        line.spans
            .into_iter()
            .map(|mut span| {
                span.style = span.style.patch(extra);
                if let Some(foreground) = span.style.fg {
                    let background = span
                        .style
                        .bg
                        .filter(|color| *color != Color::Reset)
                        .unwrap_or(theme.background);
                    span.style = span
                        .style
                        .fg(theme.readable_text_color(foreground, background));
                }
                span
            })
            .collect::<Vec<_>>(),
    )
}

fn mark_search_match(
    line: Line<'static>,
    index: usize,
    state: &AppState,
    theme: &Theme,
) -> Line<'static> {
    let Ok(match_position) = state.output_view.search_matches.binary_search(&index) else {
        return line;
    };
    let is_active = state.output_view.active_match == Some(match_position);
    let marker = if is_active { ">" } else { "*" };
    let marker_style = if is_active {
        Style::new()
            .fg(theme.contrasting_foreground(theme.warning))
            .bg(theme.warning)
    } else {
        Style::new().fg(theme.warning)
    };
    let mut spans = vec![Span::styled(marker, marker_style), Span::raw(" ")];
    spans.extend(line.spans);
    Line::from(spans)
}

fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if line.spans.is_empty() {
        return vec![Line::from("")];
    }
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;
    for span in line.spans {
        for token in styled_tokens(span.content.as_ref(), span.style) {
            push_wrapped_token(
                token.text,
                token.style,
                width,
                &mut rows,
                &mut current,
                &mut current_width,
            );
        }
    }
    rows.push(Line::from(current));
    rows
}

struct StyledToken {
    text: String,
    style: Style,
}

fn styled_tokens(text: &str, style: Style) -> Vec<StyledToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_is_whitespace = None;
    for value in text.chars() {
        let is_whitespace = value.is_whitespace();
        if current_is_whitespace == Some(is_whitespace) || current.is_empty() {
            current.push(value);
            current_is_whitespace = Some(is_whitespace);
        } else {
            tokens.push(StyledToken {
                text: std::mem::take(&mut current),
                style,
            });
            current.push(value);
            current_is_whitespace = Some(is_whitespace);
        }
    }
    if !current.is_empty() {
        tokens.push(StyledToken {
            text: current,
            style,
        });
    }
    tokens
}

fn push_wrapped_token(
    token: String,
    style: Style,
    width: usize,
    rows: &mut Vec<Line<'static>>,
    current: &mut Vec<Span<'static>>,
    current_width: &mut usize,
) {
    let token_width = display_width(&token);
    if token_width <= width {
        if *current_width + token_width > width && !current.is_empty() {
            rows.push(Line::from(std::mem::take(current)));
            *current_width = 0;
        }
        current.push(Span::styled(token, style));
        *current_width += token_width;
        return;
    }
    for value in token.chars() {
        let value_width = value.width().unwrap_or(0);
        if *current_width + value_width > width && !current.is_empty() {
            rows.push(Line::from(std::mem::take(current)));
            *current_width = 0;
        }
        current.push(Span::styled(value.to_string(), style));
        *current_width += value_width;
    }
}

fn display_width(value: &str) -> usize {
    value.chars().map(|value| value.width().unwrap_or(0)).sum()
}

#[cfg(test)]
fn ansi_line(value: &str, default_fg: Color) -> Line<'static> {
    let mut style = Style::new().fg(default_fg);
    ansi_line_with_style(value, &mut style, default_fg)
}

fn ansi_line_for_theme(value: &str, default_fg: Color, theme: &Theme) -> Line<'static> {
    let mut style = Style::new().fg(default_fg);
    ansi_line_with_style_for_theme(value, &mut style, default_fg, theme)
}

#[cfg(test)]
fn ansi_line_with_style(value: &str, style: &mut Style, default_fg: Color) -> Line<'static> {
    let mut parser = AnsiParser {
        remaining: value,
        style,
        default_fg,
        theme: None,
    };
    Line::from(parser.parse())
}

fn ansi_line_with_style_for_theme(
    value: &str,
    style: &mut Style,
    default_fg: Color,
    theme: &Theme,
) -> Line<'static> {
    let mut parser = AnsiParser {
        remaining: value,
        style,
        default_fg,
        theme: Some(theme),
    };
    Line::from(parser.parse())
}

struct AnsiParser<'a> {
    remaining: &'a str,
    style: &'a mut Style,
    default_fg: Color,
    theme: Option<&'a Theme>,
}

impl<'a> AnsiParser<'a> {
    fn parse(&mut self) -> Vec<Span<'static>> {
        self.scan(true)
    }

    fn scan(&mut self, render_text: bool) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        while let Some(index) = self.remaining.find("\x1b[") {
            let (plain, rest) = self.remaining.split_at(index);
            if render_text && !plain.is_empty() {
                spans.push(Span::styled(plain.to_string(), self.readable_style()));
            }
            let Some(end) = rest.find('m') else {
                self.remaining = "";
                return spans;
            };
            self.apply_sgr(&rest[2..end]);
            self.remaining = &rest[end + 1..];
        }
        if render_text && !self.remaining.is_empty() {
            spans.push(Span::styled(
                self.remaining.to_string(),
                self.readable_style(),
            ));
        }
        spans
    }

    fn readable_style(&self) -> Style {
        let Some(theme) = self.theme else {
            return *self.style;
        };
        let Some(foreground) = self.style.fg else {
            return *self.style;
        };
        let background = self
            .style
            .bg
            .filter(|color| *color != Color::Reset)
            .unwrap_or(theme.background);
        self.style
            .fg(theme.readable_text_color(foreground, background))
    }

    fn apply_sgr(&mut self, sequence: &str) {
        let Some(values) = parse_sgr_parameters(sequence) else {
            return;
        };
        let mut index = 0;
        while index < values.len() {
            match values[index] {
                0 => *self.style = Style::new().fg(self.default_fg),
                1 => *self.style = self.style.add_modifier(Modifier::BOLD),
                2 => *self.style = self.style.add_modifier(Modifier::DIM),
                3 => *self.style = self.style.add_modifier(Modifier::ITALIC),
                4 => *self.style = self.style.add_modifier(Modifier::UNDERLINED),
                22 => *self.style = self.style.remove_modifier(Modifier::BOLD | Modifier::DIM),
                23 => *self.style = self.style.remove_modifier(Modifier::ITALIC),
                24 => *self.style = self.style.remove_modifier(Modifier::UNDERLINED),
                30..=37 => *self.style = self.style.fg(ansi_color(values[index] - 30, false)),
                39 => *self.style = self.style.fg(self.default_fg),
                40..=47 => *self.style = self.style.bg(ansi_color(values[index] - 40, false)),
                49 => *self.style = self.style.bg(Color::Reset),
                90..=97 => *self.style = self.style.fg(ansi_color(values[index] - 90, true)),
                100..=107 => *self.style = self.style.bg(ansi_color(values[index] - 100, true)),
                38 if values.get(index + 1) == Some(&5) => {
                    if let Some(color) = values
                        .get(index + 2)
                        .and_then(|value| u8::try_from(*value).ok())
                    {
                        *self.style = self.style.fg(Color::Indexed(color));
                        index += 2;
                    }
                }
                38 if values.get(index + 1) == Some(&2) => {
                    if let (Some(red), Some(green), Some(blue)) = (
                        values
                            .get(index + 2)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 3)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 4)
                            .and_then(|value| u8::try_from(*value).ok()),
                    ) {
                        *self.style = self.style.fg(Color::Rgb(red, green, blue));
                        index += 4;
                    }
                }
                48 if values.get(index + 1) == Some(&5) => {
                    if let Some(color) = values
                        .get(index + 2)
                        .and_then(|value| u8::try_from(*value).ok())
                    {
                        *self.style = self.style.bg(Color::Indexed(color));
                        index += 2;
                    }
                }
                48 if values.get(index + 1) == Some(&2) => {
                    if let (Some(red), Some(green), Some(blue)) = (
                        values
                            .get(index + 2)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 3)
                            .and_then(|value| u8::try_from(*value).ok()),
                        values
                            .get(index + 4)
                            .and_then(|value| u8::try_from(*value).ok()),
                    ) {
                        *self.style = self.style.bg(Color::Rgb(red, green, blue));
                        index += 4;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}

fn ansi_color(index: u16, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (7, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}

fn color_for(category: OutputCategory, theme: &Theme) -> ratatui::style::Color {
    match category {
        OutputCategory::Normal | OutputCategory::Snapshot | OutputCategory::Prompt => {
            theme.foreground
        }
        OutputCategory::Combat => theme.danger,
        OutputCategory::Communication => theme.accent,
        OutputCategory::System => theme.accent,
        OutputCategory::Error => theme.danger,
        OutputCategory::Triggered => theme.warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;

    fn output_line(value: &str, category: OutputCategory) -> OutputLine {
        OutputLine {
            source_id: None,
            source_prefix: None,
            raw: value.to_string(),
            normalized: value.to_string(),
            category,
            style: None,
            starts_new_output: false,
        }
    }

    fn new_output_line(value: &str) -> OutputLine {
        OutputLine {
            starts_new_output: true,
            ..output_line(value, OutputCategory::Normal)
        }
    }

    #[test]
    fn output_title_hides_mode_status_by_default() {
        let state = AppState::new(&crate::config::AppConfig::default());

        assert_eq!(output_title(&state, "MUD Output"), "MUD Output");
    }

    fn table_state() -> AppState {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        for text in [
            "| Theme | Description |",
            "| :--- | ---: |",
            "| `hobbit` | Shire gardens and golden hearthlight |",
            "| `dwarf` | Erebor |",
        ] {
            state.push_output(text, OutputCategory::System);
        }
        state
    }

    #[test]
    fn markdown_tables_wrap_cells_align_and_preserve_viewport_anchors() {
        let state = table_state();
        let theme = Theme::from_config(&ThemeConfig::default());
        for width in [11, 20, 32, 80] {
            let rows = rendered_rows(&state, 0, 4, width, &theme);
            assert!(line_text(&rows[0][0]).starts_with('┌'));
            assert!(line_text(rows[3].last().unwrap()).ends_with('┘'));
            let table_width = rows[0][0].width();
            assert!(table_width <= width);
            assert!(
                rows.iter()
                    .flatten()
                    .all(|line| line.width() == table_width)
            );
            assert_eq!(rendered_rows(&state, 2, 3, width, &theme), rows[2..3]);
            assert!(
                !rows
                    .iter()
                    .flatten()
                    .any(|line| line_text(line).contains('`'))
            );
            assert_eq!(
                output_lines(&state, 2, width, &theme),
                rows.iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>()
                    .split_off(rows.iter().map(Vec::len).sum::<usize>() - 2)
            );
        }
        let rows = rendered_rows(&state, 0, 4, 80, &theme);
        assert!(line_text(&rows[3][0]).contains("                        Erebor │"));
        assert!(rows[2].iter().flat_map(|row| &row.spans).any(|span| span.content.contains("hobbit") && span.style.fg == Some(theme.warning)));
    }

    #[test]
    fn markdown_tables_preserve_plain_debug_and_non_table_output() {
        let mut state = table_state();
        let theme = Theme::from_config(&ThemeConfig::default());
        for mode in [OutputDisplayMode::Plain, OutputDisplayMode::Debug] {
            state.output_view.display_mode = mode;
            let rows = rendered_rows(&state, 0, 4, 100, &theme);
            assert!(line_text(&rows[0][0]).contains("| Theme |"));
            assert!(
                !rows
                    .iter()
                    .flatten()
                    .any(|line| line_text(line).contains('┌'))
            );
        }
        state.output_view.display_mode = OutputDisplayMode::Styled;
        assert!(line_text(&rendered_rows(&state, 0, 4, 5, &theme)[0][0]).contains('|'));
        for line in &mut state.output {
            line.category = OutputCategory::Normal;
        }
        assert!(line_text(&rendered_rows(&state, 0, 4, 80, &theme)[0][0]).contains('|'));
        let mut state = table_state();
        state.output[1].normalized = "| ordinary | prose |".into();
        assert!(line_text(&rendered_rows(&state, 0, 4, 80, &theme)[0][0]).contains('|'));
    }

    #[test]
    fn markdown_tables_handle_unicode_escaped_pipes_fences_and_search() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = table_state();
        state.output[2].normalized = r"| 森林 | `a\|b` |".into();
        state.output_view.search_matches = vec![2];
        state.output_view.active_match = Some(0);
        for width in [13, 20, 80] {
            let rows = rendered_rows(&state, 0, 4, width, &theme);
            assert!(rows.iter().flatten().all(|line| line.width() <= width));
            assert!(line_text(&rows[2][0]).starts_with("> │"));
        }
        let rows = rendered_rows(&state, 0, 4, 80, &theme);
        assert!(line_text(&rows[2][0]).contains("a|b"));
        state
            .output
            .push_front(output_line("```markdown", OutputCategory::System));
        assert!(
            !rendered_rows(&state, 1, 5, 80, &theme)
                .iter()
                .flatten()
                .any(|line| line_text(line).contains('┌'))
        );
    }

    #[test]
    fn theme_help_table_renders_all_races_and_scrolls_through_wrapped_cells() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        for line in include_str!("../../docs/commands/theme.md").lines() {
            state.push_output(line, OutputCategory::System);
        }
        let theme = Theme::from_config(&ThemeConfig::default());
        let start = state
            .output
            .iter()
            .position(|line| line.normalized.starts_with("| Theme |"))
            .unwrap();
        for width in [20, 40, 80, 120] {
            let rows = rendered_rows(&state, start, start + 12, width, &theme);
            assert!(line_text(&rows[0][0]).starts_with('┌'));
            assert!(line_text(rows[11].last().unwrap()).ends_with('┘'));
            assert!(rows.iter().flatten().all(|line| line.width() <= width));
            for index in start..start + 12 {
                assert_eq!(
                    rendered_rows(&state, index, index + 1, width, &theme),
                    rows[index - start..index - start + 1]
                );
            }
        }
    }

    #[test]
    fn markdown_tables_short_search_and_header_code_keep_table_styles() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        for text in ["| `A` | B |", "| --- | --- |", "| x | y |"] {
            state.push_output(text, OutputCategory::System);
        }
        let mut theme = Theme::from_config(&ThemeConfig::default());
        theme.foreground = theme.warning;
        theme.title = Color::LightBlue;
        state.output_view.search_matches = vec![2];
        let rows = rendered_rows(&state, 0, 3, 80, &theme);
        assert!(line_text(&rows[0][0]).starts_with("  ┌"));
        assert!(
            rows[0][1]
                .spans
                .iter()
                .any(|span| span.content == "B" && span.style.fg == Some(theme.title))
        );
        assert!(rows[0][1].spans.iter().any(|span| span.content == "A"
            && span.style.fg == Some(theme.warning)
            && span.style.add_modifier.contains(Modifier::BOLD)));
        for text in ["```", "````markdown"].into_iter() {
            state
                .output
                .push_front(output_line(text, OutputCategory::System));
        }
        assert!(
            !rendered_rows(&state, 2, 5, 80, &theme)
                .iter()
                .flatten()
                .any(|line| line_text(line).contains('┌'))
        );
    }

    #[test]
    fn markdown_tables_wrap_joined_emoji_and_combining_marks_without_splitting() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = table_state();
        state.output[2].normalized = "| 👩‍💻👩‍💻 | e\u{301}e\u{301} |".into();
        for width in [11, 20, 80] {
            let rows = rendered_rows(&state, 0, 4, width, &theme);
            assert!(rows.iter().flatten().all(|line| line.width() <= width));
            let content = rows[2].iter().map(line_text).collect::<Vec<_>>().join("\n");
            assert_eq!(content.matches("👩‍💻").count(), 2);
            assert_eq!(content.matches("e\u{301}").count(), 2);
        }
    }

    #[test]
    fn output_title_shows_mode_status_while_timer_is_active() {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.output_view.status_ticks_remaining = 1;

        assert_eq!(
            output_title(&state, "MUD Output"),
            "MUD Output [Styled/Follow]"
        );
    }

    #[test]
    fn ansi_line_preserves_sgr_as_spans_without_control_text() {
        let line = ansi_line("\x1b[32mgreen\x1b[0m plain", Color::White);

        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content.as_ref(), "green");
        assert_eq!(line.spans[0].style.fg, Some(Color::Green));
        assert_eq!(line.spans[1].content.as_ref(), " plain");
        assert_eq!(line.spans[1].style.fg, Some(Color::White));
    }

    #[test]
    fn ansi_line_supports_rots_rgb_style_sequences() {
        let line = ansi_line("\x1b[38;2;120;40;10mtruecolor", Color::White);

        assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(120, 40, 10)));
    }

    #[test]
    fn light_theme_adapts_low_contrast_ansi_foregrounds() {
        let theme = Theme::from_named("catppuccin-latte").unwrap();
        let line = ansi_line_for_theme("\x1b[37mwhite \x1b[30mblack", theme.foreground, &theme);
        assert_ne!(line.spans[0].style.fg, Some(Color::Gray));
        assert_eq!(line.spans[1].style.fg, Some(Color::Black));

        let explicit_background = ansi_line_for_theme(
            "\x1b[47;30mblack on white\x1b[40;37mwhite on black",
            theme.foreground,
            &theme,
        );
        assert_eq!(explicit_background.spans[0].style.fg, Some(Color::Black));
        assert_eq!(explicit_background.spans[1].style.fg, Some(Color::Gray));
    }

    #[test]
    fn light_theme_adapts_highlight_colors_against_their_final_background() {
        let theme = Theme::from_named("catppuccin-latte").unwrap();
        let low_contrast = OutputStyle {
            foreground: Some("white".into()),
            ..OutputStyle::default()
        };
        let line = apply_output_style(Line::from("highlight"), Some(&low_contrast), &theme);
        assert_ne!(line.spans[0].style.fg, Some(Color::White));

        let explicit_pair = OutputStyle {
            foreground: Some("white".into()),
            background: Some("black".into()),
            ..OutputStyle::default()
        };
        let line = apply_output_style(Line::from("highlight"), Some(&explicit_pair), &theme);
        assert_eq!(line.spans[0].style.fg, Some(Color::White));
        assert_eq!(line.spans[0].style.bg, Some(Color::Black));
    }

    #[test]
    fn ansi_line_supports_named_indexed_and_rgb_backgrounds() {
        let line = ansi_line(
            "\x1b[44mblue\x1b[48;5;17mindexed\x1b[48;2;10;20;30mrgb",
            Color::White,
        );

        assert_eq!(line.spans[0].style.bg, Some(Color::Blue));
        assert_eq!(line.spans[1].style.bg, Some(Color::Indexed(17)));
        assert_eq!(line.spans[2].style.bg, Some(Color::Rgb(10, 20, 30)));
    }

    #[test]
    fn ansi_line_rejects_malformed_sgr_and_treats_empty_parameters_as_reset() {
        let malformed = ansi_line("\x1b[38;bogus;5;196mplain", Color::White);
        assert_eq!(malformed.spans[0].style.fg, Some(Color::White));

        let reset = ansi_line("\x1b[31mred\x1b[;34mblue", Color::White);
        assert_eq!(reset.spans[0].style.fg, Some(Color::Red));
        assert_eq!(reset.spans[1].style.fg, Some(Color::Blue));
    }

    #[test]
    fn ansi_lines_carry_mud_style_across_line_boundaries() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[32mfirst", OutputCategory::Normal),
            output_line("second\x1b[0m", OutputCategory::Normal),
            output_line("third", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, state.output.len(), &state, &theme);
        let green = theme.readable_text_color(Color::Green, theme.background);

        assert_eq!(rendered[0].spans[0].style.fg, Some(green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(green));
        assert_eq!(rendered[2].spans[0].style.fg, Some(theme.foreground));
    }

    #[test]
    fn ansi_lines_do_not_carry_mud_style_into_system_messages() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[32mfirst", OutputCategory::Normal),
            output_line("look", OutputCategory::System),
            output_line("second", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, state.output.len(), &state, &theme);
        let green = theme.readable_text_color(Color::Green, theme.background);

        assert_eq!(rendered[0].spans[0].style.fg, Some(green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(theme.foreground));
        assert_eq!(rendered[2].spans[0].style.fg, Some(green));
    }

    #[test]
    fn ansi_lines_reset_mud_style_at_explicit_system_boundary() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut boundary = output_line("Disconnected.", OutputCategory::System);
        boundary.starts_new_output = true;
        let lines = vec![
            output_line("\x1b[32mfirst", OutputCategory::Normal),
            boundary,
            output_line("plain", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, 3, &state, &theme);

        assert_eq!(rendered[2].spans[0].style.fg, Some(theme.foreground));
    }

    #[test]
    fn ansi_lines_reset_mud_style_after_error_boundary() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[31;44mcolored", OutputCategory::Normal),
            output_line("connection failed", OutputCategory::Error),
            output_line("plain", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, 3, &state, &theme);

        assert_eq!(rendered[2].spans[0].style.fg, Some(theme.foreground));
        assert_eq!(rendered[2].spans[0].style.bg, None);
    }

    #[test]
    fn markdown_system_line_styles_headings_bullets_and_code() {
        let theme = Theme::from_config(&ThemeConfig::default());

        let heading = markdown_system_line("# Help", &theme);
        let bullet = markdown_system_line("- `/map door e trigger` - set trigger door", &theme);

        assert_eq!(heading.spans[0].content.as_ref(), "Help");
        assert_eq!(heading.spans[0].style.fg, Some(theme.title));
        assert_eq!(bullet.spans[0].content.as_ref(), "• ");
        assert_eq!(bullet.spans[1].style.fg, Some(theme.warning));
    }

    #[test]
    fn styled_markdown_headings_have_one_blank_row_before_them() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        for (text, category) in [
            ("MUD output", OutputCategory::Normal),
            ("# Heading", OutputCategory::System),
            ("body", OutputCategory::System),
            ("## Subheading", OutputCategory::System),
            ("", OutputCategory::System),
            ("### Third level", OutputCategory::System),
        ] {
            state.push_output(text, category);
        }
        let rows = rendered_rows(&state, 0, state.output.len(), 80, &theme);
        assert_eq!(
            rows[0].iter().map(line_text).collect::<Vec<_>>(),
            ["MUD output", ""]
        );
        assert_eq!(
            rows[1].iter().map(line_text).collect::<Vec<_>>(),
            ["Heading"]
        );
        assert_eq!(
            rows[2].iter().map(line_text).collect::<Vec<_>>(),
            ["body", ""]
        );
        assert_eq!(
            rows[3].iter().map(line_text).collect::<Vec<_>>(),
            ["Subheading"]
        );
        assert_eq!(rows[4].iter().map(line_text).collect::<Vec<_>>(), [""]);
        assert_eq!(
            rows[5].iter().map(line_text).collect::<Vec<_>>(),
            ["Third level"]
        );
        assert_eq!(rows[5][0].spans[0].style.fg, Some(theme.accent));
    }

    #[test]
    fn heading_spacing_is_stable_at_tiny_widths_and_skipped_at_viewport_start() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output("previous", OutputCategory::Normal);
        state.push_output("# A heading that can wrap", OutputCategory::System);
        state.output_view.search_matches = vec![1];
        state.output_view.active_match = Some(0);

        for width in [8, 20, 80] {
            let rows = rendered_rows(&state, 0, 2, width, &theme);
            assert_eq!(line_text(rows[0].last().unwrap()), "");
            assert!(line_text(&rows[1][0]).starts_with("> A"));
            assert_eq!(
                rows[0]
                    .iter()
                    .filter(|line| line_text(line).is_empty())
                    .count(),
                1
            );

            let anchored = rendered_rows(&state, 1, 2, width, &theme);
            assert!(line_text(&anchored[0][0]).starts_with("> A"));
        }
    }

    #[test]
    fn heading_separator_remains_scrollable_when_preceding_output_wraps() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output(
            "one two three four five six seven eight nine ten",
            OutputCategory::Normal,
        );
        state.push_output("# Heading", OutputCategory::System);

        let visible = output_lines(&state, 2, 20, &theme);

        assert_eq!(
            visible.iter().map(line_text).collect::<Vec<_>>(),
            ["", "Heading"]
        );
    }

    #[test]
    fn heading_separator_scrolls_stably_in_both_directions() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output("normal", OutputCategory::Normal);
        state.push_output("# Heading", OutputCategory::System);
        state.push_output("body", OutputCategory::System);

        let expected = [["Heading", "body"], ["", "Heading"], ["normal", ""]];
        for rows in &expected {
            assert_eq!(
                output_lines(&state, 2, 80, &theme)
                    .iter()
                    .map(line_text)
                    .collect::<Vec<_>>(),
                *rows
            );
            let position = scroll_position(&state, 2, 80, &theme, 1, true);
            (
                state.output_view.scroll_offset,
                state.output_view.wrapped_row_offset,
            ) = position;
        }
        for rows in expected.iter().rev().skip(1) {
            let position = scroll_position(&state, 2, 80, &theme, 1, false);
            (
                state.output_view.scroll_offset,
                state.output_view.wrapped_row_offset,
            ) = position;
            assert_eq!(
                output_lines(&state, 2, 80, &theme)
                    .iter()
                    .map(line_text)
                    .collect::<Vec<_>>(),
                *rows
            );
        }
    }

    #[test]
    fn only_system_markdown_headings_gain_spacing() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output("previous", OutputCategory::Normal);
        state.push_output("\x1b[31m# warning", OutputCategory::Normal);

        let rows = rendered_rows(&state, 0, 2, 80, &theme);

        assert_eq!(rows[1].len(), 1);
        assert_eq!(line_text(&rows[1][0]), "# warning");
    }

    #[test]
    fn markdown_heading_syntax_stops_at_level_six_and_requires_a_space() {
        let theme = Theme::from_config(&ThemeConfig::default());

        assert_eq!(
            line_text(&markdown_system_line("###### Six", &theme)),
            "Six"
        );
        assert_eq!(
            line_text(&markdown_system_line("####### Seven", &theme)),
            "####### Seven"
        );
        assert_eq!(
            line_text(&markdown_system_line("###Missing", &theme)),
            "###Missing"
        );
    }

    #[test]
    fn first_heading_and_plain_debug_modes_do_not_gain_rendered_spacing() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output("# First", OutputCategory::System);
        assert_eq!(rendered_rows(&state, 0, 1, 80, &theme)[0].len(), 1);

        state.push_output("## Second", OutputCategory::System);
        for mode in [OutputDisplayMode::Plain, OutputDisplayMode::Debug] {
            state.output_view.display_mode = mode;
            let rows = rendered_rows(&state, 0, 2, 80, &theme);
            assert_eq!(rows[0].len(), 1);
            assert_eq!(rows[1].len(), 1);
            assert!(line_text(&rows[1][0]).contains("## Second"));
        }
    }

    #[test]
    fn ansi_lines_carry_style_from_lines_above_visible_window() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[32mhidden", OutputCategory::Normal),
            output_line("visible", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 1, state.output.len(), &state, &theme);

        assert_eq!(rendered.len(), 1);
        assert_eq!(
            rendered[0].spans[0].style.fg,
            Some(theme.readable_text_color(Color::Green, theme.background))
        );
    }

    #[test]
    fn ansi_lines_reset_mud_style_after_prompt_boundary() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[32mroom", OutputCategory::Normal),
            output_line("Gram>", OutputCategory::Prompt),
            output_line("next output", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, state.output.len(), &state, &theme);
        let green = theme.readable_text_color(Color::Green, theme.background);

        assert_eq!(rendered[0].spans[0].style.fg, Some(green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(green));
        assert_eq!(rendered[2].spans[0].style.fg, Some(theme.foreground));
    }

    #[test]
    fn ansi_lines_reset_mud_style_after_removed_prompt_boundary() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line(
                "\x1b[32mA friendly little pony is here.",
                OutputCategory::Normal,
            ),
            new_output_line("You have 465/465 hit, 71/71 stamina, 176/176 moves."),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(&state.output, 0, state.output.len(), &state, &theme);

        assert_eq!(
            rendered[0].spans[0].style.fg,
            Some(theme.readable_text_color(Color::Green, theme.background))
        );
        assert_eq!(rendered[1].spans[0].style.fg, Some(theme.foreground));
    }

    #[test]
    fn output_lines_wrap_long_mud_output_to_pane_width() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(vec![output_line(
            "The forest path continues north toward the hill.",
            OutputCategory::Normal,
        )]);

        let rendered = output_lines(&state, 10, 20, &theme);

        assert_eq!(
            rendered.iter().map(line_text).collect::<Vec<_>>(),
            vec!["The forest path ", "continues north ", "toward the hill."]
        );
    }

    #[test]
    fn output_lines_preserve_leading_mud_spacing() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(vec![output_line(
            "                    RETURN OF THE SHADOW",
            OutputCategory::Normal,
        )]);

        let rendered = output_lines(&state, 10, 80, &theme);

        assert_eq!(
            rendered.iter().map(line_text).collect::<Vec<_>>(),
            vec!["                    RETURN OF THE SHADOW"]
        );
    }

    #[test]
    fn debug_mode_renders_snapshot_rows_literally_without_wrapping() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.output_view.display_mode = OutputDisplayMode::Debug;
        state.push_output_snapshot(vec![
            "....................".to_string(),
            ".........@..........".to_string(),
            ".........|..........".to_string(),
        ]);

        let rendered = output_lines(&state, 3, 20, &theme);

        assert_eq!(
            rendered.iter().map(line_text).collect::<Vec<_>>(),
            vec![
                "....................",
                ".........@..........",
                ".........|.........."
            ]
        );
    }

    #[test]
    fn styled_mode_renders_snapshot_ansi_colors() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.push_output_snapshot(vec!["\x1b[38;2;1;2;3mX\x1b[0m".to_string()]);

        let rendered = output_lines(&state, 1, 20, &theme);

        assert_eq!(line_text(&rendered[0]), "X");
        assert_eq!(
            rendered[0].spans[0].style.fg,
            Some(theme.readable_text_color(Color::Rgb(1, 2, 3), theme.background))
        );
    }

    #[test]
    fn wrapped_output_preserves_spaces_at_visual_line_start() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(vec![output_line(
            "alpha                    beta",
            OutputCategory::Normal,
        )]);

        let rendered = output_lines(&state, 10, 20, &theme);

        assert_eq!(
            rendered.iter().map(line_text).collect::<Vec<_>>(),
            vec!["alpha", "                    ", "beta"]
        );
    }

    #[test]
    fn wrapped_ansi_output_preserves_style() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(vec![output_line(
            "\x1b[32mgreen words continue after",
            OutputCategory::Normal,
        )]);

        let rendered = output_lines(&state, 10, 20, &theme);

        assert_eq!(rendered.len(), 2);
        assert_eq!(line_text(&rendered[0]), "green words continue");
        assert_eq!(line_text(&rendered[1]), " after");
        let green = theme.readable_text_color(Color::Green, theme.background);
        assert_eq!(rendered[0].spans[0].style.fg, Some(green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(green));
    }

    #[test]
    fn output_lines_skip_wrapping_for_transient_narrow_startup_widths() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(vec![output_line(
            "The forest path continues north toward the hill.",
            OutputCategory::Normal,
        )]);

        let rendered = output_lines(&state, 10, 10, &theme);

        assert_eq!(
            rendered.iter().map(line_text).collect::<Vec<_>>(),
            vec!["The forest path continues north toward the hill."]
        );
    }

    fn state_with_lines(lines: Vec<OutputLine>) -> AppState {
        let mut state = AppState::new(&crate::config::AppConfig::default());
        state.output = lines.into();
        state
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn scrolling_reaches_every_wrapped_row_in_both_directions() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for mode in [
            OutputDisplayMode::Styled,
            OutputDisplayMode::Plain,
            OutputDisplayMode::Debug,
        ] {
            for width in [0, 19, 20, 35, 80] {
                for height in [1, 3, 12] {
                    let mut state = state_with_lines(vec![
                        output_line("# Help heading", OutputCategory::System),
                        output_line(
                            "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty",
                            OutputCategory::System,
                        ),
                        output_line(
                            "literal snapshot must not wrap even when wider than the viewport",
                            OutputCategory::Snapshot,
                        ),
                        output_line(
                            "\x1b[32mgreen text with words that wrap across multiple rows",
                            OutputCategory::Normal,
                        ),
                        output_line("still green", OutputCategory::Normal),
                    ]);
                    state.output_view.display_mode = mode;
                    let all = rendered_rows(&state, 0, state.output.len(), width, &theme)
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();
                    let max = all.len().saturating_sub(height);
                    for hidden in 0..=max {
                        let end = all.len() - hidden;
                        assert_eq!(
                            output_lines(&state, height, width, &theme),
                            all[end.saturating_sub(height)..end],
                            "up: {mode:?}, width={width}, height={height}, hidden={hidden}"
                        );
                        let position = scroll_position(&state, height, width, &theme, 1, true);
                        (
                            state.output_view.scroll_offset,
                            state.output_view.wrapped_row_offset,
                        ) = position;
                    }
                    for hidden in (0..=max).rev() {
                        let end = all.len() - hidden;
                        assert_eq!(
                            output_lines(&state, height, width, &theme),
                            all[end.saturating_sub(height)..end]
                        );
                        let position = scroll_position(&state, height, width, &theme, 1, false);
                        (
                            state.output_view.scroll_offset,
                            state.output_view.wrapped_row_offset,
                        ) = position;
                    }
                    assert_eq!(
                        (
                            state.output_view.scroll_offset,
                            state.output_view.wrapped_row_offset
                        ),
                        (0, 0)
                    );
                }
            }
        }
    }

    #[test]
    fn single_wrapped_line_scrolls_and_new_output_preserves_the_anchor() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = state_with_lines(vec![output_line(
            "first second third fourth fifth sixth seventh eighth ninth tenth eleventh twelfth",
            OutputCategory::System,
        )]);
        let position = scroll_position(&state, 2, 20, &theme, usize::MAX, true);
        assert_eq!(position.0, 0);
        assert!(position.1 > 0);
        (
            state.output_view.scroll_offset,
            state.output_view.wrapped_row_offset,
        ) = position;
        state.output_view.follow_newest = false;
        let before = output_lines(&state, 2, 20, &theme);
        assert!(line_text(&before[0]).starts_with("first"));
        state.push_output("new output", OutputCategory::Normal);
        assert_eq!(output_lines(&state, 2, 20, &theme), before);
        state.follow_output();
        assert_eq!(state.output_view.wrapped_row_offset, 0);
        assert_eq!(
            line_text(output_lines(&state, 2, 20, &theme).last().unwrap()),
            "new output"
        );
    }

    #[test]
    fn resized_wrapped_anchor_and_empty_output_are_safe() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = state_with_lines(vec![output_line(
            "first second third fourth fifth sixth seventh eighth ninth tenth",
            OutputCategory::System,
        )]);
        (
            state.output_view.scroll_offset,
            state.output_view.wrapped_row_offset,
        ) = scroll_position(&state, 1, 20, &theme, 2, true);
        assert_eq!(output_lines(&state, 3, 200, &theme).len(), 1);
        assert_eq!(scroll_position(&state, 3, 200, &theme, 1, false), (0, 0));
        assert!(output_lines(&state, 0, 20, &theme).is_empty());
        state.clear_output();
        assert_eq!(state.output_view.wrapped_row_offset, 0);
        assert_eq!(
            scroll_position(&state, 0, 0, &theme, usize::MAX, true),
            (0, 0)
        );
        assert!(output_lines(&state, 3, 20, &theme).is_empty());
    }

    #[test]
    fn unwrapped_scroll_positions_keep_a_full_page_visible() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let state = state_with_lines(
            (0..100)
                .map(|_| output_line("line", OutputCategory::Normal))
                .collect(),
        );
        assert_eq!(max_scroll_position(&state, 10, 80, &theme), (90, 0));
        assert_eq!(scroll_position(&state, 10, 80, &theme, 5, true), (5, 0));
        assert_eq!(
            scroll_position(&state, 10, 80, &theme, usize::MAX, true),
            (90, 0)
        );
    }

    #[test]
    fn scrolled_styles_match_full_history_across_all_boundary_types() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for category in [
            OutputCategory::Normal,
            OutputCategory::Combat,
            OutputCategory::Communication,
            OutputCategory::Triggered,
            OutputCategory::Prompt,
            OutputCategory::Error,
            OutputCategory::System,
            OutputCategory::Snapshot,
        ] {
            for starts_new_output in [false, true] {
                let mut boundary = output_line("\x1b[34;1mboundary", category.clone());
                boundary.starts_new_output = starts_new_output;
                let state = state_with_lines(vec![
                    output_line("\x1b[31;43mhidden", OutputCategory::Normal),
                    boundary,
                    output_line("visible", OutputCategory::Normal),
                    output_line("\x1b[0mafter", OutputCategory::Normal),
                ]);
                let full = ansi_lines(&state.output, 0, 4, &state, &theme);
                for start in 0..4 {
                    for end in start..=4 {
                        assert_eq!(
                            ansi_lines(&state.output, start, end, &state, &theme),
                            full[start..end],
                            "{category:?}, reset={starts_new_output}, {start}..{end}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn style_only_scan_matches_rendered_parser_with_malformed_and_rgb_sequences() {
        for text in [
            "plain",
            "\x1b[31mred\x1b[broken",
            "\x1b[38;2;1;2;3mRGB\x1b[48;5;17mBG",
            "\x1b[1;3;4mstyled\x1b[22;23;24mreset",
            "\x1b[38;bogusmignored\x1b[;34mblue",
        ] {
            let mut rendered = Style::new().fg(Color::White);
            let mut hidden = rendered;
            ansi_line_with_style(text, &mut rendered, Color::White);
            let spans = AnsiParser {
                remaining: text,
                style: &mut hidden,
                default_fg: Color::White,
                theme: None,
            }
            .scan(false);
            assert!(spans.is_empty());
            assert_eq!(hidden, rendered);
        }
    }

    #[test]
    fn visible_search_markers_work_in_all_modes_with_wrapped_deque_storage() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let mut state = state_with_lines(vec![output_line("discard", OutputCategory::Normal); 4]);
        for text in ["first", "second", "third"] {
            state.output.pop_front();
            state
                .output
                .push_back(output_line(text, OutputCategory::Normal));
        }
        state.output_view.search_matches = vec![0, 2, 3];
        state.output_view.active_match = Some(1);
        state.output_view.scroll_offset = 1;
        for mode in [
            OutputDisplayMode::Styled,
            OutputDisplayMode::Plain,
            OutputDisplayMode::Debug,
        ] {
            state.output_view.display_mode = mode;
            let lines = output_lines(&state, 2, 100, &theme);
            assert!(!line_text(&lines[0]).starts_with('*'));
            assert!(line_text(&lines[1]).starts_with("> "));
            assert!(output_lines(&state, 0, 100, &theme).is_empty());
        }
    }

    #[test]
    #[ignore = "manual timing probe; run with --release --ignored --nocapture"]
    fn output_render_timing() {
        let theme = Theme::from_config(&ThemeConfig::default());
        for boundary in [false, true] {
            let mut state = state_with_lines(
                (0..10_000)
                    .map(|index| {
                        let mut line = output_line(
                            "\x1b[32mThe forest path continues north toward the hill.\x1b[0m",
                            OutputCategory::Normal,
                        );
                        line.starts_new_output = boundary && index % 20 == 0;
                        line
                    })
                    .collect(),
            );
            for mode in [
                OutputDisplayMode::Styled,
                OutputDisplayMode::Plain,
                OutputDisplayMode::Debug,
            ] {
                state.output_view.display_mode = mode;
                let start = std::time::Instant::now();
                for _ in 0..200 {
                    std::hint::black_box(output_lines(
                        std::hint::black_box(&state),
                        40,
                        100,
                        &theme,
                    ));
                }
                eprintln!(
                    "{mode:?}, boundaries={boundary}: {:?} / 200 frames",
                    start.elapsed()
                );
            }
        }
    }
}
