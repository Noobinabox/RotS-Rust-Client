use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{InputAction, handle_standard_key};
use crate::{
    commands::ClientCommand, config::InputMode, keybindings::ShortcutAction, state::AppState,
};

/// Execute a resolved shortcut through the existing input operations.
///
/// The caller retains ownership of output-search and Vim key routing. Editor
/// shortcuts are suppressed in Vim mode to preserve its undo/register contract;
/// global actions remain available after the caller's ownership checks.
pub fn handle_shortcut(state: &mut AppState, action: ShortcutAction) -> InputAction {
    let (code, modifiers) = match action {
        ShortcutAction::None => return InputAction::None,
        ShortcutAction::ScrollPageUp => return InputAction::ScrollOutputUp(10),
        ShortcutAction::ScrollPageDown => return InputAction::ScrollOutputDown(10),
        ShortcutAction::ScrollLineUp => return InputAction::ScrollOutputUp(1),
        ShortcutAction::ScrollLineDown => return InputAction::ScrollOutputDown(1),
        ShortcutAction::FollowOutput => return InputAction::FollowOutput,
        ShortcutAction::ClearOutput => return InputAction::ClearOutput,
        ShortcutAction::SearchOutput => {
            state.start_output_search();
            return InputAction::None;
        }
        ShortcutAction::SearchNext => return InputAction::SearchNext,
        ShortcutAction::SearchPrevious => return InputAction::SearchPrevious,
        ShortcutAction::ToggleOutputDisplayMode => return InputAction::ToggleOutputDisplayMode,
        ShortcutAction::Reconnect => return InputAction::Command(ClientCommand::Reconnect),
        ShortcutAction::Reload => {
            return InputAction::Command(ClientCommand::SendText("/reload".to_owned()));
        }
        ShortcutAction::Quit => return InputAction::Command(ClientCommand::Quit),
        ShortcutAction::Submit => (KeyCode::Enter, KeyModifiers::NONE),
        ShortcutAction::Backspace => (KeyCode::Backspace, KeyModifiers::NONE),
        ShortcutAction::Delete => (KeyCode::Delete, KeyModifiers::NONE),
        ShortcutAction::CursorLeft => (KeyCode::Left, KeyModifiers::NONE),
        ShortcutAction::CursorRight => (KeyCode::Right, KeyModifiers::NONE),
        ShortcutAction::LineStart => (KeyCode::Home, KeyModifiers::NONE),
        ShortcutAction::LineEnd => (KeyCode::End, KeyModifiers::NONE),
        ShortcutAction::WordLeft => (KeyCode::Left, KeyModifiers::CONTROL),
        ShortcutAction::WordRight => (KeyCode::Right, KeyModifiers::CONTROL),
        ShortcutAction::DeleteWordLeft => (KeyCode::Backspace, KeyModifiers::CONTROL),
        ShortcutAction::DeleteWordRight => (KeyCode::Delete, KeyModifiers::CONTROL),
        ShortcutAction::HistoryPrevious => (KeyCode::Up, KeyModifiers::NONE),
        ShortcutAction::HistoryNext => (KeyCode::Down, KeyModifiers::NONE),
        ShortcutAction::CompleteNext => (KeyCode::Tab, KeyModifiers::NONE),
        ShortcutAction::CompletePrevious => (KeyCode::BackTab, KeyModifiers::NONE),
    };
    if state.input_mode == InputMode::Vim {
        return InputAction::None;
    }
    handle_standard_key(state, KeyEvent::new(code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn editing_preserves_unicode_boundaries_and_selected_submission_behavior() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "say 猫é".to_owned();
        state.cursor = state.input.len();
        handle_shortcut(&mut state, ShortcutAction::CursorLeft);
        assert_eq!(state.cursor, "say 猫".len());
        handle_shortcut(&mut state, ShortcutAction::Backspace);
        assert_eq!(state.input, "say é");
        handle_shortcut(&mut state, ShortcutAction::Delete);
        assert_eq!(state.input, "say ");
        state.input = "say 猫".to_owned();
        state.cursor = state.input.len();
        assert_eq!(
            handle_shortcut(&mut state, ShortcutAction::Submit),
            InputAction::Command(ClientCommand::SendText("say 猫".to_owned()))
        );
        assert!(state.submitted_input_selected);
        handle_shortcut(&mut state, ShortcutAction::WordLeft);
        assert_eq!(state.input, "say 猫");
        assert_eq!(state.cursor, 4);
        assert!(!state.submitted_input_selected);
        handle_shortcut(&mut state, ShortcutAction::DeleteWordRight);
        assert_eq!(state.input, "say ");
        handle_shortcut(&mut state, ShortcutAction::DeleteWordLeft);
        assert_eq!(state.input, "");
    }

    #[test]
    fn history_shortcuts_restore_the_prefix_draft() {
        let mut state = AppState::new(&AppConfig::default());
        state.command_history = vec!["say 猫".to_owned(), "look".to_owned()];
        state.input = "say".to_owned();
        state.cursor = state.input.len();
        handle_shortcut(&mut state, ShortcutAction::HistoryPrevious);
        assert_eq!(state.input, "say 猫");
        assert_eq!(state.cursor, state.input.len());
        handle_shortcut(&mut state, ShortcutAction::HistoryNext);
        assert_eq!(state.input, "say");
        assert_eq!(state.history_position, None);
    }

    #[test]
    fn global_shortcuts_work_in_both_input_modes_without_rewriting_the_draft() {
        for mode in [InputMode::Standard, InputMode::Vim] {
            let mut state = AppState::new(&AppConfig::default());
            state.input_mode = mode;
            state.input = "draft 猫".to_owned();
            state.cursor = state.input.len();
            for (shortcut, expected) in [
                (ShortcutAction::None, InputAction::None),
                (
                    ShortcutAction::ScrollPageUp,
                    InputAction::ScrollOutputUp(10),
                ),
                (
                    ShortcutAction::ScrollPageDown,
                    InputAction::ScrollOutputDown(10),
                ),
                (ShortcutAction::ScrollLineUp, InputAction::ScrollOutputUp(1)),
                (
                    ShortcutAction::ScrollLineDown,
                    InputAction::ScrollOutputDown(1),
                ),
                (ShortcutAction::FollowOutput, InputAction::FollowOutput),
                (ShortcutAction::ClearOutput, InputAction::ClearOutput),
                (ShortcutAction::SearchNext, InputAction::SearchNext),
                (ShortcutAction::SearchPrevious, InputAction::SearchPrevious),
                (
                    ShortcutAction::ToggleOutputDisplayMode,
                    InputAction::ToggleOutputDisplayMode,
                ),
                (
                    ShortcutAction::Reconnect,
                    InputAction::Command(ClientCommand::Reconnect),
                ),
                (
                    ShortcutAction::Reload,
                    InputAction::Command(ClientCommand::SendText("/reload".to_owned())),
                ),
                (
                    ShortcutAction::Quit,
                    InputAction::Command(ClientCommand::Quit),
                ),
            ] {
                assert_eq!(handle_shortcut(&mut state, shortcut), expected);
                assert_eq!(state.input, "draft 猫");
                assert_eq!(state.cursor, state.input.len());
            }
            assert_eq!(
                handle_shortcut(&mut state, ShortcutAction::SearchOutput),
                InputAction::None
            );
            assert!(state.output_view.search_active);
            assert_eq!(state.input, "draft 猫");
        }
    }

    #[test]
    fn vim_editor_shortcuts_are_suppressed() {
        let mut state = AppState::new(&AppConfig::default());
        state.input_mode = InputMode::Vim;
        state.input = "draft 猫".to_owned();
        state.cursor = state.input.len();
        state.command_history.push("look".to_owned());
        for shortcut in [
            ShortcutAction::Submit,
            ShortcutAction::Backspace,
            ShortcutAction::Delete,
            ShortcutAction::CursorLeft,
            ShortcutAction::CursorRight,
            ShortcutAction::LineStart,
            ShortcutAction::LineEnd,
            ShortcutAction::WordLeft,
            ShortcutAction::WordRight,
            ShortcutAction::DeleteWordLeft,
            ShortcutAction::DeleteWordRight,
            ShortcutAction::HistoryPrevious,
            ShortcutAction::HistoryNext,
            ShortcutAction::CompleteNext,
            ShortcutAction::CompletePrevious,
        ] {
            assert_eq!(handle_shortcut(&mut state, shortcut), InputAction::None);
            assert_eq!(state.input, "draft 猫");
            assert_eq!(state.cursor, state.input.len());
            assert_eq!(state.command_history, ["look"]);
            assert_eq!(state.history_position, None);
        }
    }
}
