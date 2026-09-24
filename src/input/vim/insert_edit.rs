use super::{Snapshot, motions::next};
use crate::state::AppState;

/// Freeze the result of an insertion session, not completion/history key presses.
#[derive(Debug, Clone)]
pub(super) struct Splice {
    offset: isize,
    removed: usize,
    inserted: String,
    cursor: isize,
}

fn shift(text: &str, from: usize, count: isize) -> usize {
    if count >= 0 {
        text[from..]
            .char_indices()
            .nth(count as usize)
            .map_or(text.len(), |(i, _)| from + i)
    } else {
        text[..from]
            .char_indices()
            .rev()
            .nth(count.unsigned_abs() - 1)
            .map_or(0, |(i, _)| i)
    }
}

impl Splice {
    pub(super) fn between(before: &Snapshot, state: &AppState) -> Self {
        let prefix = before
            .text
            .chars()
            .zip(state.input.chars())
            .take_while(|(a, b)| a == b)
            .count()
            .min(before.text[..before.cursor].chars().count())
            .min(state.input[..state.cursor].chars().count());
        let start = shift(&before.text, 0, prefix as isize);
        let new_start = shift(&state.input, 0, prefix as isize);
        let suffix = before.text[start..]
            .chars()
            .rev()
            .zip(state.input[new_start..].chars().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let end = shift(&before.text, before.text.len(), -(suffix as isize));
        let new_end = shift(&state.input, state.input.len(), -(suffix as isize));
        Self {
            offset: prefix as isize - before.text[..before.cursor].chars().count() as isize,
            removed: before.text[start..end].chars().count(),
            inserted: state.input[new_start..new_end].to_owned(),
            cursor: state.input[..state.cursor].chars().count() as isize - prefix as isize,
        }
    }

    pub(super) fn apply(&self, state: &mut AppState) -> bool {
        let start = shift(&state.input, state.cursor, self.offset);
        let mut end = start;
        for _ in 0..self.removed {
            end = next(&state.input, end);
        }
        let mut result = state.input.clone();
        result.replace_range(start..end, &self.inserted);
        if super::within_limits(&result) {
            state.cursor = shift(&result, start, self.cursor);
            state.input = result;
            true
        } else {
            false
        }
    }
}
