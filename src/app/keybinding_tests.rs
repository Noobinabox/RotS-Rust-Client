use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

fn configured(bindings: &str) -> App {
    let config: AppConfig = toml::from_str(bindings).unwrap();
    config.validate().unwrap();
    App::new(config)
}

async fn press(app: &mut App, key: KeyEvent, tx: &mpsc::Sender<ClientCommand>) -> bool {
    app.handle_terminal_event(TerminalEvent::Key(key), tx).await
}

#[tokio::test]
async fn custom_shortcuts_preserve_defaults_and_require_macro_override() {
    let mut app = configured(
        r#"
[[keybindings.bindings]]
key = "F6"
action = "search_output"
[[keybindings.bindings]]
key = "Ctrl+L"
action = "none"
"#,
    );
    let (tx, mut rx) = mpsc::channel(8);
    app.state.push_output("keep", OutputCategory::Normal);
    press(
        &mut app,
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
        &tx,
    )
    .await;
    assert_eq!(app.state.output.len(), 1);
    assert!(app.handle_macro_command("{F6} {look}").is_err());
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert!(app.state.output_view.search_active);
    press(
        &mut app,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &tx,
    )
    .await;
    app.handle_macro_command("--override {F6} {look}").unwrap();
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("look".into())
    );
    assert!(!app.state.output_view.search_active);
    press(
        &mut app,
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        &tx,
    )
    .await;
    assert!(app.state.output_view.search_active);
}

#[tokio::test]
async fn custom_submit_uses_pipeline_but_repeats_and_releases_do_not_execute() {
    let mut app = configured(
        r#"
[[keybindings.bindings]]
key = "F7"
action = "submit"
[[keybindings.bindings]]
key = "F8"
action = "reconnect"
"#,
    );
    let (tx, mut rx) = mpsc::channel(8);
    app.state.input = "look;score".into();
    app.state.cursor = app.state.input.len();
    let mut key = KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE);
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        key.kind = kind;
        press(&mut app, key, &tx).await;
        assert!(rx.try_recv().is_err());
    }
    key.kind = KeyEventKind::Press;
    press(&mut app, key, &tx).await;
    for text in ["look", "score"] {
        assert_eq!(rx.try_recv().unwrap(), ClientCommand::SendText(text.into()));
    }
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert_eq!(rx.try_recv().unwrap(), ClientCommand::Reconnect);
}

#[tokio::test]
async fn search_vim_and_recovery_keys_keep_ownership() {
    let mut app = configured(
        r#"
[[keybindings.bindings]]
key = "F8"
action = "quit"
[[keybindings.bindings]]
key = "Alt+J"
action = "quit"
"#,
    );
    let (tx, mut rx) = mpsc::channel(8);
    app.state.start_output_search();
    assert!(
        !press(
            &mut app,
            KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
            &tx
        )
        .await
    );
    assert!(app.state.output_view.search_active);
    app.state.cancel_output_search();
    app.state.input_mode = crate::config::InputMode::Vim;
    assert!(
        !press(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT),
            &tx
        )
        .await
    );
    app.macros.printable_mode = true;
    press(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &tx,
    )
    .await;
    assert!(!app.macros.printable_mode);
    assert!(rx.try_recv().is_err());
    assert!(
        press(
            &mut app,
            KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
            &tx
        )
        .await
    );
}

#[tokio::test]
async fn reload_replaces_bindings_and_rejects_runtime_macro_conflicts_atomically() {
    let path = std::env::temp_dir().join(format!(
        "mud-client-keybindings-{}.toml",
        std::process::id()
    ));
    let initial = "[[keybindings.bindings]]\nkey = 'F6'\naction = 'search_output'\n";
    fs::write(&path, initial).unwrap();
    let config = AppConfig::load(Some(path.clone())).unwrap();
    let mut app =
        App::new_with_config_source(config, Some(path.clone()), ConfigLoadOptions::default());
    app.handle_macro_command("{F7} {look}").unwrap();
    fs::write(
        &path,
        "[[keybindings.bindings]]\nkey = 'F7'\naction = 'quit'\n",
    )
    .unwrap();
    let (tx, mut rx) = mpsc::channel(8);
    app.handle_command(ClientCommand::SendText("/reload".into()), &tx)
        .await;
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert!(app.state.output_view.search_active);
    app.state.cancel_output_search();
    app.handle_macro_command("remove {F7}").unwrap();
    app.handle_command(ClientCommand::SendText("/reload".into()), &tx)
        .await;
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert!(!app.state.output_view.search_active);
    assert!(
        press(
            &mut app,
            KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE),
            &tx
        )
        .await
    );
    assert!(rx.try_recv().is_err());
    fs::remove_file(path).unwrap();
}

#[test]
fn shortcut_help_reuses_documentation() {
    assert_eq!(
        help_text("keybindings"),
        Some(include_str!("../../docs/commands/keybindings.md"))
    );
    assert_eq!(help_text("shortcuts"), help_text("keybindings"));
}

#[tokio::test]
async fn ctrl_c_recovers_standard_search_even_with_uppercase_terminal_reporting() {
    let mut app = App::new(AppConfig::default());
    let (tx, mut rx) = mpsc::channel(1);
    app.state.input = "draft".into();
    app.state.cursor = 5;
    app.macros.printable_mode = true;
    app.state.start_output_search();
    press(
        &mut app,
        KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL),
        &tx,
    )
    .await;
    assert!(!app.state.output_view.search_active);
    assert!(!app.macros.printable_mode);
    assert!(app.state.input.is_empty());
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn character_switch_restores_cached_profile_shortcuts() {
    let root = std::env::temp_dir().join(format!(
        "mud-client-shortcut-profiles-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("characters")).unwrap();
    let path = root.join("config.toml");
    let profile = root.join("characters/hero.toml");
    fs::write(&path, "[lua]\nenabled=false\n[map.persistence]\nload_on_startup=false\n[[keybindings.bindings]]\nkey='F6'\naction='search_output'\n").unwrap();
    fs::write(
        &profile,
        "[[keybindings.bindings]]\nkey='F6'\naction='clear_output'\n",
    )
    .unwrap();
    let config = AppConfig::load(Some(path.clone())).unwrap();
    let mut app =
        App::new_with_config_source(config, Some(path.clone()), ConfigLoadOptions::default());
    let (tx, mut rx) = mpsc::channel(8);
    let key = KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE);
    press(&mut app, key, &tx).await;
    assert!(app.state.output_view.search_active);
    app.state.cancel_output_search();

    app.select_msdp_character("hero", &[]).await;
    app.state
        .push_output("character output", OutputCategory::Normal);
    press(&mut app, key, &tx).await;
    assert!(app.state.output.is_empty());
    assert!(!app.state.output_view.search_active);

    app.select_msdp_character("missing", &[]).await;
    press(&mut app, key, &tx).await;
    assert!(app.state.output_view.search_active);
    app.state.cancel_output_search();
    // Returning to this profile uses its parked session, not a new disk load.
    fs::write(
        &profile,
        "[[keybindings.bindings]]\nkey='F6'\naction='quit'\n",
    )
    .unwrap();
    app.select_msdp_character("hero", &[]).await;
    app.state
        .push_output("cached character output", OutputCategory::Normal);
    assert!(!press(&mut app, key, &tx).await);
    assert!(app.state.output.is_empty());
    assert!(!app.state.output_view.search_active);
    assert!(rx.try_recv().is_err());
    fs::remove_file(profile).unwrap();
    fs::remove_dir(root.join("characters")).unwrap();
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}

#[tokio::test]
async fn conflicting_saved_macro_is_rejected_and_configured_engines_remain_active() {
    let root = std::env::temp_dir().join(format!(
        "mud-client-shortcut-snapshot-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("config.toml");
    fs::write(&path, "[lua]\nenabled=false\n[map.persistence]\nload_on_startup=false\n[[keybindings.bindings]]\nkey='F6'\naction='search_output'\n[[macros.rules]]\nkey='F7'\ncommand='look'\n").unwrap();
    let snapshot = root.join("config.toml.runtime.toml");
    let saved = "version=1\n[[macros]]\nkey='F6'\ncommand='score'\n";
    fs::write(&snapshot, saved).unwrap();
    let config = AppConfig::load(Some(path.clone())).unwrap();
    let mut app =
        App::new_with_config_source(config, Some(path.clone()), ConfigLoadOptions::default());
    assert!(app.state.output.iter().any(|line| {
        line.normalized
            .contains("Saved runtime settings not loaded")
    }));
    let (tx, mut rx) = mpsc::channel(8);
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert!(app.state.output_view.search_active);
    assert!(rx.try_recv().is_err());
    app.state.cancel_output_search();
    press(
        &mut app,
        KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE),
        &tx,
    )
    .await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("look".into())
    );
    assert_eq!(fs::read_to_string(&snapshot).unwrap(), saved);
    fs::remove_file(snapshot).unwrap();
    fs::remove_file(path).unwrap();
    fs::remove_dir(root).unwrap();
}
