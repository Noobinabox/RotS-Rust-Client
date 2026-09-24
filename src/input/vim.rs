//! Session-local modal editing. Only Enter can produce a command action.
mod dispatch;
mod history_search;
mod insert_edit;
mod motions;
#[cfg(test)]
mod tests;

use super::InputAction;
use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};
use motions::{find, line, lines, motion, next, object, previous};
use std::{collections::BTreeMap, ops::Range};

const MAX_COUNT: usize = 10_000;
const MAX_UNDO: usize = 100;
const MAX_UNDO_BYTES: usize = 1_048_576;
const MAX_RECIPE: usize = 4096;
const MAX_WORK: usize = 8_388_608;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Insert,
    Normal,
    Visual,
    VisualLine,
}

#[derive(Debug, Clone)]
struct Snapshot {
    text: String,
    cursor: usize,
}
impl Snapshot {
    fn capture(state: &AppState) -> Self {
        Self {
            text: state.input.clone(),
            cursor: state.cursor,
        }
    }
    fn restore(self, state: &mut AppState) {
        state.input = self.text;
        state.cursor = self.cursor;
    }
}

#[derive(Debug, Clone)]
enum EditEvent {
    Key(KeyEvent),
    Paste(String),
    Insert(insert_edit::Splice),
}

#[derive(Debug, Clone, Copy)]
enum Waiting {
    Find(char),
    Object(bool),
    Register,
    Replace,
}

#[derive(Debug, Default, Clone)]
struct Register {
    text: String,
    linewise: bool,
}

#[derive(Debug, Default, Clone)]
pub struct VimEditor {
    history_search: history_search::HistorySearch,
    pub mode: Mode,
    anchor: usize,
    count: usize,
    operator: Option<(char, usize)>,
    waiting: Option<Waiting>,
    register: Option<char>,
    registers: BTreeMap<char, Register>,
    last_find: Option<(char, char)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    before: Option<Snapshot>,
    insert_start: Option<(Snapshot, usize)>,
    recipe: Vec<EditEvent>,
    last_change: Vec<EditEvent>,
    replaying: bool,
    recipe_overflow: bool,
    replay_work: Option<usize>,
    work_exhausted: bool,
}

impl VimEditor {
    pub fn label(&self) -> &'static str {
        if self.history_search_active() {
            return "H-SEARCH";
        }
        match self.mode {
            Mode::Insert => "INSERT",
            Mode::Normal => "NORMAL",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }

    /// Reload/profile changes reset draft-local state, but never persist registers.
    pub fn reset(&mut self) {
        let registers = std::mem::take(&mut self.registers);
        let last = self.history_search.last.take();
        let accepted_enter = self.history_search.accepted_enter;
        *self = Self {
            registers,
            ..Self::default()
        };
        self.history_search.last = last;
        self.history_search.accepted_enter = accepted_enter;
    }

    /// Vim consumes editing keys before macros, including explicitly overridden macros.
    pub fn owns(key: KeyEvent) -> bool {
        if key.state.contains(KeyEventState::KEYPAD) {
            return false;
        }
        matches!(
            key.code,
            KeyCode::Char(_)
                | KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Tab
                | KeyCode::BackTab
        )
    }

    fn cancel_pending(&mut self) {
        self.count = 0;
        self.operator = None;
        self.waiting = None;
        self.register = None;
        self.before = None;
        self.insert_start = None;
        self.recipe.clear();
        self.recipe_overflow = false;
    }

    fn record(&mut self, event: EditEvent) {
        if self.recipe.len() < MAX_RECIPE && !self.recipe_overflow {
            self.recipe.push(event);
        } else {
            self.recipe.clear();
            self.recipe_overflow = true;
        }
    }

    fn checkpoint(&mut self, state: &AppState) {
        if self.before.is_none() {
            self.before = Some(Snapshot::capture(state));
            if self.mode == Mode::Insert && self.recipe.is_empty() {
                self.record(EditEvent::Key(KeyEvent::new(
                    KeyCode::Char('i'),
                    KeyModifiers::NONE,
                )));
            }
            if self.mode == Mode::Insert {
                self.insert_start = Some((Snapshot::capture(state), self.recipe.len()));
            }
        }
    }

    fn trim_history(history: &mut Vec<Snapshot>) {
        let mut bytes: usize = history.iter().map(|s| s.text.len()).sum();
        while history.len() > MAX_UNDO || bytes > MAX_UNDO_BYTES {
            bytes -= history.remove(0).text.len();
        }
    }

    fn finish(&mut self, state: &AppState) {
        if let Some((start, recipe_len)) = self.insert_start.take()
            && !self.recipe_overflow
        {
            self.recipe.truncate(recipe_len);
            self.record(EditEvent::Insert(insert_edit::Splice::between(
                &start, state,
            )));
            self.record(EditEvent::Key(KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )));
        }
        if let Some(before) = self.before.take().filter(|s| s.text != state.input) {
            self.undo.push(before);
            Self::trim_history(&mut self.undo);
            self.redo.clear();
            if !self.replaying {
                self.last_change = if self.recipe_overflow {
                    Vec::new()
                } else {
                    self.recipe.clone()
                };
            }
            // Leaving history on any real mutation prevents the recalled entry changing.
        }
        self.cancel_pending();
    }

    fn clamp(&self, state: &mut AppState) {
        state.cursor = state.cursor.min(state.input.len());
        while !state.input.is_char_boundary(state.cursor) {
            state.cursor -= 1;
        }
        if self.mode != Mode::Insert {
            let row = line(&state.input, state.cursor);
            if state.cursor == row.end && !row.is_empty() {
                state.cursor = previous(&state.input, state.cursor);
            }
        }
    }

    pub fn selection(&self, text: &str, cursor: usize) -> Option<Range<usize>> {
        if !matches!(self.mode, Mode::Visual | Mode::VisualLine) {
            return None;
        }
        let mut start = self.anchor.min(cursor).min(text.len());
        let mut end = self.anchor.max(cursor).min(text.len());
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Some(if self.mode == Mode::VisualLine {
            let row = line(text, end);
            line(text, start).start..if row.end < text.len() {
                row.end + 1
            } else {
                row.end
            }
        } else {
            start..next(text, end)
        })
    }

    fn store_register(&mut self, text: String, linewise: bool) {
        let value = Register { text, linewise };
        self.registers.insert('"', value.clone());
        if let Some(name) = self.register {
            self.registers.insert(name, value);
        }
    }

    fn operate(&mut self, state: &mut AppState, op: char, mut range: Range<usize>, linewise: bool) {
        if op == 'c'
            && linewise
            && range.end > range.start
            && state.input[..range.end].ends_with('\n')
        {
            range.end -= 1;
        }
        if range.is_empty() && op != 'c' {
            self.finish(state);
            return;
        }
        self.store_register(state.input[range.clone()].to_string(), linewise);
        if op == 'd' && linewise && range.end == state.input.len() && range.start > 0 {
            range.start -= 1; // Remove the preceding separator of a deleted final line.
        }
        state.cursor = range.start;
        if op != 'y' {
            state.input.replace_range(range, "");
            super::clear_history_navigation(state);
        }
        self.mode = if op == 'c' {
            Mode::Insert
        } else {
            Mode::Normal
        };
        self.operator = None;
        self.count = 0;
        self.waiting = None;
        if op == 'c' {
            self.insert_start = Some((Snapshot::capture(state), self.recipe.len()));
        } else {
            self.finish(state);
        }
        self.clamp(state);
    }

    fn apply_motion(&mut self, state: &mut AppState, target: usize, inclusive: bool) {
        if let Some((op, _)) = self.operator {
            let start = state.cursor.min(target);
            let end = state.cursor.max(target);
            let end = if inclusive {
                next(&state.input, end)
            } else {
                end
            };
            self.operate(state, op, start..end, false);
        } else {
            state.cursor = target;
            if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
                self.count = 0;
                self.waiting = None;
            } else {
                self.cancel_pending();
            }
            self.clamp(state);
        }
    }

    fn history(&mut self, state: &mut AppState, key: KeyCode, count: usize) {
        for _ in 0..count {
            super::navigate_history(state, key);
        }
        self.undo.clear();
        self.redo.clear();
        self.cancel_pending();
        self.mode = Mode::Normal;
        self.clamp(state);
    }

    fn undo(&mut self, state: &mut AppState, redo: bool, count: usize) {
        self.cancel_pending();
        for _ in 0..count {
            let (source, dest) = if redo {
                (&mut self.redo, &mut self.undo)
            } else {
                (&mut self.undo, &mut self.redo)
            };
            let Some(snapshot) = source.pop() else {
                break;
            };
            dest.push(Snapshot::capture(state));
            Self::trim_history(dest);
            snapshot.restore(state);
        }
        super::clear_history_navigation(state);
        self.mode = Mode::Normal;
        self.clamp(state);
    }

    fn put(&mut self, state: &mut AppState, after: bool, count: usize) {
        let Some(value) = self.registers.get(&self.register.unwrap_or('"')).cloned() else {
            self.finish(state);
            return;
        };
        let mut text = value.text;
        if let Some(range) = self.selection(&state.input, state.cursor) {
            if text.len().saturating_mul(count) > super::MAX_PASTE_BYTES {
                self.work_exhausted = self.replaying;
                self.finish(state);
                return;
            }
            text = text.repeat(count);
            if self.mode == Mode::VisualLine
                && range.end < state.input.len()
                && !text.ends_with('\n')
            {
                text.push('\n');
            }
            let mut result = state.input.clone();
            result.replace_range(range.clone(), &text);
            if within_limits(&result) {
                state.input = result;
                state.cursor = range.start;
                super::clear_history_navigation(state);
            } else {
                self.work_exhausted = self.replaying;
            }
            self.mode = Mode::Normal;
            self.finish(state);
            self.clamp(state);
            return;
        }
        let mut at = if after {
            next(&state.input, state.cursor)
        } else {
            state.cursor
        };
        if value.linewise {
            let row = line(&state.input, state.cursor);
            at = if after {
                row.end + usize::from(row.end < state.input.len())
            } else {
                row.start
            };
            if !text.ends_with('\n') {
                text.push('\n');
            }
        }
        if text
            .len()
            .saturating_mul(count)
            .saturating_add(state.input.len())
            > super::MAX_PASTE_BYTES
        {
            self.work_exhausted = self.replaying;
            self.finish(state);
            return;
        }
        text = text.repeat(count);
        if value.linewise
            && after
            && at == state.input.len()
            && !state.input.is_empty()
            && !state.input.ends_with('\n')
        {
            text.insert(0, '\n');
            text.pop();
        }
        if !fits(&state.input, 0, &text) {
            self.work_exhausted = self.replaying;
            self.finish(state);
            return;
        }
        state.input.insert_str(at, &text);
        state.cursor = at;
        super::clear_history_navigation(state);
        self.finish(state);
        self.clamp(state);
    }

    pub(super) fn key(&mut self, state: &mut AppState, key: KeyEvent) -> InputAction {
        if self.suppress_history_enter_repeat(key) {
            return InputAction::None;
        }
        if let Some(remaining) = self.replay_work.as_mut() {
            let cost = state
                .input
                .len()
                .max(1)
                .saturating_mul(self.count.max(1))
                .saturating_mul(self.operator.map_or(1, |(_, n)| n));
            if cost > *remaining {
                self.work_exhausted = true;
                return InputAction::None;
            }
            *remaining -= cost;
        }
        self.clamp(state);
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            super::clear_current_input(state);
            self.reset();
            return InputAction::None;
        }
        if self.history_search_active() {
            self.history_search_key(state, key);
            return InputAction::None;
        }
        if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::ALT {
            return InputAction::None; // The application inserts a newline only when configured.
        }
        if key.code == KeyCode::Enter {
            self.reset();
            return super::submit_input(state);
        }
        if self.mode == Mode::Insert {
            return self.insert_key(state, key);
        }
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            self.cancel_pending();
            state.clear_input_completion();
            return InputAction::None;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('r') {
            let count = self.count.max(1);
            self.undo(state, true, count);
            return InputAction::None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(
                key.code,
                KeyCode::Char('f' | 'e' | 'l' | 'n' | 'p') | KeyCode::Up | KeyCode::Down
            ) {
                self.cancel_pending();
                return super::handle_standard_key(state, key);
            }
            return InputAction::None;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            return InputAction::None;
        }
        super::edit_selected_submission(state);
        state.clear_input_completion();
        if matches!(key.code, KeyCode::Up | KeyCode::Down) && self.operator.is_none() {
            self.history(state, key.code, self.count.max(1));
            return InputAction::None;
        }
        let ch = match key.code {
            KeyCode::Char(ch) => ch,
            KeyCode::Left => 'h',
            KeyCode::Right => 'l',
            KeyCode::Home => '0',
            KeyCode::End => '$',
            KeyCode::Delete => 'x',
            KeyCode::Backspace => 'X',
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::F(2) => {
                return super::handle_standard_key(state, key);
            }
            _ => {
                self.cancel_pending();
                return InputAction::None;
            }
        };
        self.checkpoint(state);
        self.record(EditEvent::Key(key));
        let count = self
            .count
            .max(1)
            .saturating_mul(self.operator.map_or(1, |(_, n)| n))
            .min(MAX_COUNT);
        if count.saturating_mul(state.input.len().max(1)) > MAX_WORK {
            self.work_exhausted = self.replaying;
            self.cancel_pending();
            return InputAction::None;
        }
        if let Some(waiting) = self.waiting.take() {
            return self.waiting_key(state, ch, count, waiting);
        }
        if ch.is_ascii_digit() && (ch != '0' || self.count > 0) {
            self.count = self
                .count
                .saturating_mul(10)
                .saturating_add(ch as usize - '0' as usize)
                .min(MAX_COUNT);
            return InputAction::None;
        }
        if ch == '"' {
            self.waiting = Some(Waiting::Register);
            return InputAction::None;
        }
        if matches!(ch, 'f' | 'F' | 't' | 'T') {
            self.waiting = Some(Waiting::Find(ch));
            return InputAction::None;
        }
        if matches!(ch, 'i' | 'a')
            && (self.operator.is_some() || self.selection(&state.input, state.cursor).is_some())
        {
            self.waiting = Some(Waiting::Object(ch == 'a'));
            return InputAction::None;
        }
        if matches!(ch, 'd' | 'c' | 'y') {
            if let Some(range) = self.selection(&state.input, state.cursor) {
                self.operate(state, ch, range, self.mode == Mode::VisualLine);
            } else if let Some((op, _)) = self.operator {
                if op == ch {
                    let range = lines(&state.input, state.cursor, count);
                    self.operate(state, ch, range, true);
                } else {
                    self.cancel_pending();
                }
            } else {
                self.operator = Some((ch, self.count.max(1)));
                self.count = 0;
            }
            return InputAction::None;
        }
        if matches!(ch, ';' | ',') {
            if let Some((mut command, target)) = self.last_find {
                if ch == ',' {
                    command = match command {
                        'f' => 'F',
                        'F' => 'f',
                        't' => 'T',
                        _ => 't',
                    };
                }
                // Till-repeat skips the previously targeted adjacent character.
                let start = match command {
                    't' => next(&state.input, state.cursor),
                    'T' => previous(&state.input, state.cursor),
                    _ => state.cursor,
                };
                if let Some(at) = find(&state.input, start, command, target, count) {
                    self.apply_motion(state, at, matches!(command, 'f' | 't'));
                    return InputAction::None;
                }
            }
            self.cancel_pending();
            return InputAction::None;
        }
        // cw changes through the end of a nonblank word, without its following space.
        if self.operator.is_some_and(|(op, _)| op == 'c')
            && matches!(ch, 'w' | 'W')
            && state.input[state.cursor..]
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_whitespace())
        {
            let end = motions::change_word_end(&state.input, state.cursor, ch == 'W', count);
            self.operate(state, 'c', state.cursor..end, false);
            return InputAction::None;
        }
        if let Some((at, inclusive)) = motion(&state.input, state.cursor, ch, count) {
            self.apply_motion(state, at, inclusive);
            return InputAction::None;
        }
        if self.operator.is_some() {
            self.cancel_pending();
            return InputAction::None;
        }
        match ch {
            '/' | '?' if self.mode == Mode::Normal => self.start_history_search(ch == '/', count),
            'n' | 'N' if self.mode == Mode::Normal => {
                self.repeat_history_search(state, ch == 'N', count)
            }
            'i' | 'a' | 'I' | 'A' => {
                let row = line(&state.input, state.cursor);
                state.cursor = match ch {
                    'a' => next(&state.input, state.cursor).min(row.end),
                    'I' => motion(&state.input, state.cursor, '^', 1).map_or(row.start, |(p, _)| p),
                    'A' => row.end,
                    _ => state.cursor,
                };
                self.mode = Mode::Insert;
                self.count = 0;
                self.insert_start = Some((Snapshot::capture(state), self.recipe.len()));
            }
            'j' | 'k' => self.history(
                state,
                if ch == 'k' {
                    KeyCode::Up
                } else {
                    KeyCode::Down
                },
                count,
            ),
            'v' | 'V' => {
                let mode = if ch == 'v' {
                    Mode::Visual
                } else {
                    Mode::VisualLine
                };
                if self.mode == mode {
                    self.mode = Mode::Normal;
                    self.cancel_pending();
                } else {
                    self.anchor = state.cursor;
                    self.mode = mode;
                    self.count = 0;
                }
            }
            'x' | 'X' | 'D' | 'C' => {
                let range = self
                    .selection(&state.input, state.cursor)
                    .unwrap_or_else(|| {
                        let row = line(&state.input, state.cursor);
                        if matches!(ch, 'D' | 'C') {
                            state.cursor..row.end
                        } else {
                            let mut end = state.cursor;
                            for _ in 0..count {
                                end = if ch == 'X' {
                                    previous(&state.input, end).max(row.start)
                                } else {
                                    next(&state.input, end).min(row.end)
                                };
                            }
                            state.cursor.min(end)..state.cursor.max(end)
                        }
                    });
                self.operate(
                    state,
                    if ch == 'C' { 'c' } else { 'd' },
                    range,
                    self.mode == Mode::VisualLine,
                );
            }
            'r' => self.waiting = Some(Waiting::Replace),
            'u' => self.undo(state, false, count),
            'p' | 'P' => {
                self.put(state, ch == 'p', count);
            }
            '.' if !self.replaying => self.repeat(state, count),
            _ => self.cancel_pending(),
        }
        InputAction::None
    }

    /// Paste is literal text, not a stream of keys, and never executes a command.
    pub(super) fn paste_checked(
        &mut self,
        state: &mut AppState,
        text: &str,
    ) -> Result<(), &'static str> {
        if self.history_search_active() {
            return self.paste_history_search(text);
        }
        let selected = state.submitted_input_selected;
        let range = if selected {
            0..state.input.len()
        } else {
            self.selection(&state.input, state.cursor)
                .unwrap_or(state.cursor..state.cursor)
        };
        let mut result = state.input.clone();
        result.replace_range(range, text);
        if !within_limits(&result) {
            return Err("Paste rejected: input exceeds 64 KiB or 128 lines.");
        }
        if selected {
            super::clear_selected_submission(state);
        }
        self.paste(state, text);
        Ok(())
    }

    fn paste(&mut self, state: &mut AppState, text: &str) {
        if let Some(remaining) = self.replay_work.as_mut() {
            let cost = state.input.len().saturating_add(text.len());
            if cost > *remaining {
                self.work_exhausted = true;
                return;
            }
            *remaining -= cost;
        }
        let range = self
            .selection(&state.input, state.cursor)
            .unwrap_or(state.cursor..state.cursor);
        let mut result = state.input.clone();
        result.replace_range(range.clone(), text);
        if !within_limits(&result) {
            self.work_exhausted = self.replaying;
            return;
        }
        let visual_recipe = if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
            self.recipe.clone()
        } else {
            Vec::new()
        };
        self.finish(state);
        self.checkpoint(state);
        // Paste repeats as literal replacement/insertion, never as typed commands.
        self.recipe = visual_recipe;
        self.record(EditEvent::Paste(text.to_owned()));
        state.input.replace_range(range.clone(), text);
        state.cursor = range.start + text.len();
        super::clear_history_navigation(state);
        state.clear_input_completion();
        if self.mode != Mode::Insert {
            self.mode = Mode::Normal;
        }
        self.finish(state);
        self.clamp(state);
    }
}

fn within_limits(text: &str) -> bool {
    text.len() <= super::MAX_PASTE_BYTES
        && text.bytes().filter(|b| *b == b'\n').count() < super::MAX_INPUT_LINES
}

fn fits(text: &str, removed: usize, inserted: &str) -> bool {
    // The caller also checks the final line count for replacements that remove lines.
    text.len()
        .saturating_sub(removed)
        .saturating_add(inserted.len())
        <= super::MAX_PASTE_BYTES
        && text.bytes().filter(|b| *b == b'\n').count()
            + inserted.bytes().filter(|b| *b == b'\n').count()
            < super::MAX_INPUT_LINES
}
