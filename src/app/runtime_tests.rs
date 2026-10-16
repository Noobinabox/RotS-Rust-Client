use super::*;

#[tokio::test]
async fn full_command_queue_does_not_stall_input_or_record_unsent_movement() {
    let mut app = App::new(AppConfig::default());
    let (tx, mut rx) = mpsc::channel(1);
    tx.send(ClientCommand::SendText("queued".into()))
        .await
        .unwrap();
    let original_room = app.state.map.current_room.clone();
    tokio::time::timeout(Duration::from_secs(1), app.send_mud_text_command("n", &tx))
        .await
        .unwrap();
    assert_eq!(app.state.map.current_room, original_room);
    assert!(
        app.state
            .output
            .back()
            .unwrap()
            .normalized
            .contains("queue is full")
    );
    app.handle_terminal_event(
        TerminalEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        )),
        &tx,
    )
    .await;
    assert_eq!(app.state.input, "x");
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("queued".into())
    );
    assert!(rx.try_recv().is_err());
}

#[test]
fn closed_command_queue_reports_failure() {
    let mut app = App::new(AppConfig::default());
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    assert!(!app.queue_network_command(ClientCommand::Reconnect, &tx));
    assert!(
        app.state
            .output
            .back()
            .unwrap()
            .normalized
            .contains("queue is closed")
    );
}

#[tokio::test]
async fn reconnect_bypasses_a_full_command_queue() {
    let mut app = App::new(AppConfig::default());
    let (tx, _rx) = mpsc::channel(1);
    tx.try_send(ClientCommand::SendText("pending".into()))
        .unwrap();
    let (controls, mut control_rx) = watch::channel(None);
    app.network_controls = Some(controls);
    assert!(app.queue_network_command(ClientCommand::Reconnect, &tx));
    control_rx.changed().await.unwrap();
    assert_eq!(*control_rx.borrow(), Some(ConnectionControl::Reconnect));
}
