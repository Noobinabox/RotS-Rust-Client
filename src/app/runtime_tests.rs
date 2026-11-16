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

#[tokio::test]
async fn large_lua_send_batch_yields_to_network_consumer() {
    let mut app = App::new(AppConfig::default());
    let (tx, mut rx) = mpsc::channel(1);
    let actions = (1..=1000).map(|i| LuaAction::Send(i.to_string())).collect();
    let consume = async {
        for i in 1..=1000 {
            assert_eq!(
                rx.recv().await,
                Some(ClientCommand::SendText(i.to_string()))
            );
        }
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut budgets = Vec::new();
        tokio::join!(app.apply_lua_actions(actions, &tx, &mut budgets), consume);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn lua_execute_allows_large_command_expansions() {
    let mut app = App::new(AppConfig::default());
    let (tx, mut rx) = mpsc::channel(1);
    let commands = (1..=1000)
        .map(|i| format!("look {i}"))
        .collect::<Vec<_>>()
        .join(";");
    let consume = async {
        for i in 1..=1000 {
            assert_eq!(
                rx.recv().await,
                Some(ClientCommand::SendText(format!("look {i}")))
            );
        }
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut budgets = Vec::new();
        tokio::join!(
            app.apply_lua_actions(vec![LuaAction::Execute(commands)], &tx, &mut budgets),
            consume
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stalled_network_does_not_wait_for_every_lua_action() {
    let mut app = App::new(AppConfig::default());
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(ClientCommand::SendText("pending".into()))
        .unwrap();
    let actions = (0..1000).map(|_| LuaAction::Send("look".into())).collect();
    tokio::time::timeout(
        Duration::from_secs(1),
        app.apply_lua_actions(actions, &tx, &mut Vec::new()),
    )
    .await
    .unwrap();
    assert!(app.network_queue_stalled);
    rx.try_recv().unwrap();
    app.send_mud_text_command("recovered", &tx).await;
    assert!(!app.network_queue_stalled);
    assert_eq!(
        rx.try_recv().unwrap(),
        ClientCommand::SendText("recovered".into())
    );
}
