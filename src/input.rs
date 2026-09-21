use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    commands::ClientCommand,
    state::{AppState, SearchEdit},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    None,
    Command(ClientCommand),
    ClearOutput,
    FollowOutput,
    ScrollOutputDown(usize),
    ScrollOutputUp(usize),
    SearchNext,
    SearchPrevious,
    ToggleOutputDisplayMode,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> InputAction {
    if state.output_view.search_active {
        return handle_search_key(state, key);
    }

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if state.submitted_input_selected || !state.input.is_empty() {
                clear_current_input(state);
                InputAction::None
            } else {
                InputAction::Command(ClientCommand::Quit)
            }
        }
        KeyCode::Esc => InputAction::None,
        KeyCode::Enter => submit_input(state),
        KeyCode::PageUp => InputAction::ScrollOutputUp(10),
        KeyCode::PageDown => InputAction::ScrollOutputDown(10),
        KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::ScrollOutputUp(1)
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::ScrollOutputDown(1)
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::FollowOutput
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::ClearOutput
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.start_output_search();
            InputAction::None
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::SearchNext
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputAction::SearchPrevious
        }
        KeyCode::F(2) => InputAction::ToggleOutputDisplayMode,
        KeyCode::Tab => {
            state.complete_input(false);
            InputAction::None
        }
        KeyCode::BackTab => {
            state.complete_input(true);
            InputAction::None
        }
        KeyCode::Backspace => {
            clear_selected_submission(state);
            clear_history_navigation(state);
            state.clear_input_completion();
            if state.cursor > 0 {
                state.cursor = previous_boundary(&state.input, state.cursor);
                state.input.remove(state.cursor);
            }
            InputAction::None
        }
        KeyCode::Delete => {
            clear_selected_submission(state);
            clear_history_navigation(state);
            state.clear_input_completion();
            if state.cursor < state.input.len() {
                state.input.remove(state.cursor);
            }
            InputAction::None
        }
        KeyCode::Left => {
            edit_selected_submission(state);
            state.clear_input_completion();
            state.cursor = previous_boundary(&state.input, state.cursor);
            InputAction::None
        }
        KeyCode::Right => {
            edit_selected_submission(state);
            state.clear_input_completion();
            state.cursor = next_boundary(&state.input, state.cursor);
            InputAction::None
        }
        KeyCode::Home => {
            edit_selected_submission(state);
            state.clear_input_completion();
            state.cursor = 0;
            InputAction::None
        }
        KeyCode::End => {
            edit_selected_submission(state);
            state.clear_input_completion();
            state.cursor = state.input.len();
            InputAction::None
        }
        KeyCode::Up | KeyCode::Down => {
            navigate_history(state, key.code);
            InputAction::None
        }
        KeyCode::Char(value) => {
            clear_selected_submission(state);
            clear_history_navigation(state);
            state.clear_input_completion();
            state.input.insert(state.cursor, value);
            state.cursor += value.len_utf8();
            InputAction::None
        }
        _ => InputAction::None,
    }
}

fn handle_search_key(state: &mut AppState, key: KeyEvent) -> InputAction {
    match key.code {
        KeyCode::Enter => {
            state.commit_output_search();
            InputAction::None
        }
        KeyCode::Esc => {
            state.cancel_output_search();
            InputAction::None
        }
        KeyCode::Backspace => {
            state.edit_search_input(SearchEdit::Backspace);
            InputAction::None
        }
        KeyCode::Delete => {
            state.edit_search_input(SearchEdit::Delete);
            InputAction::None
        }
        KeyCode::Left => {
            state.edit_search_input(SearchEdit::Left);
            InputAction::None
        }
        KeyCode::Right => {
            state.edit_search_input(SearchEdit::Right);
            InputAction::None
        }
        KeyCode::Home => {
            state.edit_search_input(SearchEdit::Home);
            InputAction::None
        }
        KeyCode::End => {
            state.edit_search_input(SearchEdit::End);
            InputAction::None
        }
        KeyCode::Char(value) => {
            state.edit_search_input(SearchEdit::Insert(value));
            InputAction::None
        }
        _ => InputAction::None,
    }
}

fn previous_boundary(input: &str, cursor: usize) -> usize {
    input
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index < cursor)
        .last()
        .unwrap_or(0)
}

fn next_boundary(input: &str, cursor: usize) -> usize {
    input
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index > cursor)
        .unwrap_or(input.len())
}

fn submit_input(state: &mut AppState) -> InputAction {
    let command = if state.submitted_input_selected {
        state.submitted_input.clone().unwrap_or_default()
    } else {
        state.input.trim_end().to_string()
    };
    state.input.clear();
    state.cursor = 0;
    state.submitted_input_selected = false;
    state.clear_input_completion();
    clear_history_navigation(state);

    if command.is_empty() {
        state.submitted_input = None;
        return InputAction::Command(ClientCommand::SendText(String::new()));
    }
    if command == "/clear" {
        state.submitted_input = None;
        return InputAction::ClearOutput;
    }
    state.submitted_input = Some(command.clone());
    state.submitted_input_selected = true;
    state.command_history.push(command.clone());
    if command == "/quit" {
        InputAction::Command(ClientCommand::Quit)
    } else {
        InputAction::Command(ClientCommand::SendText(command))
    }
}

fn clear_selected_submission(state: &mut AppState) {
    if state.submitted_input_selected {
        state.input.clear();
        state.cursor = 0;
        state.submitted_input_selected = false;
        state.clear_input_completion();
    }
}

fn clear_current_input(state: &mut AppState) {
    state.input.clear();
    state.cursor = 0;
    state.submitted_input_selected = false;
    state.clear_input_completion();
    clear_history_navigation(state);
}

fn edit_selected_submission(state: &mut AppState) {
    if state.submitted_input_selected {
        state.input = state.submitted_input.clone().unwrap_or_default();
        state.cursor = state.input.len();
        state.submitted_input_selected = false;
        state.clear_input_completion();
    }
}

fn clear_history_navigation(state: &mut AppState) {
    state.history_position = None;
    state.history_draft = None;
}

fn navigate_history(state: &mut AppState, key_code: KeyCode) {
    if state.command_history.is_empty() {
        return;
    }

    state.submitted_input_selected = false;
    state.clear_input_completion();
    let draft = state
        .history_draft
        .clone()
        .unwrap_or_else(|| state.input.clone());
    let next_position = match key_code {
        KeyCode::Up => {
            if state.history_draft.is_none() {
                state.history_draft = Some(draft.clone());
            }
            previous_history_match(&state.command_history, state.history_position, &draft)
        }
        KeyCode::Down => match state.history_position {
            Some(position) => {
                next_history_match(&state.command_history, position, &draft).or_else(|| {
                    state.input = state.history_draft.take().unwrap_or_default();
                    state.cursor = state.input.len();
                    state.history_position = None;
                    None
                })
            }
            None => None,
        },
        _ => None,
    };
    let Some(next_position) = next_position else {
        return;
    };

    state.input = state.command_history[next_position].clone();
    state.cursor = state.input.len();
    state.history_position = Some(next_position);
}

fn previous_history_match(
    history: &[String],
    current_position: Option<usize>,
    prefix: &str,
) -> Option<usize> {
    let end = current_position.unwrap_or(history.len());
    history
        .iter()
        .take(end)
        .rposition(|command| command.starts_with(prefix))
}

fn next_history_match(history: &[String], current_position: usize, prefix: &str) -> Option<usize> {
    history
        .iter()
        .enumerate()
        .skip(current_position.saturating_add(1))
        .find_map(|(position, command)| command.starts_with(prefix).then_some(position))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::{config::AppConfig, state::OutputCategory};

    use super::*;

    #[test]
    fn enter_sends_text_and_selects_last_command() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "look".to_string();
        state.cursor = 4;

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(
            action,
            InputAction::Command(ClientCommand::SendText("look".to_string()))
        );
        assert!(state.input.is_empty());
        assert_eq!(state.submitted_input.as_deref(), Some("look"));
        assert!(state.submitted_input_selected);
    }

    #[test]
    fn enter_with_empty_input_sends_blank_line() {
        let mut state = AppState::new(&AppConfig::default());

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(
            action,
            InputAction::Command(ClientCommand::SendText(String::new()))
        );
        assert!(state.input.is_empty());
        assert!(state.submitted_input.is_none());
        assert!(!state.submitted_input_selected);
        assert!(state.command_history.is_empty());
    }

    #[test]
    fn ctrl_c_clears_nonempty_input_without_quitting() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "cast fireball".to_string();
        state.cursor = state.input.len();

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );

        assert_eq!(action, InputAction::None);
        assert!(state.input.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn ctrl_c_quits_when_input_is_empty() {
        let mut state = AppState::new(&AppConfig::default());

        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            ),
            InputAction::Command(ClientCommand::Quit)
        );
    }

    #[test]
    fn typing_replaces_selected_last_command() {
        let mut state = AppState::new(&AppConfig::default());
        state.submitted_input = Some("look".to_string());
        state.submitted_input_selected = true;

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );

        assert_eq!(action, InputAction::None);
        assert_eq!(state.input, "n");
        assert_eq!(state.cursor, 1);
        assert!(!state.submitted_input_selected);
        assert_eq!(state.submitted_input.as_deref(), Some("look"));
    }

    #[test]
    fn arrow_key_edits_selected_last_command() {
        let mut state = AppState::new(&AppConfig::default());
        state.submitted_input = Some("look".to_string());
        state.submitted_input_selected = true;

        let action = handle_key(&mut state, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

        assert_eq!(action, InputAction::None);
        assert_eq!(state.input, "look");
        assert_eq!(state.cursor, 3);
        assert!(!state.submitted_input_selected);
    }

    #[test]
    fn tab_completes_current_word_from_recent_mud_output() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("A sturdy door blocks the passage.", OutputCategory::Normal);
        state.input = "open do".to_string();
        state.cursor = state.input.len();

        let action = handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert_eq!(action, InputAction::None);
        assert_eq!(state.input, "open door");
        assert_eq!(state.cursor, "open door".len());
        assert!(state.input_completion.active);
    }

    #[test]
    fn tab_cycles_multiple_completion_matches() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("A dolphin passes a sturdy door.", OutputCategory::Normal);
        state.input = "open do".to_string();
        state.cursor = state.input.len();

        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.input, "open dolphin");
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.input, "open door");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(state.input, "open dolphin");
    }

    #[test]
    fn typing_clears_completion_dropdown() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("A dolphin passes a sturdy door.", OutputCategory::Normal);
        state.input = "open do".to_string();
        state.cursor = state.input.len();
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(state.input_completion.active);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );

        assert!(!state.input_completion.active);
    }

    #[test]
    fn up_arrow_loads_selected_last_command_from_history() {
        let mut state = AppState::new(&AppConfig::default());
        state.submitted_input = Some("look".to_string());
        state.submitted_input_selected = true;
        state.command_history.push("look".to_string());

        let action = handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        assert_eq!(action, InputAction::None);
        assert_eq!(state.input, "look");
        assert_eq!(state.cursor, 4);
        assert!(!state.submitted_input_selected);
        assert_eq!(state.history_position, Some(0));
    }

    #[test]
    fn enter_resends_selected_last_command() {
        let mut state = AppState::new(&AppConfig::default());
        state.submitted_input = Some("look".to_string());
        state.submitted_input_selected = true;

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(
            action,
            InputAction::Command(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(state.submitted_input.as_deref(), Some("look"));
        assert!(state.submitted_input_selected);
    }

    #[test]
    fn up_and_down_navigate_command_history() {
        let mut state = AppState::new(&AppConfig::default());
        state.command_history = vec!["look".to_string(), "score".to_string()];

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.input, "score");
        assert_eq!(state.cursor, 5);

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.input, "look");
        assert_eq!(state.cursor, 4);

        handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.input, "score");
        assert_eq!(state.cursor, 5);

        handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(state.input.is_empty());
        assert_eq!(state.cursor, 0);
        assert_eq!(state.history_position, None);
    }

    #[test]
    fn up_and_down_filter_history_by_typed_prefix() {
        let mut state = AppState::new(&AppConfig::default());
        state.command_history = vec![
            "look".to_string(),
            "open gate".to_string(),
            "score".to_string(),
            "open door".to_string(),
        ];
        state.input = "op".to_string();
        state.cursor = 2;

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.input, "open door");
        assert_eq!(state.cursor, "open door".len());

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.input, "open gate");
        assert_eq!(state.cursor, "open gate".len());

        handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.input, "open door");
        assert_eq!(state.cursor, "open door".len());

        handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.input, "op");
        assert_eq!(state.cursor, 2);
        assert_eq!(state.history_position, None);
    }

    #[test]
    fn history_prefix_without_matches_preserves_input() {
        let mut state = AppState::new(&AppConfig::default());
        state.command_history = vec!["look".to_string(), "score".to_string()];
        state.input = "zz".to_string();
        state.cursor = 2;

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        assert_eq!(state.input, "zz");
        assert_eq!(state.cursor, 2);
        assert_eq!(state.history_position, None);
    }

    #[test]
    fn output_control_keys_return_output_actions() {
        let mut state = AppState::new(&AppConfig::default());

        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)
            ),
            InputAction::ScrollOutputUp(10)
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL)
            ),
            InputAction::ScrollOutputUp(1)
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)
            ),
            InputAction::FollowOutput
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)
            ),
            InputAction::ClearOutput
        );
        assert_eq!(
            handle_key(&mut state, KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)),
            InputAction::ToggleOutputDisplayMode
        );
    }

    #[test]
    fn search_mode_edits_and_commits_query() {
        let mut state = AppState::new(&AppConfig::default());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        );

        assert!(state.output_view.search_active);
        assert_eq!(state.output_view.search_input, "bear");

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert!(!state.output_view.search_active);
        assert_eq!(state.output_view.search_query, "bear");
    }

    #[test]
    fn slash_clear_clears_output_locally() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "/clear".to_string();
        state.cursor = 6;

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(action, InputAction::ClearOutput);
        assert!(state.submitted_input.is_none());
        assert!(!state.submitted_input_selected);
    }

    #[test]
    fn history_navigation_preserves_draft_input() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "ki".to_string();
        state.cursor = 2;
        state.command_history.push("look".to_string());

        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        assert_eq!(state.input, "ki");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn slash_quit_sends_quit() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "/quit".to_string();
        state.cursor = 5;

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(action, InputAction::Command(ClientCommand::Quit));
    }

    #[test]
    fn escape_does_not_quit() {
        let mut state = AppState::new(&AppConfig::default());

        let action = handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn unicode_cursor_movement_stays_on_char_boundaries() {
        let mut state = AppState::new(&AppConfig::default());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('é'), KeyModifiers::NONE),
        );
        handle_key(&mut state, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );

        assert_eq!(state.input, "xé");
        assert_eq!(state.cursor, 1);
    }
}
