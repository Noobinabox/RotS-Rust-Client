use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{state::plain_text, ui::theme::Theme};

#[cfg(test)]
pub(super) fn markdown_system_line(raw: &str, theme: &Theme) -> Line<'static> {
    MarkdownState::default().render(raw, theme)
}

#[derive(Default)]
pub(super) struct MarkdownState {
    fence: Option<(char, usize)>,
}

impl MarkdownState {
    pub(super) fn render(&mut self, raw: &str, theme: &Theme) -> Line<'static> {
        let text = plain_text(raw);
        let trimmed = text.trim_start();
        if let Some((marker, minimum)) = self.fence {
            let length = trimmed
                .chars()
                .take_while(|character| *character == marker)
                .count();
            if length >= minimum && trimmed[length..].trim().is_empty() {
                self.fence = None;
            }
            return Line::from(Span::styled(text, Style::new().fg(theme.warning)));
        }
        let marker = trimmed.chars().next();
        let length = marker
            .map(|marker| {
                trimmed
                    .chars()
                    .take_while(|character| *character == marker)
                    .count()
            })
            .unwrap_or(0);
        if matches!(marker, Some('`' | '~')) && length >= 3 {
            self.fence = Some((marker.unwrap(), length));
            return Line::from(Span::styled(text, Style::new().fg(theme.accent)));
        }
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
        if is_markdown_horizontal_rule(&text) {
            return Line::from(Span::styled(
                "─".repeat(text.chars().count()),
                Style::new().fg(theme.border),
            ));
        }
        let (indent, list_text) = text.split_at(text.len() - text.trim_start().len());
        if let Some(item) = markdown_ordered_item(list_text) {
            return prefixed_line(&format!("{indent}• "), item, theme);
        }
        if let Some(quote) = text.strip_prefix("> ").or_else(|| text.strip_prefix('>')) {
            return prefixed_line("│ ", quote.trim_start(), theme);
        }
        if let Some(item) = list_text.strip_prefix("- ") {
            return prefixed_line(&format!("{indent}• "), item, theme);
        }
        Line::from(markdown_inline_spans(&text, theme))
    }
}

fn prefixed_line(prefix: &str, text: &str, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(
        prefix.to_string(),
        Style::new().fg(theme.accent),
    )];
    spans.extend(markdown_inline_spans(text, theme));
    Line::from(spans)
}

fn is_markdown_horizontal_rule(text: &str) -> bool {
    let compact = text.replace(' ', "");
    compact.len() >= 3
        && compact
            .chars()
            .all(|character| matches!(character, '-' | '*' | '_'))
}

fn markdown_ordered_item(text: &str) -> Option<&str> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && text.as_bytes().get(digits..digits + 2) == Some(b". "))
        .then(|| &text[digits + 2..])
}

pub(super) fn markdown_heading(text: &str) -> Option<(usize, &str)> {
    let level = text.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) || text.as_bytes().get(level) != Some(&b' ') {
        return None;
    }
    Some((level, &text[level + 1..]))
}

pub(super) fn markdown_inline_spans(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let marker = next_markdown_marker(rest);
        if let Some((index, label, end)) = find_markdown_link(rest)
            && marker.is_none_or(|(marker_index, _, _)| index < marker_index)
        {
            spans.extend(markdown_inline_spans(&rest[..index], theme));
            spans.push(Span::styled(
                label.to_string(),
                Style::new()
                    .fg(theme.accent)
                    .add_modifier(Modifier::UNDERLINED),
            ));
            rest = &rest[end..];
            continue;
        }
        let Some((index, marker, marker_len)) = marker else {
            spans.push(Span::styled(
                rest.to_string(),
                Style::new().fg(theme.foreground),
            ));
            break;
        };
        if index > 0 {
            spans.push(Span::styled(
                rest[..index].to_string(),
                Style::new().fg(theme.foreground),
            ));
        }
        let content_start = index + marker_len;
        let Some(end) = rest[content_start..].find(marker) else {
            spans.push(Span::styled(
                rest[index..].to_string(),
                Style::new().fg(theme.foreground),
            ));
            break;
        };
        let content_end = content_start + end;
        let modifier = match marker_len {
            1 if marker == "`" => Modifier::empty(),
            1 => Modifier::ITALIC,
            2 => Modifier::BOLD,
            _ => unreachable!(),
        };
        let color = if marker == "`" {
            theme.warning
        } else {
            theme.foreground
        };
        let modifier = if marker == "~~" {
            Modifier::CROSSED_OUT
        } else {
            modifier
        };
        spans.push(Span::styled(
            rest[content_start..content_end].to_string(),
            Style::new().fg(color).add_modifier(modifier),
        ));
        rest = &rest[content_end + marker_len..];
    }
    spans
}

fn find_markdown_link(text: &str) -> Option<(usize, &str, usize)> {
    let start = text.find('[')?;
    let label_end = text[start + 1..].find("](")? + start + 1;
    let url_end = text[label_end + 2..].find(')')? + label_end + 2;
    (!text[start + 1..label_end].is_empty() && !text[label_end + 2..url_end].is_empty())
        .then(|| (start, &text[start + 1..label_end], url_end + 1))
}

fn next_markdown_marker(text: &str) -> Option<(usize, &str, usize)> {
    for (index, _) in text.char_indices() {
        let rest = &text[index..];
        let marker = if rest.starts_with("**") || rest.starts_with("__") || rest.starts_with("~~") {
            &rest[..2]
        } else if rest.starts_with('`') || rest.starts_with('*') || rest.starts_with('_') {
            &rest[..1]
        } else {
            continue;
        };
        if marker != "_"
            || index == 0
            || !text[..index]
                .chars()
                .last()
                .is_some_and(char::is_alphanumeric)
        {
            return Some((index, marker, marker.len()));
        }
    }
    None
}
