//! Vim-style history search is separate from both the draft and output search.
use super::*;
use regex::{Regex, RegexBuilder};

const MAX_QUERY_BYTES: usize = 4096;
const MAX_HISTORY_ENTRIES: usize = 100_000;

#[derive(Debug, Clone)]
pub(super) struct LastSearch {
    pattern: String,
    regex: Regex,
    older: bool,
}

#[derive(Debug, Clone)]
struct Prompt {
    text: String,
    cursor: usize,
    older: bool,
    count: usize,
}

#[derive(Debug, Default, Clone)]
pub(super) struct HistorySearch {
    prompt: Option<Prompt>,
    pub(super) last: Option<LastSearch>,
    pub(super) accepted_enter: bool,
}

impl VimEditor {
    /// A held acceptance key must not become a send or keypad macro afterward.
    pub fn suppress_history_enter_repeat(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Enter {
            return false;
        }
        match key.kind {
            crossterm::event::KeyEventKind::Press => {
                self.history_search.accepted_enter = false;
                false
            }
            crossterm::event::KeyEventKind::Repeat => {
                self.history_search.accepted_enter || self.history_search_active()
            }
            crossterm::event::KeyEventKind::Release => false,
        }
    }
    pub fn history_search_active(&self) -> bool {
        self.history_search.prompt.is_some()
    }

    pub fn history_search_prompt(&self) -> Option<(&str, usize, char)> {
        self.history_search
            .prompt
            .as_ref()
            .map(|p| (p.text.as_str(), p.cursor, if p.older { '/' } else { '?' }))
    }

    pub(super) fn start_history_search(&mut self, older: bool, count: usize) {
        self.cancel_pending();
        self.history_search.prompt = Some(Prompt {
            text: String::new(),
            cursor: 0,
            older,
            count,
        });
    }

    pub(super) fn paste_history_search(&mut self, text: &str) -> Result<(), &'static str> {
        let Some(prompt) = self.history_search.prompt.as_mut() else {
            return Ok(());
        };
        if prompt.text.len().saturating_add(text.len()) > MAX_QUERY_BYTES {
            return Err("History search rejected: maximum pattern size is 4 KiB.");
        }
        prompt.text.insert_str(prompt.cursor, text);
        prompt.cursor += text.len();
        Ok(())
    }

    pub(super) fn history_search_key(&mut self, state: &mut AppState, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.history_search.prompt = None;
            return;
        }
        if key.code == KeyCode::Enter && key.modifiers.is_empty() {
            self.history_search.accepted_enter = true;
            self.accept_history_search(state);
            return;
        }
        let Some(prompt) = self.history_search.prompt.as_mut() else {
            return;
        };
        if let Some(edit) = crate::input::word_edit::WordEdit::from_key(key) {
            edit.apply(&mut prompt.text, &mut prompt.cursor);
            return;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('u') {
            prompt.text.drain(..prompt.cursor);
            prompt.cursor = 0;
            return;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        match key.code {
            KeyCode::Left => prompt.cursor = previous(&prompt.text, prompt.cursor),
            KeyCode::Right => prompt.cursor = next(&prompt.text, prompt.cursor),
            KeyCode::Home => prompt.cursor = 0,
            KeyCode::End => prompt.cursor = prompt.text.len(),
            KeyCode::Backspace if prompt.cursor > 0 => {
                let from = previous(&prompt.text, prompt.cursor);
                prompt.text.drain(from..prompt.cursor);
                prompt.cursor = from;
            }
            KeyCode::Delete if prompt.cursor < prompt.text.len() => {
                prompt.text.remove(prompt.cursor);
            }
            KeyCode::Char(ch)
                if !ch.is_control() && prompt.text.len() + ch.len_utf8() <= MAX_QUERY_BYTES =>
            {
                prompt.text.insert(prompt.cursor, ch);
                prompt.cursor += ch.len_utf8();
            }
            _ => {}
        }
    }

    fn accept_history_search(&mut self, state: &mut AppState) {
        let Some(prompt) = self.history_search.prompt.as_ref() else {
            return;
        };
        let pattern = if prompt.text.is_empty() {
            let Some(last) = &self.history_search.last else {
                search_error(state, "No previous history search.");
                return;
            };
            last.pattern.clone()
        } else {
            prompt.text.clone()
        };
        let regex = match RegexBuilder::new(&pattern).size_limit(1_048_576).build() {
            Ok(regex) => regex,
            Err(_) => {
                search_error(
                    state,
                    "Invalid history search pattern (or pattern too complex); edit it or press Esc.",
                );
                return;
            }
        };
        let older = prompt.older;
        let count = prompt.count;
        self.history_search.last = Some(LastSearch {
            pattern,
            regex,
            older,
        });
        self.history_search.prompt = None;
        self.repeat_history_search(state, false, count);
    }

    pub(super) fn repeat_history_search(
        &mut self,
        state: &mut AppState,
        reverse: bool,
        count: usize,
    ) {
        self.cancel_pending();
        let Some(last) = &self.history_search.last else {
            search_error(state, "No previous history search.");
            return;
        };
        let older = last.older != reverse;
        let result = find_match(
            &state.command_history,
            state.history_position,
            &last.regex,
            older,
            count,
        );
        match result {
            Ok(Some((index, cursor))) => {
                if state.history_draft.is_none() {
                    state.history_draft = Some(state.input.clone());
                }
                state.input.clone_from(&state.command_history[index]);
                state.cursor = cursor;
                state.history_position = Some(index);
                state.submitted_input_selected = false;
                state.clear_input_completion();
                self.undo.clear();
                self.redo.clear();
                self.mode = Mode::Normal;
                self.clamp(state);
            }
            Ok(None) => search_error(state, "History search: pattern not found."),
            Err(message) => search_error(state, message),
        }
    }
}

fn search_error(state: &mut AppState, message: &str) {
    state.push_output(message, crate::state::OutputCategory::System);
}

/// Scan once, then resolve counts arithmetically; never rescan the history per count.
fn find_match(
    history: &[String],
    current: Option<usize>,
    regex: &Regex,
    older: bool,
    count: usize,
) -> Result<Option<(usize, usize)>, &'static str> {
    if history.len() > MAX_HISTORY_ENTRIES {
        return Err("History search limit reached: more than 100,000 entries.");
    }
    let mut bytes = 0usize;
    let mut matches = Vec::new();
    for (index, text) in history.iter().enumerate() {
        bytes = bytes.saturating_add(
            text.len()
                .max(1)
                .saturating_mul(regex.as_str().len().max(1)),
        );
        if bytes > MAX_WORK {
            return Err(
                "History search work limit reached; use a shorter pattern or a smaller history.",
            );
        }
        if let Some(found) = regex.find(text) {
            matches.push((index, found.start()));
        }
    }
    if matches.is_empty() {
        return Ok(None);
    }
    let total = matches.len();
    let position = current.filter(|&p| p < history.len());
    let first = if older {
        let split =
            matches.partition_point(|(index, _)| *index < position.unwrap_or(history.len()));
        (split + total - 1) % total
    } else {
        position.map_or(0, |position| {
            matches.partition_point(|(index, _)| *index <= position) % total
        })
    };
    let offset = (count.max(1) - 1) % total;
    let result = if older {
        (first + total - offset) % total
    } else {
        (first + offset) % total
    };
    Ok(Some(matches[result]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_limits_are_reported_and_counts_do_not_rescan() {
        let regex = Regex::new("x").unwrap();
        assert!(
            find_match(
                &vec![String::new(); MAX_HISTORY_ENTRIES + 1],
                None,
                &regex,
                true,
                1
            )
            .is_err()
        );
        let history = vec!["x".repeat(MAX_WORK + 1)];
        assert!(find_match(&history, None, &regex, true, 1).is_err());
        let history = vec!["x".into(), "not a match".into(), "x again".into()];
        assert_eq!(
            find_match(&history, None, &regex, true, 10_000).unwrap(),
            Some((0, 0))
        );
        assert_eq!(
            find_match(&history, Some(2), &regex, false, 1).unwrap(),
            Some((0, 0))
        );
    }
}
