//! Render Markdown tables without changing the logical scrollback line anchors.
use std::collections::BTreeMap;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use unicode_width::UnicodeWidthStr;

use super::{apply_output_style, inline_code_spans, mark_search_match, styled_tokens};
use crate::{
    state::{AppState, OutputCategory, plain_text},
    ui::theme::Theme,
};

fn cells(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    if !text.contains('|') {
        return None;
    }
    let mut result = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cell.push('|');
            }
            '|' => {
                result.push(cell.trim().to_owned());
                cell.clear();
            }
            _ => cell.push(ch),
        }
    }
    result.push(cell.trim().to_owned());
    if text.starts_with('|') {
        result.remove(0);
    }
    if text.ends_with('|') && result.last().is_some_and(String::is_empty) {
        result.pop();
    }
    (!result.is_empty()).then_some(result)
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

fn separator(cell: &str) -> Option<Align> {
    let dashes = cell.trim_matches(':');
    if dashes.len() < 3 || !dashes.chars().all(|c| c == '-') {
        return None;
    }
    Some(match (cell.starts_with(':'), cell.ends_with(':')) {
        (true, true) => Align::Center,
        (_, true) => Align::Right,
        _ => Align::Left,
    })
}

fn border(widths: &[usize], left: char, join: char, right: char, theme: &Theme) -> Line<'static> {
    let segments = widths.iter().map(|w| "─".repeat(w + 2)).collect::<Vec<_>>();
    Line::from(Span::styled(
        format!("{left}{}{right}", segments.join(&join.to_string())),
        Style::new().fg(theme.border),
    ))
}

fn wrap_cell(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut used = 0;
    for span in line.spans {
        for token in styled_tokens(&span.content, span.style) {
            let token_width = token.text.width();
            if token_width <= width {
                if used + token_width > width && !current.is_empty() {
                    rows.push(Line::from(std::mem::take(&mut current)));
                    used = 0;
                }
                current.push(Span::styled(token.text, token.style));
                used += token_width;
            } else {
                let span = Span::styled(token.text, token.style);
                for grapheme in span.styled_graphemes(Style::default()) {
                    let size = grapheme.symbol.width();
                    if used + size > width && !current.is_empty() {
                        rows.push(Line::from(std::mem::take(&mut current)));
                        used = 0;
                    }
                    current.push(Span::styled(grapheme.symbol.to_owned(), grapheme.style));
                    used += size;
                }
            }
        }
    }
    rows.push(Line::from(current));
    rows
}

pub(super) fn render(
    state: &AppState,
    start: usize,
    end: usize,
    width: usize,
    theme: &Theme,
) -> BTreeMap<usize, Vec<Line<'static>>> {
    let mut rendered = BTreeMap::new();
    if start == end || width == 0 {
        return rendered;
    }
    let lines = &state.output;
    let row = |index: usize| {
        let line = lines.get(index)?;
        (line.category == OutputCategory::System)
            .then(|| cells(&plain_text(&line.normalized)))
            .flatten()
    };
    let mut cursor = start;
    // Look behind a partial viewport to recover the header and consistent widths.
    while cursor > 0 && row(cursor - 1).is_some() {
        cursor -= 1;
    }
    while cursor < end {
        let header_index = cursor;
        cursor += 1;
        let Some(header) = row(header_index) else {
            continue;
        };
        let Some(delimiter) = row(cursor) else {
            continue;
        };
        if header.len() != delimiter.len() {
            continue;
        }
        let Some(alignments) = delimiter
            .iter()
            .map(|c| separator(c))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        // Do not interpret examples inside fenced system-output code blocks.
        let mut block_start = header_index;
        while block_start > 0 && lines[block_start - 1].category == OutputCategory::System {
            block_start -= 1;
        }
        let mut fence: Option<(char, usize)> = None;
        for line in lines.range(block_start..header_index) {
            let text = line.normalized.trim_start();
            let Some(marker @ ('`' | '~')) = text.chars().next() else {
                continue;
            };
            let length = text.chars().take_while(|ch| *ch == marker).count();
            if length < 3 {
                continue;
            }
            if let Some((opening, minimum)) = fence {
                if marker == opening && length >= minimum && text[length..].trim().is_empty() {
                    fence = None;
                }
            } else if marker != '`' || !text[length..].contains('`') {
                fence = Some((marker, length));
            }
        }
        if fence.is_some() {
            continue;
        }
        let mut table_end = header_index + 2;
        while row(table_end).is_some_and(|cells| cells.len() == header.len()) {
            table_end += 1;
        }
        cursor = table_end;
        // Two cells per column allow wide glyphs; smaller panes retain raw text.
        if width < header.len().saturating_mul(5).saturating_add(1) {
            continue;
        }
        let mut widths = vec![2; header.len()];
        for index in (header_index..table_end).filter(|i| *i != header_index + 1) {
            if let Some(cells) = row(index) {
                for (column, cell) in cells.iter().enumerate() {
                    widths[column] = widths[column].max(
                        Line::from(inline_code_spans(cell, theme))
                            .width()
                            .min(width),
                    );
                }
            }
        }
        // Reserve the search marker margin for every row so columns stay aligned.
        let has_matches = state
            .output_view
            .search_matches
            .iter()
            .any(|i| (header_index..table_end).contains(i));
        let available = width - 3 * widths.len() - 1 - if has_matches { 2 } else { 0 };
        if available < 2 * widths.len() {
            continue;
        }
        while widths.iter().sum::<usize>() > available {
            if let Some((index, _)) = widths.iter().enumerate().max_by_key(|(_, w)| **w) {
                widths[index] -= 1;
            }
        }
        for (index, source) in lines
            .iter()
            .enumerate()
            .take(table_end.min(end))
            .skip(header_index.max(start))
        {
            let mut rows = Vec::new();
            if index == header_index {
                rows.push(border(&widths, '┌', '┬', '┐', theme));
            }
            if index == header_index + 1 {
                rows.push(border(&widths, '├', '┼', '┤', theme));
            } else if let Some(cells) = row(index) {
                let mut cell_theme = *theme;
                if index == header_index {
                    cell_theme.foreground = theme.title;
                }
                let wrapped = cells
                    .iter()
                    .zip(&widths)
                    .map(|(cell, w)| {
                        wrap_cell(Line::from(inline_code_spans(cell, &cell_theme)), *w)
                    })
                    .collect::<Vec<_>>();
                let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
                for y in 0..height {
                    let mut spans = vec![Span::styled("│", Style::new().fg(theme.border))];
                    for (column, cell_rows) in wrapped.iter().enumerate() {
                        let cell = cell_rows.get(y).cloned().unwrap_or_default();
                        let padding = widths[column].saturating_sub(cell.width());
                        let before = match alignments[column] {
                            Align::Left => 0,
                            Align::Center => padding / 2,
                            Align::Right => padding,
                        };
                        spans.push(Span::raw(" ".repeat(1 + before)));
                        spans.extend(cell.spans.into_iter().map(|mut span| {
                            if index == header_index {
                                span.style = span.style.add_modifier(Modifier::BOLD);
                            }
                            span
                        }));
                        spans.push(Span::raw(" ".repeat(1 + padding - before)));
                        spans.push(Span::styled("│", Style::new().fg(theme.border)));
                    }
                    rows.push(Line::from(spans));
                }
            }
            if index + 1 == table_end {
                rows.push(border(&widths, '└', '┴', '┘', theme));
            }
            let matches = state
                .output_view
                .search_matches
                .binary_search(&index)
                .is_ok();
            for line in &mut rows {
                *line = apply_output_style(std::mem::take(line), source.style.as_ref(), theme);
                if matches {
                    *line = mark_search_match(std::mem::take(line), index, state, theme);
                } else if has_matches {
                    line.spans.insert(0, Span::raw("  "));
                }
                *line = std::mem::take(line).left_aligned();
            }
            rendered.insert(index, rows);
        }
    }
    rendered
}
