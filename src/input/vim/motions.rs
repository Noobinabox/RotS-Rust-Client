use std::ops::Range;

pub(super) fn previous(text: &str, at: usize) -> usize {
    text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
}

pub(super) fn next(text: &str, at: usize) -> usize {
    at + text[at..].chars().next().map_or(0, char::len_utf8)
}

pub(super) fn line(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    start..end
}

pub(super) fn lines(text: &str, at: usize, count: usize) -> Range<usize> {
    let start = line(text, at).start;
    let mut end = start;
    for _ in 0..count {
        end = line(text, end).end;
        if end == text.len() {
            break;
        }
        end += 1;
    }
    start..end
}

fn class(ch: char, big: bool) -> u8 {
    if ch.is_whitespace() {
        0
    } else if big || ch.is_alphanumeric() || ch == '_' {
        1
    } else {
        2
    }
}

fn kind(text: &str, at: usize, big: bool) -> u8 {
    text[at..].chars().next().map_or(0, |ch| class(ch, big))
}

/// Motions return insertion boundaries; the editor clamps display cursors separately.
pub(super) fn motion(text: &str, at: usize, key: char, count: usize) -> Option<(usize, bool)> {
    let mut pos = at;
    let big = key.is_ascii_uppercase();
    let mut inclusive = false;
    for _ in 0..count {
        let row = if matches!(key, 'h' | 'l' | '0' | '^' | '$') {
            line(text, pos)
        } else {
            0..0
        };
        match key {
            'h' => pos = previous(text, pos).max(row.start),
            'l' => pos = next(text, pos).min(row.end),
            '0' => pos = row.start,
            '^' => {
                pos = row.start + text[row.clone()].len()
                    - text[row].trim_start_matches(char::is_whitespace).len()
            }
            '$' => {
                pos = if row.is_empty() {
                    row.end
                } else {
                    previous(text, row.end)
                };
                inclusive = true;
            }
            'w' | 'W' => {
                let first = kind(text, pos, big);
                while pos < text.len() && kind(text, pos, big) == first {
                    pos = next(text, pos);
                }
                while pos < text.len() && kind(text, pos, big) == 0 {
                    pos = next(text, pos);
                }
            }
            'b' | 'B' => {
                if pos > 0 {
                    pos = previous(text, pos);
                }
                while pos > 0 && kind(text, pos, big) == 0 {
                    pos = previous(text, pos);
                }
                let first = kind(text, pos, big);
                while pos > 0 && kind(text, previous(text, pos), big) == first {
                    pos = previous(text, pos);
                }
            }
            'e' | 'E' => {
                if pos < text.len() {
                    pos = next(text, pos);
                }
                while pos < text.len() && kind(text, pos, big) == 0 {
                    pos = next(text, pos);
                }
                let first = kind(text, pos, big);
                while next(text, pos) < text.len() && kind(text, next(text, pos), big) == first {
                    pos = next(text, pos);
                }
                inclusive = true;
            }
            _ => return None,
        }
    }
    Some((pos, inclusive))
}

pub(super) fn change_word_end(text: &str, at: usize, big: bool, count: usize) -> usize {
    let mut end = at;
    for n in 0..count {
        if n > 0 {
            while end < text.len() && kind(text, end, big) == 0 {
                end = next(text, end);
            }
        }
        let first = kind(text, end, big);
        while end < text.len() && kind(text, end, big) == first {
            end = next(text, end);
        }
    }
    end
}

pub(super) fn find(
    text: &str,
    at: usize,
    command: char,
    target: char,
    count: usize,
) -> Option<usize> {
    let row = line(text, at);
    if matches!(command, 'f' | 't') {
        let end = text[next(text, at).min(row.end)..row.end]
            .char_indices()
            .filter(|(_, ch)| *ch == target)
            .nth(count - 1)?
            .0
            + next(text, at).min(row.end);
        Some(if command == 't' {
            previous(text, end)
        } else {
            end
        })
    } else {
        let end = text[row.start..at]
            .char_indices()
            .rev()
            .filter(|(_, ch)| *ch == target)
            .nth(count - 1)?
            .0
            + row.start;
        Some(if command == 'T' { next(text, end) } else { end })
    }
}

pub(super) fn object(
    text: &str,
    at: usize,
    object: char,
    around: bool,
    count: usize,
) -> Option<Range<usize>> {
    if matches!(object, 'w' | 'W') {
        if at == text.len() {
            return None;
        }
        let big = object == 'W';
        let first = kind(text, at, big);
        let mut start = at;
        while start > 0 && kind(text, previous(text, start), big) == first {
            start = previous(text, start);
        }
        let mut end = at;
        while end < text.len() && kind(text, end, big) == first {
            end = next(text, end);
        }
        for _ in 1..count {
            while end < text.len() && kind(text, end, big) == 0 {
                end = next(text, end);
            }
            let k = kind(text, end, big);
            while end < text.len() && kind(text, end, big) == k {
                end = next(text, end);
            }
        }
        if around {
            let original = end;
            while end < text.len() && kind(text, end, big) == 0 {
                end = next(text, end);
            }
            if original == end {
                while start > 0 && kind(text, previous(text, start), big) == 0 {
                    start = previous(text, start);
                }
            }
        }
        return Some(start..end);
    }
    let (open, close) = match object {
        '(' | ')' | 'b' => ('(', ')'),
        '[' | ']' => ('[', ']'),
        '{' | '}' | 'B' => ('{', '}'),
        '\'' => ('\'', '\''),
        '"' => ('"', '"'),
        _ => return None,
    };
    let scope = if open == close {
        line(text, at)
    } else {
        0..text.len()
    };
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    let mut escaped = false;
    for (offset, ch) in text[scope.clone()].char_indices() {
        let i = offset + scope.start;
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == close && !stack.is_empty() {
            if let Some(start) = stack.pop()
                && start <= at
                && at <= i
            {
                pairs.push(start..i + ch.len_utf8());
            }
        } else if ch == open {
            stack.push(i);
        }
    }
    pairs.sort_by_key(|range| range.len());
    let range = pairs.get(count - 1)?.clone();
    Some(if around {
        range
    } else {
        next(text, range.start)..previous(text, range.end)
    })
}
