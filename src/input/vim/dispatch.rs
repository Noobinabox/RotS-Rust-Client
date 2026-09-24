use super::*;

impl VimEditor {
    pub(super) fn insert_key(&mut self, state: &mut AppState, key: KeyEvent) -> InputAction {
        if key.code == KeyCode::Esc {
            self.record(EditEvent::Key(key));
            self.finish(state);
            self.mode = Mode::Normal;
            if state.submitted_input_selected {
                crate::input::edit_selected_submission(state);
            }
            state.cursor = previous(&state.input, state.cursor);
            self.clamp(state);
            state.clear_input_completion();
            return InputAction::None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
            && !crate::input::is_word_edit_key(key)
            && !matches!(key.code, KeyCode::Char('f' | 'e' | 'l' | 'n' | 'p'))
        {
            return InputAction::None;
        }
        if matches!(key.code, KeyCode::Up | KeyCode::Down) && key.modifiers.is_empty() {
            self.finish(state);
            crate::input::navigate_history(state, key.code);
            self.undo.clear();
            self.redo.clear();
            return InputAction::None;
        }
        if matches!(
            key.code,
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
        ) || (key.modifiers == KeyModifiers::ALT && matches!(key.code, KeyCode::Char('b' | 'f')))
        {
            // Vim ends an insertion transaction at cursor movement. Otherwise a
            // frozen diff could repeat unchanged text traversed before typing.
            self.finish(state);
            return crate::input::handle_standard_key(state, key);
        }
        // A fresh draft replaces a highlighted submission, matching standard input.
        if state.submitted_input_selected
            && matches!(
                key.code,
                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
            )
        {
            crate::input::clear_selected_submission(state);
        } else if state.submitted_input_selected {
            crate::input::edit_selected_submission(state);
        }
        self.checkpoint(state);
        let before = Snapshot::capture(state);
        let action = crate::input::handle_standard_key(state, key);
        if !within_limits(&state.input) {
            before.restore(state);
            return InputAction::None;
        }
        if matches!(action, InputAction::None) && !state.output_view.search_active {
            self.record(EditEvent::Key(key));
        }
        action
    }
    pub(super) fn waiting_key(
        &mut self,
        state: &mut AppState,
        ch: char,
        count: usize,
        waiting: Waiting,
    ) -> InputAction {
        match waiting {
            Waiting::Register => {
                if ch.is_ascii_lowercase() || ch == '"' {
                    self.register = Some(ch);
                } else {
                    self.cancel_pending();
                }
            }
            Waiting::Find(command) => {
                self.last_find = Some((command, ch));
                if let Some(at) = find(&state.input, state.cursor, command, ch, count) {
                    self.apply_motion(state, at, matches!(command, 'f' | 't'));
                } else {
                    self.cancel_pending();
                }
            }
            Waiting::Object(around) => {
                if let Some(range) = object(&state.input, state.cursor, ch, around, count) {
                    if let Some((op, _)) = self.operator {
                        self.operate(state, op, range, false);
                    } else {
                        self.anchor = range.start;
                        state.cursor = previous(&state.input, range.end).max(range.start);
                        self.count = 0;
                    }
                } else {
                    self.cancel_pending();
                }
            }
            Waiting::Replace => {
                let row = line(&state.input, state.cursor);
                let visual = self.selection(&state.input, state.cursor);
                let end = state.input[state.cursor..row.end]
                    .char_indices()
                    .nth(count)
                    .map_or(row.end, |(i, _)| state.cursor + i);
                let range = visual.clone().unwrap_or(state.cursor..end);
                let chars = state.input[range.clone()].chars().count();
                if visual.is_some() || chars == count {
                    let replacement = ch.to_string().repeat(chars);
                    if fits(&state.input, range.len(), &replacement) {
                        state.cursor = range.start;
                        state.input.replace_range(range, &replacement);
                        crate::input::clear_history_navigation(state);
                    }
                }
                self.mode = Mode::Normal;
                self.finish(state);
                self.clamp(state);
            }
        }
        InputAction::None
    }
    pub(super) fn repeat(&mut self, state: &mut AppState, count: usize) {
        let recipe = self.last_change.clone();
        self.cancel_pending();
        let original_editor = self.clone();
        let original = Snapshot::capture(state);
        let history = (state.history_position, state.history_draft.clone());
        self.replaying = true;
        self.replay_work = Some(MAX_WORK);
        for _ in 0..count.min(MAX_RECIPE / recipe.len().max(1)) {
            for event in &recipe {
                match event {
                    EditEvent::Key(key) => {
                        self.key(state, *key);
                    }
                    EditEvent::Paste(text) => {
                        self.paste(state, text);
                    }
                    EditEvent::Insert(edit) if self.mode == Mode::Insert => {
                        if let Some(remaining) = self.replay_work.as_mut() {
                            let cost = state.input.len().max(1);
                            if cost > *remaining {
                                self.work_exhausted = true;
                                break;
                            }
                            *remaining -= cost;
                        }
                        if !edit.apply(state) {
                            self.work_exhausted = true;
                            break;
                        }
                        crate::input::clear_history_navigation(state);
                    }
                    EditEvent::Insert(_) => {}
                }
                if self.work_exhausted {
                    break;
                }
            }
            if self.work_exhausted {
                break;
            }
        }
        if self.work_exhausted {
            original.restore(state);
            (state.history_position, state.history_draft) = history;
            *self = original_editor;
        }
        self.replaying = false;
        self.replay_work = None;
    }
}
