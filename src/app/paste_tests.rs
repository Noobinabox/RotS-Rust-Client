use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn app(multiline: bool) -> App {
    let mut config = AppConfig::default();
    config.terminal.multiline_input = multiline;
    config.map.persistence.load_on_startup = false;
    config.lua.enabled = false;
    App::new(config)
}

#[tokio::test]
async fn paste_never_runs_printable_macros_or_pasted_commands() {
    let mut app = app(false);
    app.handle_macro_command("{x} {flee}").unwrap();
    app.handle_macro_command("mode on").unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    assert!(
        !app.handle_terminal_event(TerminalEvent::Paste("x\r\n/quit\n".into()), &tx)
            .await
    );
    assert_eq!(app.state.input, "x /quit ");
    assert!(rx.try_recv().is_err());
    assert!(app.state.command_history.is_empty());
    // Legacy terminals deliver ordinary keys, not Paste events: retain normal
    // macro behavior rather than guessing whether a fast key sequence is paste.
    app.handle_terminal_event(
        TerminalEvent::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
        &tx,
    )
    .await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("flee".into())
    );
}

#[tokio::test]
async fn multiline_submits_lines_only_on_enter_and_quit_stops_remaining_lines() {
    let mut app = app(true);
    let (tx, mut rx) = mpsc::channel(16);
    app.handle_terminal_event(
        TerminalEvent::Paste("look\n\nscore\n/quit\nflee".into()),
        &tx,
    )
    .await;
    assert!(rx.try_recv().is_err());
    assert!(
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx
        )
        .await
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("look".into())
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("score".into())
    );
    assert!(rx.try_recv().is_err());
    assert_eq!(app.state.command_history.len(), 1);
}

#[tokio::test]
async fn alt_enter_adds_line_without_submission_and_repeats_are_ignored() {
    let mut app = app(true);
    app.handle_macro_command("--override {Alt+Enter} {flee}")
        .unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    app.handle_terminal_event(TerminalEvent::Paste("look".into()), &tx)
        .await;
    let mut key = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
    app.handle_terminal_event(TerminalEvent::Key(key), &tx)
        .await;
    key.kind = crossterm::event::KeyEventKind::Repeat;
    app.handle_terminal_event(TerminalEvent::Key(key), &tx)
        .await;
    assert_eq!(app.state.input, "look\n");
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn blank_multiline_draft_is_noop_but_empty_enter_still_sends_blank() {
    let mut app = app(true);
    let (tx, mut rx) = mpsc::channel(16);
    app.handle_terminal_event(TerminalEvent::Paste("\n \n".into()), &tx)
        .await;
    let enter = TerminalEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_terminal_event(enter.clone(), &tx).await;
    assert!(rx.try_recv().is_err());
    app.handle_terminal_event(enter, &tx).await;
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText(String::new())
    );
}

#[tokio::test]
async fn oversized_paste_keeps_draft_and_never_sends() {
    let mut app = app(true);
    let (tx, mut rx) = mpsc::channel(16);
    app.state.input = "draft".into();
    app.state.cursor = 5;
    app.handle_terminal_event(TerminalEvent::PasteTooLarge, &tx)
        .await;
    assert_eq!(app.state.input, "draft");
    assert!(rx.try_recv().is_err());
    assert_eq!(
        app.state.output.back().unwrap().category,
        OutputCategory::Error
    );
}
