use super::*;
use crate::{
    config::{AppConfig, InputMode},
    input::handle_key,
};

fn state(text: &str) -> AppState {
    let mut config = AppConfig::default();
    config.terminal.input_mode = InputMode::Vim;
    let mut state = AppState::new(&config);
    state.input = text.into();
    state.cursor = text.len();
    press(&mut state, KeyCode::Esc);
    state.cursor = 0;
    state
}
fn press(state: &mut AppState, code: KeyCode) -> InputAction {
    handle_key(state, KeyEvent::new(code, KeyModifiers::NONE))
}
fn keys(state: &mut AppState, keys: &str) {
    for ch in keys.chars() {
        assert_eq!(press(state, KeyCode::Char(ch)), InputAction::None);
    }
}

#[test]
fn motions_counts_operators_and_undo() {
    let mut s = state("one two three four");
    keys(&mut s, "2dw");
    assert_eq!(s.input, "three four");
    keys(&mut s, "u");
    assert_eq!(s.input, "one two three four");
    handle_key(
        &mut s,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    assert_eq!(s.input, "three four");
    keys(&mut s, "cwnew");
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.input, "new four");
    keys(&mut s, "w.");
    assert_eq!(s.input, "new new");
}

#[test]
fn objects_visual_registers_and_paste() {
    let mut s = state("say (hello [猫]) world");
    keys(&mut s, "f猫di[");
    assert_eq!(s.input, "say (hello []) world");
    keys(&mut s, "u0viwy");
    keys(&mut s, "wP");
    assert_eq!(s.input, "say say(hello [猫]) world");
    let mut s = state("one two");
    keys(&mut s, "\"ayiw w"); // space is intentionally unsupported
    keys(&mut s, "\"ap");
    assert_eq!(s.input, "one tonewo");
    let mut s = state("abc def");
    keys(&mut s, "vll");
    super::super::insert_paste(&mut s, "/quit", false).unwrap();
    assert_eq!(s.input, "/quit def");
    keys(&mut s, "u");
    assert_eq!(s.input, "abc def");
}

#[test]
fn insert_session_repeat_is_editing_only() {
    let mut s = state("");
    keys(&mut s, "ihello");
    press(&mut s, KeyCode::Esc);
    keys(&mut s, "u");
    assert_eq!(s.input, "");
    keys(&mut s, ".");
    assert_eq!(s.input, "hello");
    let action = press(&mut s, KeyCode::Enter);
    assert!(matches!(action, InputAction::Command(_)));
    assert_eq!(s.vim.mode, Mode::Insert);
    press(&mut s, KeyCode::Esc);
    keys(&mut s, ".");
    assert_eq!(s.input, "hello");
}

#[test]
fn operator_and_motion_examples() {
    for (text, sequence, expected) in [
        ("one two three four five six seven", "2d3w", "seven"),
        ("one-two three", "dw", "-two three"),
        ("one-two three", "dW", "three"),
        ("one-two three", "de", "-two three"),
        ("one-two three", "dE", " three"),
        ("abc", "$dFa", "c"),
        ("abc", "$dTa", "ac"),
        ("abc def", "dfc", " def"),
        ("abc def", "dtc", "c def"),
        ("abc def", "2x", "c def"),
        ("abc def", "$2X", "abc f"),
        ("abc def", "wD", "abc "),
        ("abc def", "3r猫", "猫猫猫 def"),
        ("first\nsecond\nthird", "dd", "second\nthird"),
        ("first\nsecond\nthird", "2dd", "third"),
        ("one\ntwo", "wdd", "one"),
        ("one\ntwo", "wVd", "one"),
        ("one\ntwo", "VcX", "X\ntwo"),
        ("one two", "yiwwviwp", "one one"),
        ("first\nsecond", "yyP", "first\nfirst\nsecond"),
        ("first", "yyp", "first\nfirst"),
        ("a next", "cwZ", "Z next"),
        ("abc next", "llcwZ", "abZ next"),
        ("abc def", "vld", "c def"),
        ("abc def", "vler猫", "猫猫猫 def"),
        ("abc", "$vhr猫", "a猫猫"),
        ("one two", "viwcnew", "new two"),
        ("say 'hello world'", "f'di'", "say ''"),
        ("say (hello [猫])", "f猫da[", "say (hello )"),
        ("a (b (c) d) e", "fc2di(", "a () e"),
        ("plain words", "di(", "plain words"),
        ("abc", "dZ", "abc"),
    ] {
        let mut s = state(text);
        keys(&mut s, sequence);
        assert_eq!(s.input, expected, "{text:?} {sequence}");
        assert!(s.input.is_char_boundary(s.cursor));
        if let Some(range) = s.vim.selection(&s.input, s.cursor) {
            assert!(s.input.get(range).is_some());
        }
    }
}

#[test]
fn visual_unicode_replace_followed_by_delete_is_safe() {
    let mut s = state("abc");
    keys(&mut s, "$vhr猫d$");
    assert_eq!(s.input, "a");
}

#[test]
fn visual_and_change_repeat_undo_and_cancellation() {
    let mut s = state("abc def ghi");
    keys(&mut s, "vldw.");
    assert_eq!(s.input, "c f ghi");
    let mut s = state("first\nsecond\nthird");
    keys(&mut s, "Vd");
    assert_eq!(s.input, "second\nthird");
    keys(&mut s, "u");
    assert_eq!(s.input, "first\nsecond\nthird");
    for prefix in ["d", "2c", "f", "r", "di", "\"", "vll"] {
        let mut s = state("abc def");
        keys(&mut s, prefix);
        press(&mut s, KeyCode::Esc);
        assert_eq!(s.input, "abc def");
        assert_eq!(s.vim.mode, Mode::Normal);
        keys(&mut s, "x");
        assert!(s.input.len() < 7);
    }
}

#[test]
fn initial_insert_undo_and_paste_are_separate_transactions() {
    let mut s = state("");
    s.vim.mode = Mode::Insert;
    keys(&mut s, "abc");
    super::super::insert_paste(&mut s, "XYZ", false).unwrap();
    keys(&mut s, "def");
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.input, "abcXYZdef");
    keys(&mut s, "u");
    assert_eq!(s.input, "abcXYZ");
    keys(&mut s, "u");
    assert_eq!(s.input, "abc");
    keys(&mut s, "u");
    assert_eq!(s.input, "");
    keys(&mut s, ".");
    assert_eq!(s.input, "def");
}

#[test]
fn search_history_and_completion_remain_isolated() {
    let mut s = state("draft");
    s.command_history = vec!["draft old".into(), "draft newer".into()];
    keys(&mut s, "k");
    assert_eq!(s.input, "draft newer");
    keys(&mut s, "k");
    assert_eq!(s.input, "draft old");
    keys(&mut s, "jj");
    assert_eq!(s.input, "draft");
    s.input_completion.active = true;
    keys(&mut s, "l");
    assert!(!s.input_completion.active);
    s.start_output_search();
    keys(&mut s, "xyz");
    assert_eq!(s.input, "draft");
    assert_eq!(s.output_view.search_input, "xyz");
    press(&mut s, KeyCode::Esc);
    assert!(!s.output_view.search_active);
    assert_eq!(s.vim.mode, Mode::Normal);
    keys(&mut s, "dd");
    assert_eq!(s.command_history, ["draft old", "draft newer"]);
}

#[test]
fn paste_limits_are_atomic_and_visual_replacement_can_remove_lines() {
    let mut s = state(&"x".repeat(super::super::MAX_PASTE_BYTES));
    assert!(super::super::insert_paste(&mut s, "x", false).is_err());
    assert_eq!(s.input.len(), super::super::MAX_PASTE_BYTES);
    let mut s = state(&"x\n".repeat(127));
    keys(&mut s, "V");
    super::super::insert_paste(&mut s, "a\nb\n", true).unwrap_err();
    assert_eq!(s.input, "x\n".repeat(127));
    super::super::insert_paste(&mut s, "z\n", true).unwrap();
    assert!(s.input.starts_with("z\nx\n"));
}

#[test]
fn empty_input_and_random_unicode_sequences_do_not_panic() {
    let chars: Vec<char> = "hlwebWEB0^$dcyiaIAvVxXDCuprfFtT;,\"1239()[]{}猫é'"
        .chars()
        .collect();
    let mut seed = 37u64;
    for initial in ["", "a", "é 猫 hello-world\n👩‍💻 e\u{301}"] {
        let mut s = state(initial);
        for _ in 0..1000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            press(&mut s, KeyCode::Char(chars[(seed as usize) % chars.len()]));
            if seed.is_multiple_of(7) {
                press(&mut s, KeyCode::Esc);
            }
            assert!(s.input.is_char_boundary(s.cursor));
            if let Some(range) = s.vim.selection(&s.input, s.cursor) {
                assert!(s.input.get(range).is_some());
            }
        }
    }
}

#[test]
fn memory_and_repeat_work_are_bounded() {
    let mut s = state(&"ab ".repeat(5000));
    keys(&mut s, "x9999.");
    assert!(s.input.len() <= 15000);
    assert!(!s.vim.replaying);
    assert!(s.vim.replay_work.is_none());
    for _ in 0..150 {
        keys(&mut s, "x");
    }
    assert!(s.vim.undo.len() <= MAX_UNDO);
    assert!(s.vim.undo.iter().map(|snap| snap.text.len()).sum::<usize>() <= MAX_UNDO_BYTES);
    let mut s = state("abc");
    keys(&mut s, "d");
    s.vim.reset();
    assert_eq!(s.input, "abc");
    assert_eq!(s.vim.mode, Mode::Insert);
}

#[test]
fn repeat_freezes_completion_result_instead_of_replaying_tab() {
    let mut s = state("");
    s.push_output("elephant", crate::state::OutputCategory::Normal);
    keys(&mut s, "iel");
    press(&mut s, KeyCode::Tab);
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.input, "elephant");
    keys(&mut s, "u");
    s.clear_output();
    s.push_output("electric", crate::state::OutputCategory::Normal);
    keys(&mut s, ".");
    assert_eq!(s.input, "elephant");
}

#[test]
fn repeated_insertions_are_anchored_and_failed_changes_are_atomic() {
    let mut s = state("aaaa xyz");
    keys(&mut s, "ia");
    press(&mut s, KeyCode::Esc);
    keys(&mut s, "w.");
    assert_eq!(s.input, "aaaaa axyz");
    let mut s = state("abc");
    keys(&mut s, "ia");
    press(&mut s, KeyCode::Esc);
    keys(&mut s, "ll.");
    assert_eq!(s.input, "aaabc");
    // A change may delete successfully but its frozen insertion may not fit.
    let mut s = state("x");
    keys(&mut s, "cwlarger");
    press(&mut s, KeyCode::Esc);
    s.input = format!("a {}", "x".repeat(super::super::MAX_PASTE_BYTES - 2));
    s.cursor = 0;
    let before = s.input.clone();
    let undo = s.vim.undo.len();
    keys(&mut s, ".");
    assert_eq!(s.input, before);
    assert_eq!(s.cursor, 0);
    assert_eq!(s.vim.undo.len(), undo);
    assert_eq!(s.vim.mode, Mode::Normal);
}

#[test]
fn insert_cursor_movement_starts_a_new_repeat_transaction() {
    let mut s = state("abc");
    keys(&mut s, "i");
    press(&mut s, KeyCode::Right);
    keys(&mut s, "X");
    press(&mut s, KeyCode::Esc);
    keys(&mut s, "ll.");
    assert_eq!(s.input, "aXbXc");
    for movement in [KeyCode::Home, KeyCode::End] {
        let mut s = state("abc xyz");
        keys(&mut s, "i");
        press(&mut s, movement);
        keys(&mut s, "X");
        press(&mut s, KeyCode::Esc);
        keys(&mut s, "0w.");
        assert!(s.input.contains(" X"), "{}", s.input);
        assert!(s.input.contains("abc"));
    }
}

#[test]
fn repeated_put_rolls_back_if_later_iteration_cannot_fit() {
    let mut s = state("word");
    keys(&mut s, "yiwp");
    s.input = "x".repeat(super::super::MAX_PASTE_BYTES - 5);
    s.cursor = 0;
    let original = s.input.clone();
    keys(&mut s, "2.");
    assert_eq!(s.input, original);
    assert_eq!(s.cursor, 0);
}

#[test]
fn history_search_directions_counts_regex_and_wraparound() {
    let mut s = state("unrelated draft");
    s.command_history = ["say old", "look", "say middle", "say newest"]
        .map(str::to_owned)
        .to_vec();
    keys(&mut s, "/^say ");
    assert!(s.vim.history_search_active());
    assert_eq!(s.input, "unrelated draft");
    assert_eq!(press(&mut s, KeyCode::Enter), InputAction::None);
    assert_eq!(s.input, "say newest");
    assert_eq!(s.history_position, Some(3));
    keys(&mut s, "n");
    assert_eq!(s.input, "say middle");
    keys(&mut s, "N");
    assert_eq!(s.input, "say newest");
    keys(&mut s, "2n");
    assert_eq!(s.input, "say old");
    keys(&mut s, "n");
    assert_eq!(s.input, "say newest");
    keys(&mut s, "?");
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.input, "say old");
    keys(&mut s, "n");
    assert_eq!(s.input, "say middle");
    keys(&mut s, "N");
    assert_eq!(s.input, "say old");
    assert_eq!(s.command_history.len(), 4);
    assert_eq!(s.history_draft.as_deref(), Some("unrelated draft"));
    // Search is separate from submission; the accepted pattern survives submission.
    assert!(matches!(
        press(&mut s, KeyCode::Enter),
        InputAction::Command(_)
    ));
    press(&mut s, KeyCode::Esc);
    keys(&mut s, "n");
    assert_eq!(s.input, "say old");
}

#[test]
fn history_search_cancel_invalid_missing_and_unicode_editing() {
    let mut s = state("draft 猫");
    s.cursor = 2;
    s.command_history = vec!["say 猫".into()];
    keys(&mut s, "/猫");
    press(&mut s, KeyCode::Left);
    keys(&mut s, "é");
    press(&mut s, KeyCode::Backspace);
    assert_eq!(s.vim.history_search_prompt(), Some(("猫", 0, '/')));
    press(&mut s, KeyCode::Esc);
    assert_eq!(s.input, "draft 猫");
    assert_eq!(s.cursor, 2);
    assert_eq!(s.history_position, None);
    keys(&mut s, "/[");
    press(&mut s, KeyCode::Enter);
    assert!(s.vim.history_search_active());
    assert_eq!(s.input, "draft 猫");
    press(&mut s, KeyCode::Home);
    press(&mut s, KeyCode::Delete);
    keys(&mut s, "missing");
    press(&mut s, KeyCode::Enter);
    assert!(!s.vim.history_search_active());
    assert_eq!(s.input, "draft 猫");
    assert_eq!(s.history_position, None);
    keys(&mut s, "/猫");
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.input, "say 猫");
    assert_eq!(s.cursor, 4);
    let mut empty = state("draft");
    keys(&mut empty, "/");
    press(&mut empty, KeyCode::Enter);
    assert!(empty.vim.history_search_active());
    press(&mut empty, KeyCode::Esc);
    keys(&mut empty, "/nothing");
    press(&mut empty, KeyCode::Enter);
    assert_eq!(empty.input, "draft");
    assert!(!empty.vim.history_search_active());
}

#[test]
fn history_search_paste_limits_reset_and_operator_isolation() {
    let mut s = state("draft");
    keys(&mut s, "/");
    super::super::insert_paste(&mut s, "say\n猫", true).unwrap();
    assert_eq!(s.vim.history_search_prompt(), Some(("say 猫", 7, '/')));
    assert!(super::super::insert_paste(&mut s, &"x".repeat(4096), true).is_err());
    assert_eq!(s.input, "draft");
    s.vim.reset();
    assert!(!s.vim.history_search_active());
    assert_eq!(s.input, "draft");
    press(&mut s, KeyCode::Esc);
    s.input = "a/b".into();
    s.cursor = 0;
    keys(&mut s, "f/");
    assert_eq!(s.cursor, 1);
    assert!(!s.vim.history_search_active());
    keys(&mut s, "d/");
    assert!(!s.vim.history_search_active());
    assert_eq!(s.input, "a/b");
    let mut config = AppConfig::default();
    config.terminal.input_mode = InputMode::Standard;
    let mut standard = AppState::new(&config);
    keys(&mut standard, "/help");
    assert_eq!(standard.input, "/help");
}
