use super::*;
use crate::{config::InputMode, input::vim::Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

fn app() -> App {
    let mut config = AppConfig::default();
    config.terminal.input_mode = InputMode::Vim;
    config.terminal.multiline_input = true;
    config.lua.enabled = false;
    config.map.persistence.load_on_startup = false;
    App::new(config)
}

async fn key(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    tx: &mpsc::Sender<ClientCommand>,
) {
    app.handle_terminal_event(TerminalEvent::Key(KeyEvent::new(code, modifiers)), tx)
        .await;
}

#[tokio::test]
async fn vim_owns_editor_keys_but_keeps_function_and_keypad_macros() {
    let mut app = app();
    for definition in [
        "{h} {flee}",
        "--override {Ctrl+w} {flee}",
        "{F5} {look}",
        "{Numpad8} {score}",
    ] {
        app.handle_macro_command(definition).unwrap();
    }
    app.handle_macro_command("mode on").unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE, &tx).await;
    assert_eq!(app.state.input, "h");
    assert!(rx.try_recv().is_err());
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE, &tx).await;
    key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE, &tx).await;
    key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL, &tx).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(app.state.input, "h");
    key(&mut app, KeyCode::F(5), KeyModifiers::NONE, &tx).await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("look".into())
    );
    let mut keypad = KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE);
    keypad.state = KeyEventState::KEYPAD;
    app.handle_terminal_event(TerminalEvent::Key(keypad), &tx)
        .await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("score".into())
    );
    key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL, &tx).await;
    assert!(app.state.input.is_empty());
    assert_eq!(app.state.vim.mode, Mode::Insert);
    assert!(!app.macros.printable_mode);
    app.state.input = "draft".into();
    app.state.cursor = 5;
    app.state.start_output_search();
    key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL, &tx).await;
    assert!(app.state.input.is_empty());
    assert!(!app.state.output_view.search_active);
    assert_eq!(app.state.vim.mode, Mode::Insert);
}

#[tokio::test]
async fn vim_paste_multiline_submission_search_and_follow_output() {
    let mut app = app();
    let (tx, mut rx) = mpsc::channel(16);
    app.handle_terminal_event(TerminalEvent::Paste("look\nscore".into()), &tx)
        .await;
    assert!(rx.try_recv().is_err());
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE, &tx).await;
    let draft = app.state.input.clone();
    key(&mut app, KeyCode::Enter, KeyModifiers::ALT, &tx).await;
    assert_eq!(app.state.input, draft);
    app.state.output_view.follow_newest = false;
    app.state.output_view.scroll_offset = 5;
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &tx).await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("look".into())
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("score".into())
    );
    assert_eq!(app.state.vim.mode, Mode::Insert);
    assert!(app.state.output_view.follow_newest);
    app.state.start_output_search();
    app.handle_terminal_event(TerminalEvent::Paste("dd /quit".into()), &tx)
        .await;
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &tx).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(app.state.vim.mode, Mode::Insert);
}

#[test]
fn input_mode_configuration_is_optional_and_validated() {
    assert_eq!(
        toml::from_str::<AppConfig>("").unwrap().terminal.input_mode,
        InputMode::Standard
    );
    assert_eq!(
        toml::from_str::<AppConfig>("[terminal]\ninput_mode='vim'")
            .unwrap()
            .terminal
            .input_mode,
        InputMode::Vim
    );
    assert!(toml::from_str::<AppConfig>("[terminal]\ninput_mode='invalid'").is_err());
}

#[tokio::test]
async fn vim_history_search_never_sends_query_or_runs_macros() {
    let mut app = app();
    let (tx, mut rx) = mpsc::channel(16);
    app.handle_macro_command("{F5} {flee}").unwrap();
    app.handle_macro_command("--repeat {NumpadEnter} {flee}")
        .unwrap();
    app.state.input = "draft".into();
    app.state.command_history = vec!["say hello".into()];
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE, &tx).await;
    key(&mut app, KeyCode::Char('/'), KeyModifiers::NONE, &tx).await;
    app.handle_terminal_event(TerminalEvent::Paste("hello".into()), &tx)
        .await;
    key(&mut app, KeyCode::F(5), KeyModifiers::NONE, &tx).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(app.state.input, "draft");
    let mut enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    enter.state = KeyEventState::KEYPAD;
    app.handle_terminal_event(TerminalEvent::Key(enter), &tx)
        .await;
    assert_eq!(app.state.input, "say hello");
    assert!(rx.try_recv().is_err());
    enter.kind = crossterm::event::KeyEventKind::Repeat;
    app.handle_terminal_event(TerminalEvent::Key(enter), &tx)
        .await;
    enter.state = KeyEventState::NONE;
    app.handle_terminal_event(TerminalEvent::Key(enter), &tx)
        .await;
    assert!(rx.try_recv().is_err());
    assert_eq!(app.state.input, "say hello");
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &tx).await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("say hello".into())
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn reload_and_profile_switch_preserve_draft_and_reset_modes() {
    let root = std::env::temp_dir().join(format!(
        "mud-vim-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("characters")).unwrap();
    let path = root.join("config.toml");
    fs::write(&path, "[terminal]\ninput_mode='vim'\n[lua]\nenabled=false\n[map.persistence]\nload_on_startup=false").unwrap();
    fs::write(
        root.join("characters/hero.toml"),
        "[terminal]\ninput_mode='standard'",
    )
    .unwrap();
    let config =
        AppConfig::load_with_options(Some(path.clone()), ConfigLoadOptions::default()).unwrap();
    let mut app =
        App::new_with_config_source(config, Some(path.clone()), ConfigLoadOptions::default());
    app.state.input = "draft".into();
    app.state.cursor = 5;
    crate::input::handle_key(
        &mut app.state,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    );
    crate::input::handle_key(
        &mut app.state,
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
    );
    app.reload_config().unwrap();
    assert_eq!(app.state.input, "draft");
    assert_eq!(app.state.vim.mode, Mode::Insert);
    app.select_msdp_character("hero", &[]).await;
    assert_eq!(app.state.input, "draft");
    assert_eq!(app.state.input_mode, InputMode::Standard);
    app.select_msdp_character("missing", &[]).await;
    assert_eq!(app.state.input, "draft");
    assert_eq!(app.state.input_mode, InputMode::Vim);
    assert_eq!(app.state.vim.mode, Mode::Insert);
    fs::remove_file(root.join("characters/hero.toml")).unwrap();
    fs::remove_dir(root.join("characters")).unwrap();
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}
