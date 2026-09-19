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
    let all_lines = state.output.iter().cloned().collect::<Vec<_>>();
    let (visible_start, visible_end) = visible_bounds(
        all_lines.len(),
        inner_height,
        state.output_view.scroll_offset,
    );
    let literal_lines = all_lines[visible_start..visible_end]
        .iter()
        .map(|line| line.category == OutputCategory::Snapshot)
        .collect::<Vec<_>>();
    let lines = match state.output_view.display_mode {
        OutputDisplayMode::Styled => {
            ansi_lines(all_lines, visible_start, visible_end, state, theme)
        }
        OutputDisplayMode::Plain => {
            plain_lines(all_lines, visible_start, visible_end, state, theme, false)
        }
        OutputDisplayMode::Debug => {
            plain_lines(all_lines, visible_start, visible_end, state, theme, true)
        }
    };
    if inner_width < MIN_OUTPUT_WRAP_WIDTH {
        return lines;
    }
    wrap_visible_lines(lines, literal_lines, inner_width, inner_height)
}

fn visible_bounds(total: usize, height: usize, scroll_offset: usize) -> (usize, usize) {
    let max_offset = total.saturating_sub(height);
    let visible_end = total.saturating_sub(scroll_offset.min(max_offset));
    let visible_start = visible_end.saturating_sub(height);
    (visible_start, visible_end)
}

fn ansi_lines(
    lines: Vec<OutputLine>,
    visible_start: usize,
    visible_end: usize,
    state: &AppState,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let default_fg = theme.foreground;
    let mut mud_style = Style::new().fg(default_fg);
    let mut rendered = Vec::with_capacity(lines.len().saturating_sub(visible_start));

    for (index, line) in lines.into_iter().enumerate() {
        if line.starts_new_output {
            mud_style = Style::new().fg(default_fg);
        }
        let rendered_line = match line.category {
            OutputCategory::Snapshot => ansi_line(&line.normalized, default_fg),
            OutputCategory::Normal
            | OutputCategory::Combat
            | OutputCategory::Communication
            | OutputCategory::Triggered => {
                ansi_line_with_style(&line.normalized, &mut mud_style, default_fg)
            }
            OutputCategory::Prompt => {
                let rendered_line =
                    ansi_line_with_style(&line.normalized, &mut mud_style, default_fg);
                mud_style = Style::new().fg(default_fg);
                rendered_line
            }
            OutputCategory::System => markdown_system_line(&line.normalized, theme),
            OutputCategory::Error => {
                mud_style = Style::new().fg(default_fg);
                ansi_line(&line.normalized, color_for(line.category.clone(), theme))
            }
        };
        if index >= visible_start && index < visible_end {
            let rendered_line = apply_output_style(rendered_line, line.style.as_ref());
            rendered.push(if line.category == OutputCategory::Snapshot {
                rendered_line
            } else {
                mark_search_match(rendered_line, index, state, theme)
            });
        }
    }

    rendered
}

fn markdown_system_line(raw: &str, theme: &Theme) -> Line<'static> {
    let text = plain_text(raw);
    if text.trim().is_empty() {
        return Line::from("");
    }
    if let Some(title) = text.strip_prefix("# ") {
        return Line::from(Span::styled(
            title.to_string(),
            Style::new().fg(theme.title).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(title) = text.strip_prefix("## ") {
        return Line::from(Span::styled(
            title.to_string(),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(item) = text.strip_prefix("- ") {
        let mut spans = vec![Span::styled("• ", Style::new().fg(theme.accent))];
        spans.extend(inline_code_spans(item, theme));
        return Line::from(spans);
    }
    Line::from(inline_code_spans(&text, theme))
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
    lines: Vec<OutputLine>,
    visible_start: usize,
    visible_end: usize,
    state: &AppState,
    theme: &Theme,
    debug: bool,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .enumerate()
        .skip(visible_start)
        .take(visible_end.saturating_sub(visible_start))
        .map(|(index, line)| {
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
            );
            if line.category == OutputCategory::Snapshot {
                rendered_line
            } else {
                mark_search_match(rendered_line, index, state, theme)
            }
        })
        .collect()
}

fn apply_output_style(line: Line<'static>, style: Option<&OutputStyle>) -> Line<'static> {
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
    let Some(match_position) = state
        .output_view
        .search_matches
        .iter()
        .position(|match_index| *match_index == index)
    else {
        return line;
    };
    let is_active = state.output_view.active_match == Some(match_position);
    let marker = if is_active { ">" } else { "*" };
    let marker_style = if is_active {
        Style::new()
            .fg(theme.background_safe_foreground())
            .bg(theme.warning)
    } else {
        Style::new().fg(theme.warning)
    };
    let mut spans = vec![Span::styled(marker, marker_style), Span::raw(" ")];
    spans.extend(line.spans);
    Line::from(spans)
}

fn wrap_visible_lines(
    lines: Vec<Line<'static>>,
    literal_lines: Vec<bool>,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let wrapped = lines
        .into_iter()
        .zip(literal_lines)
        .flat_map(|(line, literal)| {
            if literal {
                vec![line]
            } else {
                wrap_line(line, width)
            }
        })
        .collect::<Vec<_>>();
    let start = wrapped.len().saturating_sub(height);
    wrapped.into_iter().skip(start).collect()
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

fn ansi_line(value: &str, default_fg: Color) -> Line<'static> {
    let mut style = Style::new().fg(default_fg);
    ansi_line_with_style(value, &mut style, default_fg)
}

fn ansi_line_with_style(value: &str, style: &mut Style, default_fg: Color) -> Line<'static> {
    let mut parser = AnsiParser {
        remaining: value,
        style,
        default_fg,
    };
    Line::from(parser.parse())
}

struct AnsiParser<'a> {
    remaining: &'a str,
    style: &'a mut Style,
    default_fg: Color,
}

impl<'a> AnsiParser<'a> {
    fn parse(&mut self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        while let Some(index) = self.remaining.find("\x1b[") {
            let (plain, rest) = self.remaining.split_at(index);
            if !plain.is_empty() {
                spans.push(Span::styled(plain.to_string(), *self.style));
            }
            let Some(end) = rest.find('m') else {
                self.remaining = "";
                return spans;
            };
            self.apply_sgr(&rest[2..end]);
            self.remaining = &rest[end + 1..];
        }
        if !self.remaining.is_empty() {
            spans.push(Span::styled(self.remaining.to_string(), *self.style));
        }
        spans
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
        let rendered = ansi_lines(
            state.output.iter().cloned().collect(),
            0,
            state.output.len(),
            &state,
            &theme,
        );

        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(Color::Green));
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
        let rendered = ansi_lines(
            state.output.iter().cloned().collect(),
            0,
            state.output.len(),
            &state,
            &theme,
        );

        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(theme.foreground));
        assert_eq!(rendered[2].spans[0].style.fg, Some(Color::Green));
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

        let state = state_with_lines(Vec::new());
        let rendered = ansi_lines(lines, 0, 3, &state, &theme);

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

        let state = state_with_lines(Vec::new());
        let rendered = ansi_lines(lines, 0, 3, &state, &theme);

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
    fn ansi_lines_carry_style_from_lines_above_visible_window() {
        let theme = Theme::from_config(&ThemeConfig::default());
        let lines = vec![
            output_line("\x1b[32mhidden", OutputCategory::Normal),
            output_line("visible", OutputCategory::Normal),
        ];

        let state = state_with_lines(lines);
        let rendered = ansi_lines(
            state.output.iter().cloned().collect(),
            1,
            state.output.len(),
            &state,
            &theme,
        );

        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
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
        let rendered = ansi_lines(
            state.output.iter().cloned().collect(),
            0,
            state.output.len(),
            &state,
            &theme,
        );

        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(Color::Green));
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
        let rendered = ansi_lines(
            state.output.iter().cloned().collect(),
            0,
            state.output.len(),
            &state,
            &theme,
        );

        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
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
        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Rgb(1, 2, 3)));
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
        assert_eq!(rendered[0].spans[0].style.fg, Some(Color::Green));
        assert_eq!(rendered[1].spans[0].style.fg, Some(Color::Green));
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
    fn visible_bounds_keep_scrolled_window_to_height() {
        assert_eq!(visible_bounds(100, 10, 0), (90, 100));
        assert_eq!(visible_bounds(100, 10, 5), (85, 95));
        assert_eq!(visible_bounds(100, 10, 999), (0, 10));
        assert_eq!(visible_bounds(3, 10, 0), (0, 3));
        assert_eq!(visible_bounds(3, 10, 5), (0, 3));
    }
}
