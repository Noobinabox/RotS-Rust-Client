use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget},
};

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
    let content = if state.output_view.search_active {
        Line::from(Span::styled(
            format!("/{}", state.output_view.search_input),
            Style::new().fg(theme.warning),
        ))
    } else if state.submitted_input_selected {
        Line::from(Span::styled(
            state.submitted_input.clone().unwrap_or_default(),
            Style::new()
                .fg(theme.background_safe_foreground())
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(
            state.input.clone(),
            Style::new().fg(theme.foreground),
        ))
    };

    Paragraph::new(content)
        .style(Style::new().fg(theme.foreground))
        .block(panel(title, theme))
        .render(area, buf);
    render_completion_popup(area, buf, state, theme);
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
