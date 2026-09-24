use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    state::AppState,
    ui::{panels::panel, theme::Theme},
};

pub fn render_input(
    area: ratatui::layout::Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
    title: &str,
) {
    let block = panel(title, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    // Keep one cell free for the insertion cursor, including right-aligned text.
    let label = mode_label(state, inner.width);
    let label_width = label.width() as u16;
    Paragraph::new(label)
        .style(Style::new().fg(theme.accent))
        .render(
            Rect {
                x: inner.right().saturating_sub(label_width),
                width: label_width,
                ..inner
            },
            buf,
        );
    let text_area = Rect {
        width: inner.width.saturating_sub(1 + label_width),
        ..inner
    };
    let viewport = input_viewport(state, text_area.width as usize);
    let content = if state.output_view.search_active || state.vim.history_search_active() {
        Line::from(Span::styled(viewport.text, Style::new().fg(theme.warning)))
    } else if state.submitted_input_selected {
        Line::from(Span::styled(
            viewport.text,
            Style::new()
                .fg(theme.background_safe_foreground())
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else if let Some(selection) = viewport.selection {
        Line::from(vec![
            Span::raw(viewport.text[..selection.start].to_owned()),
            Span::styled(
                viewport.text[selection.clone()].to_owned(),
                Style::new().add_modifier(Modifier::REVERSED),
            ),
            Span::raw(viewport.text[selection.end..].to_owned()),
        ])
    } else {
        Line::from(Span::styled(
            viewport.text,
            Style::new().fg(theme.foreground),
        ))
    };

    Paragraph::new(content)
        .alignment(theme.alignment)
        .style(Style::new().fg(theme.foreground))
        .render(text_area, buf);
    render_completion_popup(area, buf, state, theme);
}

struct InputViewport {
    text: String,
    cursor_width: usize,
    selection: Option<std::ops::Range<usize>>,
}

fn mode_label(state: &AppState, width: u16) -> String {
    if state.input_mode != crate::config::InputMode::Vim || width == 0 {
        return String::new();
    }
    let mode = if state.output_view.search_active {
        "SEARCH"
    } else {
        state.vim.label()
    };
    if width > mode.len() as u16 + 4 {
        format!(" [{mode}]")
    } else {
        mode.chars().take(1).collect()
    }
}

const NEWLINE_MARKER: &str = "↵";

fn input_viewport(state: &AppState, width: usize) -> InputViewport {
    let (text, cursor) = if state.output_view.search_active {
        (
            format!("/{}", state.output_view.search_input),
            state.output_view.search_cursor.saturating_add(1),
        )
    } else if let Some((query, cursor, prefix)) = state.vim.history_search_prompt() {
        (format!("{prefix}{query}"), cursor + 1)
    } else if state.submitted_input_selected {
        let text = state.submitted_input.clone().unwrap_or_default();
        let cursor = text.len();
        (text, cursor)
    } else {
        (state.input.clone(), state.cursor)
    };
    // Logical multiline input stays compact: show each newline as a visible
    // marker, translating the byte cursor before computing the viewport.
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let selection = if state.input_mode == crate::config::InputMode::Vim
        && !state.output_view.search_active
        && !state.vim.history_search_active()
        && !state.submitted_input_selected
    {
        state.vim.selection(&text, cursor).map(|range| {
            let translate = |at| {
                at + text[..at].bytes().filter(|&b| b == b'\n').count() * (NEWLINE_MARKER.len() - 1)
            };
            translate(range.start)..translate(range.end)
        })
    } else {
        None
    };
    let cursor = cursor
        + text[..cursor].bytes().filter(|&byte| byte == b'\n').count()
            * (NEWLINE_MARKER.len() - '\n'.len_utf8());
    let text = text.replace('\n', NEWLINE_MARKER);
    // Use the same graphemes as Ratatui, so combining marks and emoji sequences
    // are never split or measured as unrelated scalar characters.
    let span = Span::raw(text.as_str());
    let graphemes = span.styled_graphemes(Style::default()).collect::<Vec<_>>();
    let mut bytes = 0;
    let mut cursor_width = 0usize;
    for grapheme in &graphemes {
        bytes += grapheme.symbol.len();
        if bytes > cursor {
            break;
        }
        cursor_width += grapheme.symbol.width();
    }
    let mut start = 0;
    // Scroll horizontally only when necessary to keep the cursor in view.
    for (index, grapheme) in graphemes.iter().enumerate() {
        if cursor_width <= width {
            break;
        }
        cursor_width = cursor_width.saturating_sub(grapheme.symbol.width());
        start = index + 1;
    }
    let mut displayed = String::new();
    let mut used = 0;
    for grapheme in &graphemes[start..] {
        let grapheme_width = grapheme.symbol.width();
        if used + grapheme_width > width {
            break;
        }
        displayed.push_str(grapheme.symbol);
        used += grapheme_width;
    }
    InputViewport {
        selection: selection.and_then(|range| {
            let offset: usize = graphemes[..start].iter().map(|g| g.symbol.len()).sum();
            let start = range.start.saturating_sub(offset).min(displayed.len());
            let end = range.end.saturating_sub(offset).min(displayed.len());
            (start < end).then_some(start..end)
        }),
        text: displayed,
        cursor_width,
    }
}

pub(crate) fn input_cursor_position(
    area: Rect,
    state: &AppState,
    theme: &Theme,
) -> Option<(u16, u16)> {
    if area.width < 3 || area.height < 3 {
        return None;
    }
    let label_width = mode_label(state, area.width.saturating_sub(2)).width();
    if label_width >= area.width.saturating_sub(2) as usize {
        return None;
    }
    let width = area
        .width
        .saturating_sub(3)
        .saturating_sub(label_width as u16) as usize;
    let viewport = input_viewport(state, width);
    let padding = match theme.alignment {
        ratatui::layout::Alignment::Left => 0,
        // Match Paragraph's centering convention, including odd/even widths.
        ratatui::layout::Alignment::Center => (width / 2).saturating_sub(viewport.text.width() / 2),
        ratatui::layout::Alignment::Right => width.saturating_sub(viewport.text.width()),
    };
    Some((
        area.x
            .saturating_add(1)
            .saturating_add((padding + viewport.cursor_width).min(width) as u16),
        area.y.saturating_add(1),
    ))
}

fn render_completion_popup(
    input_area: Rect,
    buf: &mut ratatui::buffer::Buffer,
    state: &AppState,
    theme: &Theme,
) {
    let completion = &state.input_completion;
    if !completion.active || completion.matches.len() < 2 || input_area.y == 0 {
        return;
    }
    let height = (completion.matches.len() as u16 + 2)
        .min(7)
        .min(input_area.y);
    let x = input_area.x.saturating_add(1);
    let y = input_area.y.saturating_sub(height);
    let available_width = buf.area.width.saturating_sub(x).min(input_area.width);
    let width = if available_width >= 12 {
        available_width.min(32)
    } else {
        available_width
    };
    let area = Rect::new(x, y, width, height);
    if area.width == 0 || area.height == 0 {
        return;
    }
    Clear.render(area, buf);
    let visible = area.height.saturating_sub(2) as usize;
    let selected = completion
        .selected
        .min(completion.matches.len().saturating_sub(1));
    let first = selected.saturating_sub(visible.saturating_sub(1));
    let lines = completion
        .matches
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, value)| {
            let style = if index == selected {
                Style::new()
                    .fg(theme.background_safe_foreground())
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.foreground)
            };
            Line::from(Span::styled(value.clone(), style))
        })
        .collect::<Vec<_>>();
    Paragraph::new(lines)
        .block(panel("Complete", theme))
        .render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use ratatui::{buffer::Buffer, layout::Alignment};

    #[test]
    fn multiline_markers_preserve_cursor_positions_and_horizontal_following() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "é\n猫\nend".into();
        for (cursor, expected_width) in [(0, 0), (2, 1), (3, 2), (6, 4), (7, 5), (10, 8)] {
            state.cursor = cursor;
            let viewport = input_viewport(&state, 20);
            assert_eq!(viewport.text, "é↵猫↵end");
            assert_eq!(viewport.cursor_width, expected_width);
        }
        state.cursor = state.input.len();
        assert_eq!(input_viewport(&state, 3).text, "end");
        assert!(input_viewport(&state, 0).text.is_empty());
    }

    #[test]
    fn aligned_cursor_matches_rendered_text_for_parities_unicode_and_end() {
        for width in [7, 8, 79, 80] {
            for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
                for text in [
                    "abc",
                    "abcd",
                    "é界!",
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa界",
                ] {
                    for cursor in [0, text.len()] {
                        let mut state = AppState::new(&AppConfig::default());
                        state.input = text.into();
                        state.cursor = cursor;
                        let mut theme = Theme::from_config(&AppConfig::default().colors);
                        theme.alignment = alignment;
                        let area = Rect::new(0, 0, width, 3);
                        let mut buf = Buffer::empty(area);
                        render_input(area, &mut buf, &state, &theme, "Input");
                        let (x, y) = input_cursor_position(area, &state, &theme).unwrap();
                        assert!(x > 0 && x < area.right() - 1);
                        if cursor == 0 {
                            assert_eq!(
                                buf[(x, y)].symbol(),
                                &text[..text.chars().next().unwrap().len_utf8()],
                                "{width} {alignment:?} {text}"
                            );
                        } else {
                            assert_eq!(
                                buf[(x, y)].symbol(),
                                " ",
                                "End: {width} {alignment:?} {text}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn viewport_keeps_combining_marks_and_joined_emoji_intact() {
        let mut state = AppState::new(&AppConfig::default());
        for (text, width, expected) in [
            ("👨‍👩‍👧‍👦", 3, "👨‍👩‍👧‍👦"),
            ("xxxxx👨‍👩‍👧‍👦", 3, "x👨‍👩‍👧‍👦"),
            ("abcde\u{301}", 1, "e\u{301}"),
        ] {
            state.input = text.into();
            state.cursor = text.len();
            let viewport = input_viewport(&state, width);
            assert_eq!(viewport.text, expected);
            assert_eq!(viewport.cursor_width, expected.width());
            let area = Rect::new(0, 0, width as u16 + 3, 3);
            let mut buf = Buffer::empty(area);
            let mut theme = Theme::from_config(&AppConfig::default().colors);
            theme.alignment = ratatui::layout::Alignment::Right;
            render_input(area, &mut buf, &state, &theme, "Input");
            let position = input_cursor_position(area, &state, &theme).unwrap();
            assert_eq!(buf[position].symbol(), " ");
            assert_eq!(state.input, text);
        }
    }

    #[test]
    fn search_and_selected_submission_reserve_an_end_cursor_cell() {
        let mut state = AppState::new(&AppConfig::default());
        let mut theme = Theme::from_config(&AppConfig::default().colors);
        theme.alignment = ratatui::layout::Alignment::Right;
        let area = Rect::new(0, 0, 20, 3);
        state.output_view.search_active = true;
        state.output_view.search_input = "é界".into();
        state.output_view.search_cursor = "é界".len();
        for search in [true, false] {
            state.output_view.search_active = search;
            state.submitted_input_selected = !search;
            state.submitted_input = Some("é界".into());
            let mut buf = Buffer::empty(area);
            render_input(area, &mut buf, &state, &theme, "Input");
            let position = input_cursor_position(area, &state, &theme).unwrap();
            assert_eq!(position, (18, 1));
            assert_eq!(buf[position].symbol(), " ");
        }
    }

    #[test]
    fn history_query_renders_without_changing_the_draft() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut config = AppConfig::default();
        config.terminal.input_mode = crate::config::InputMode::Vim;
        let mut state = AppState::new(&config);
        state.input = "untouched".into();
        state.cursor = 2;
        state.vim.mode = crate::input::vim::Mode::Normal;
        crate::input::handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        crate::input::insert_paste(&mut state, "猫", false).unwrap();
        let theme = Theme::from_config(&config.colors);
        let area = Rect::new(0, 0, 30, 3);
        let mut buf = Buffer::empty(area);
        render_input(area, &mut buf, &state, &theme, "Input");
        assert_eq!(buf[(1, 1)].symbol(), "/");
        assert_eq!(buf[(2, 1)].symbol(), "猫");
        assert_eq!(input_cursor_position(area, &state, &theme), Some((4, 1)));
        let row: String = (0..30).map(|x| buf[(x, 1)].symbol()).collect();
        assert!(row.contains("H-SEARCH"));
        assert!(!row.contains("untouched"));
        assert_eq!(state.input, "untouched");
        assert_eq!(state.cursor, 2);
        crate::input::insert_paste(&mut state, &"猫".repeat(30), false).unwrap();
        for width in 4..40 {
            let area = Rect { width, ..area };
            let mut buf = Buffer::empty(area);
            render_input(area, &mut buf, &state, &theme, "Input");
            let (x, y) = input_cursor_position(area, &state, &theme).unwrap();
            assert!(x < area.right() - 1 - mode_label(&state, width - 2).width() as u16);
            assert_eq!(buf[(x, y)].symbol(), " ");
        }
    }

    #[test]
    fn vim_indicator_stays_right_without_shifting_input_or_cursor() {
        let mut config = AppConfig::default();
        config.terminal.input_mode = crate::config::InputMode::Vim;
        let mut state = AppState::new(&config);
        state.input = "look".into();
        state.cursor = 2;
        let area = Rect::new(3, 2, 30, 3);
        for border in [
            crate::config::PanelBorderStyle::None,
            crate::config::PanelBorderStyle::Rounded,
        ] {
            let mut theme = Theme::from_config(&config.colors);
            theme.border_style = border;
            for mode in [
                crate::input::vim::Mode::Insert,
                crate::input::vim::Mode::Normal,
            ] {
                state.vim.mode = mode;
                let mut buf = Buffer::empty(Rect::new(0, 0, 40, 8));
                render_input(area, &mut buf, &state, &theme, "Input");
                assert_eq!(buf[(area.x + 1, area.y + 1)].symbol(), "l");
                assert_eq!(
                    input_cursor_position(area, &state, &theme),
                    Some((area.x + 3, area.y + 1))
                );
                assert_eq!(buf[(area.right() - 2, area.y + 1)].symbol(), "]");
            }
            state.input = "long command with Unicode 猫 and more text".into();
            state.cursor = state.input.len();
            for alignment in [
                ratatui::layout::Alignment::Left,
                ratatui::layout::Alignment::Center,
                ratatui::layout::Alignment::Right,
            ] {
                theme.alignment = alignment;
                for width in 4..40 {
                    let area = Rect { width, ..area };
                    let mut buf = Buffer::empty(Rect::new(0, 0, 50, 8));
                    render_input(area, &mut buf, &state, &theme, "Input");
                    let (x, y) = input_cursor_position(area, &state, &theme).unwrap();
                    let label_start =
                        area.right() - 1 - mode_label(&state, width - 2).width() as u16;
                    assert!(x < label_start);
                    assert_eq!(buf[(x, y)].symbol(), " ");
                }
            }
            state.input = "look".into();
            state.cursor = 2;
        }
    }

    #[test]
    fn vim_mode_and_selection_render_with_small_and_borderless_panes() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut config = AppConfig::default();
        config.terminal.input_mode = crate::config::InputMode::Vim;
        let mut state = AppState::new(&config);
        state.input = "abc 猫 e\u{301}".into();
        state.cursor = 0;
        state.vim.mode = crate::input::vim::Mode::Normal;
        for ch in "vl".chars() {
            crate::input::handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        for border in [
            crate::config::PanelBorderStyle::None,
            crate::config::PanelBorderStyle::Rounded,
        ] {
            for alignment in [
                ratatui::layout::Alignment::Left,
                ratatui::layout::Alignment::Center,
                ratatui::layout::Alignment::Right,
            ] {
                let mut theme = Theme::from_config(&config.colors);
                theme.border_style = border;
                theme.alignment = alignment;
                for width in 0..40 {
                    let area = Rect::new(0, 0, width, 3);
                    let mut buf = Buffer::empty(area);
                    render_input(area, &mut buf, &state, &theme, "Input");
                    if let Some((x, y)) = input_cursor_position(area, &state, &theme) {
                        assert!(x < area.right() - 1 && y < area.bottom());
                    }
                    if width >= 20 {
                        let row: String = (0..width).map(|x| buf[(x, 1)].symbol()).collect();
                        assert!(row.contains("VISUAL"));
                        assert!(
                            (0..width).any(|x| buf[(x, 1)].modifier.contains(Modifier::REVERSED))
                        );
                    }
                }
            }
        }
    }
}
