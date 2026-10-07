//! Context-aware choices and brace boundaries for local command completion.
use std::collections::BTreeMap;

pub const COMMANDS: &[&str] = &[
    "help",
    "macro",
    "msdp",
    "echo",
    "clear",
    "quit",
    "reload",
    "theme",
    "save",
    "reconnect",
    "lua",
    "timer",
    "alias",
    "trigger",
    "triggers",
    "highlight",
    "substitute",
    "handler",
    "variable",
    "event",
    "toggle",
    "map",
    "path",
];

#[derive(Debug, Clone, Default)]
pub struct CommandCompletions(pub BTreeMap<String, Vec<String>>);

/// The innermost command body, except for braced removal targets which are one literal argument.
pub(crate) struct InputContext {
    pub start: usize,
    pub argument_start: Option<usize>,
}

pub(crate) fn input_context(input: &str, cursor: usize) -> Option<InputContext> {
    let before = input.get(..cursor)?;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let mut opened = Vec::new();
    let mut escaped = false;
    for (offset, value) in before[line_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match value {
            '\\' => escaped = true,
            '{' => opened.push(line_start + offset),
            '}' => {
                opened.pop();
            }
            _ => {}
        }
    }
    let mut context = InputContext {
        start: line_start,
        argument_start: None,
    };
    for open in opened {
        let words: Vec<_> = input[context.start..open].split_whitespace().collect();
        if matches!(
            words.as_slice(),
            [
                "/alias"
                    | "/trigger"
                    | "/triggers"
                    | "/highlight"
                    | "/substitute"
                    | "/handler"
                    | "/variable",
                "unset"
            ] | ["/macro", "remove"]
        ) {
            context.argument_start = Some(open);
            break;
        }
        context.start = open + 1;
    }
    Some(context)
}

impl CommandCompletions {
    /// Restricted contexts remain restricted even when their live candidate list is empty.
    pub(crate) fn has_argument_choices(&self, before: &str) -> bool {
        let words: Vec<_> = before.split_whitespace().collect();
        matches!(
            words.as_slice(),
            [
                "/alias"
                    | "/trigger"
                    | "/triggers"
                    | "/highlight"
                    | "/substitute"
                    | "/handler"
                    | "/variable",
                "unset"
            ] | ["/macro", "remove"]
                | ["/timer", "cancel"]
                | ["/map", "goto" | "find" | "run" | "unlandmark"]
        ) || self.0.contains_key(&words.join(" "))
            || !self.matches(before, "").is_empty()
    }

    pub fn matches(&self, before: &str, prefix: &str) -> Vec<String> {
        let words: Vec<_> = before.split_whitespace().collect();
        let mut choices: Vec<String> = if words.is_empty() {
            COMMANDS.iter().map(|name| format!("/{name}")).collect()
        } else {
            let fixed = match words.as_slice() {
                ["/theme"] => "list current use auto",
                ["/theme", "auto"] | ["/macro", "mode"] | ["/toggle", _] => "on off",
                ["/alias"] => "unset clear",
                ["/trigger" | "/triggers" | "/highlight" | "/substitute" | "/handler"] => {
                    "plain regex unset clear"
                }
                ["/variable"] => "unset",
                ["/lua"] => "status reload call",
                ["/timer"] => "list set cancel clear",
                ["/timer", "set", _, _, _] => "once repeat",
                ["/macro"] => "remove clear mode --override --repeat",
                ["/toggle"] => "group opponent social",
                ["/map"] => {
                    "create goto move dig link unlink delete undo return leave set get info list landmarks landmark unlandmark find run map flag roomflag exitflag door read write help"
                }
                [
                    "/map",
                    "move" | "dig" | "link" | "unlink" | "door" | "exitflag",
                ] => "n ne e se s sw w nw u d",
                ["/map", "set"] => {
                    "roomname roomdesc roomarea roomnote roomterrain roomsymbol roomweight"
                }
                ["/map", "set", "roomterrain"] => {
                    "Inside City Field Forest Hills Mountains Shallowwater Water Rapids Underwater Road Brush Swamp Denseforest"
                }
                ["/map", "flag"] => "static nofollow direction unicode asciigraphics asciivnums",
                ["/map", "roomflag"] => {
                    "avoid block curved fog hide invis leave noglobal static void"
                }
                ["/map", "exitflag", _] => "avoid block hide invis teleport gate boundary",
                ["/map", "flag" | "roomflag", _] | ["/map", "exitflag", _, _] => "on off",
                ["/map", "door", _] => "trigger unknown open closed pickable locked none",
                ["/map", "goto", _] => "dig",
                ["/map", "link", _, _] => "both",
                ["/path"] => {
                    "create start stop destroy describe mapping insert delete undo get goto move walk run swap zip unzip map save load help"
                }
                ["/path", "mapping"] => "stop save",
                ["/path", "get"] => "length position",
                ["/path", "goto"] => "start end",
                ["/path", "move" | "walk"] => "forward backward",
                ["/path", "save"] => "forward backward both",
                _ => "",
            };
            fixed.split_whitespace().map(str::to_owned).collect()
        };
        if words == ["/help"] {
            choices.extend(
                "commands msdp echo variable lua map path alias trigger event substitute highlight animation toggle social output input config diagnostics keybindings reconnect vim save panels theme macro"
                    .split_whitespace()
                    .map(str::to_owned),
            );
        }
        if words == ["/theme", "use"] {
            choices.extend(
                crate::ui::theme::builtin_theme_names()
                    .iter()
                    .map(|s| s.to_string()),
            );
            choices.push("configured".into());
        }
        if let Some(dynamic) = self.0.get(&words.join(" ")) {
            choices.extend(dynamic.iter().cloned());
        }
        let prefix = prefix.to_ascii_lowercase();
        choices.retain(|value| !value.chars().any(char::is_control));
        choices.retain(|value| {
            value.to_ascii_lowercase().starts_with(&prefix)
                || value
                    .strip_prefix('{')
                    .is_some_and(|value| value.to_ascii_lowercase().starts_with(&prefix))
        });
        choices.sort();
        choices.dedup();
        choices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig, state::AppState};

    fn draft_at_cursor(state: &mut AppState, marked: &str) {
        state.clear_input_completion();
        state.cursor = marked.find('|').unwrap();
        state.input = marked.replacen('|', "", 1);
    }

    #[test]
    fn brace_context_boundaries_and_unicode_preserve_surrounding_text() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew rétablir", crate::state::OutputCategory::Normal);
        for (draft, expected) in [
            ("/alias rr {look}{cast r|}", "/alias rr {look}{cast renew}"),
            (
                "/alias rr {look {nested} cast r|}",
                "/alias rr {look {nested} cast renew}",
            ),
            ("/alias rr {cast \\} r|}", "/alias rr {cast \\} renew}"),
            ("/alias rr {  /varia|}", "/alias rr {  /variable}"),
            ("/alias rr {cast\tr|}", "/alias rr {cast\trenew}"),
            (
                "/alias é {cast ré|} {say ü}",
                "/alias é {cast rétablir} {say ü}",
            ),
            (
                "look\n/alias rr {cast r|}\nsay ü",
                "look\n/alias rr {cast renew}\nsay ü",
            ),
            ("/alias rr {cast r|}tail", "/alias rr {cast renew}tail"),
            (
                "/alias rr {cast r|}{cast s}",
                "/alias rr {cast renew}{cast s}",
            ),
        ] {
            draft_at_cursor(&mut state, draft);
            state.complete_input(false);
            assert_eq!(state.input, expected, "{draft}");
            assert!(state.input.is_char_boundary(state.cursor));
        }
    }

    #[test]
    fn empty_and_unmatched_braced_prefixes_do_not_change_draft() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew zzzword", crate::state::OutputCategory::Normal);
        for draft in [
            "/alias rr {|}",
            "/alias rr {cast |}",
            "/alias rr {cast missing|}",
            "/alias rr {/zzzz|}",
            "/alias rr {/theme use zzz|}",
            "/alias rr {/variable test |}",
            "/variable test r|",
        ] {
            draft_at_cursor(&mut state, draft);
            let original = state.input.clone();
            let cursor = state.cursor;
            state.complete_input(false);
            assert_eq!(state.input, original, "{draft}");
            assert_eq!(state.cursor, cursor);
            assert!(!state.input_completion.active);
        }
    }

    #[test]
    fn nested_slash_and_empty_argument_lists_retain_all_choices() {
        let mut state = AppState::new(&AppConfig::default());
        draft_at_cursor(&mut state, "/alias rr {/|}");
        state.complete_input(false);
        assert_eq!(state.input_completion.matches.len(), COMMANDS.len());
        assert!(state.input.ends_with('}'));
        draft_at_cursor(&mut state, "/alias rr {/theme use |}");
        state.complete_input(false);
        assert_eq!(
            state.input_completion.matches.len(),
            crate::ui::theme::builtin_theme_names().len() + 1
        );
        draft_at_cursor(&mut state, "/alias rr {/map door n lo|}");
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {/map door n locked}");
    }

    #[test]
    fn braced_completion_filters_output_categories_and_ansi() {
        use crate::state::OutputCategory;
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew", OutputCategory::Normal);
        state.push_output("\u{1b}[31mrestore\u{1b}[0m renew", OutputCategory::Combat);
        state.push_output("restart", OutputCategory::System);
        state.push_output("reset", OutputCategory::Error);
        draft_at_cursor(&mut state, "/alias rr {cast re|}");
        state.complete_input(false);
        assert_eq!(state.input_completion.matches, ["restore", "renew"]);
        state.complete_input(true);
        assert_eq!(state.input, "/alias rr {cast renew}");
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {cast restore}");
    }

    #[test]
    fn typing_and_search_do_not_reuse_nested_completion_state() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew restore", crate::state::OutputCategory::Normal);
        draft_at_cursor(&mut state, "/alias rr {cast r|}");
        for code in [KeyCode::Tab, KeyCode::Char('x')] {
            crate::input::handle_key(&mut state, KeyEvent::new(code, KeyModifiers::NONE));
        }
        assert_eq!(state.input, "/alias rr {cast renewx}");
        assert!(!state.input_completion.active);
        state.start_output_search();
        let before = state.input.clone();
        crate::input::handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.input, before);
    }

    #[test]
    fn invalid_cursor_boundaries_are_rejected_without_mutation() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "/alias é {cast r}".into();
        for cursor in [state.input.find('é').unwrap() + 1, state.input.len() + 1] {
            state.cursor = cursor;
            let before = state.input.clone();
            state.complete_input(false);
            assert_eq!(state.input, before);
            assert!(!state.input_completion.active);
        }
    }

    #[test]
    fn escaped_braces_in_replaced_word_cannot_become_structural() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew", crate::state::OutputCategory::Normal);
        for draft in [
            "/alias rr {cast r|\\}suffix}",
            "/alias rr {cast r|\\{suffix}",
            "/alias rr {cast r|\\\\\\}suffix}",
        ] {
            draft_at_cursor(&mut state, draft);
            state.complete_input(false);
            assert_eq!(state.input, "/alias rr {cast renew}", "{draft}");
        }
        draft_at_cursor(&mut state, "/alias rr {cast r\\}|suffix}");
        let original = state.input.clone();
        state.complete_input(false);
        assert_eq!(state.input, original);
    }

    #[test]
    fn empty_dynamic_contexts_never_fall_back_to_mud_words() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew", crate::state::OutputCategory::Normal);
        for command in [
            "alias unset",
            "trigger unset",
            "highlight unset",
            "handler unset",
            "substitute unset",
            "variable unset",
            "macro remove",
            "timer cancel",
            "map goto",
            "map find",
            "map run",
            "map unlandmark",
        ] {
            draft_at_cursor(&mut state, &format!("/alias rr {{/{command} r|}}"));
            let original = state.input.clone();
            state.complete_input(false);
            assert_eq!(state.input, original);
            assert!(!state.input_completion.active);
        }
        state
            .command_completions
            .0
            .insert("/timer cancel".into(), vec!["reminder".into()]);
        draft_at_cursor(&mut state, "/alias rr {/timer cancel r|}");
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {/timer cancel reminder}");
        state.command_completions.0.clear();
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {/timer cancel reminder}");
        assert!(!state.input_completion.active);
    }

    #[test]
    fn whole_removal_argument_replacement_respects_nested_and_escaped_suffixes() {
        let mut state = AppState::new(&AppConfig::default());
        state
            .command_completions
            .0
            .insert("/alias unset".into(), vec!["{hello world}".into()]);
        for draft in [
            "/alias rr {/alias unset {hel|{nested}}} {look}",
            "/alias rr {/alias unset {hel|\\}suffix}} {look}",
        ] {
            draft_at_cursor(&mut state, draft);
            state.complete_input(false);
            assert_eq!(state.input, "/alias rr {/alias unset {hello world}} {look}");
        }
    }

    #[test]
    fn braced_completion_uses_mud_words_or_nested_slash_commands() {
        for (draft, expected) in [
            ("/alias rr {cast r|}", "/alias rr {cast renew}"),
            ("/alias rr {/varia|}", "/alias rr {/variable}"),
            (
                "/alias rr {/variable test r|}",
                "/alias rr {/variable test renew}",
            ),
            (
                "/alias {rr} {cast r|} {look}",
                "/alias {rr} {cast renew} {look}",
            ),
            (
                "/alias rr {/alias xx {cast r|}}",
                "/alias rr {/alias xx {cast renew}}",
            ),
            (
                "/alias rr {cast r|suffix target}",
                "/alias rr {cast renew target}",
            ),
            ("/alias rr {cast r|", "/alias rr {cast renew"),
            ("/alias rr {cast \\{ r|}", "/alias rr {cast \\{ renew}"),
            (
                "/alias rr {/theme use ho|}",
                "/alias rr {/theme use hobbit}",
            ),
        ] {
            let mut state = AppState::new(&AppConfig::default());
            state.push_output("renew restore", crate::state::OutputCategory::Normal);
            state.cursor = draft.find('|').unwrap();
            state.input = draft.replace('|', "");
            state.complete_input(false);
            assert_eq!(state.input, expected, "{draft}");
        }
    }

    #[test]
    fn braced_word_completion_cycles_without_changing_braces_or_suffix() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("renew restore", crate::state::OutputCategory::Normal);
        state.input = "/alias rr {/variable test r} {say ü}".into();
        state.cursor = "/alias rr {/variable test r".len();
        state.complete_input(false);
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {/variable test restore} {say ü}");
        state.complete_input(true);
        assert_eq!(state.input, "/alias rr {/variable test renew} {say ü}");
        state.input = "/alias rr {/theme use zzz}".into();
        state.cursor = state.input.len() - 1;
        state.clear_input_completion();
        state.push_output("zzzz", crate::state::OutputCategory::Normal);
        state.complete_input(false);
        assert_eq!(state.input, "/alias rr {/theme use zzz}");
        assert!(!state.input_completion.active);
    }

    #[test]
    fn slash_completion_filters_cycles_and_preserves_suffix() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "/theme use h trailing ü".into();
        state.cursor = "/theme use h".len();
        state.complete_input(false);
        assert_eq!(
            state.input_completion.matches,
            ["haradrim", "hobbit", "human"]
        );
        assert_eq!(state.input, "/theme use haradrim trailing ü");
        state.complete_input(false);
        assert_eq!(state.input, "/theme use hobbit trailing ü");
        state.complete_input(true);
        assert_eq!(state.input, "/theme use haradrim trailing ü");
    }

    #[test]
    fn empty_prefix_lists_all_choices_and_unknown_context_is_empty() {
        let choices = CommandCompletions::default();
        assert_eq!(choices.matches("", "/").len(), COMMANDS.len());
        assert_eq!(
            choices.matches("/theme use ", "").len(),
            crate::ui::theme::builtin_theme_names().len() + 1
        );
        for theme in [
            "solarized-light",
            "gruvbox-light",
            "rose-pine-dawn",
            "github-light",
            "everforest-dark",
            "kanagawa-wave",
            "rose-pine-moon",
            "everforest-light",
            "kanagawa-dragon",
            "kanagawa-lotus",
            "rose-pine",
            "tokyo-night-storm",
            "tokyo-night-moon",
            "tokyo-night-day",
        ] {
            assert!(
                choices
                    .matches("/theme use ", "")
                    .contains(&theme.to_string()),
                "missing completion for {theme}"
            );
        }
        assert!(choices.matches("/map ", "").contains(&"landmark".into()));
        assert_eq!(choices.matches("/alias ", "u"), ["unset"]);
        assert!(choices.matches("/echo ", "").is_empty());
        assert!(choices.matches("/not-a-command ", "x").is_empty());
        assert_eq!(choices.matches("/map door n ", "lo"), ["locked"]);
    }

    #[test]
    fn completion_handles_empty_arguments_multiline_and_no_match() {
        let mut state = AppState::new(&AppConfig::default());
        state.input = "look\n/theme use ".into();
        state.cursor = state.input.len();
        state.complete_input(false);
        assert!(state.input_completion.matches.len() > 12);
        assert!(state.input.starts_with("look\n/theme use "));
        state.input = "/theme use zzzz".into();
        state.cursor = state.input.len();
        state.complete_input(false);
        assert!(!state.input_completion.active);
        assert_eq!(state.input, "/theme use zzzz");
    }
}
