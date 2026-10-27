//! Character history lifecycle; disk work runs on Tokio's blocking pool.
use super::*;
use crate::history::HistoryStore;

#[derive(Default)]
pub(super) struct CommandHistory {
    pub(super) active: Option<String>,
    sessions: BTreeMap<String, Session>,
    pub(super) save: Option<JoinHandle<(String, HistoryStore, std::io::Result<()>)>>,
    next_save: Option<Instant>,
    saving_name: Option<String>,
}

#[derive(Default)]
struct Session {
    entries: Vec<String>,
    store: Option<HistoryStore>,
    attempted_load: bool,
    dirty: bool,
    persist: bool,
}

impl App {
    pub(super) fn capture_history(&mut self) {
        let Some(name) = &self.command_history.active else {
            return;
        };
        let session = self
            .command_history
            .sessions
            .entry(name.clone())
            .or_default();
        session.persist = self.state.history_config.persist;
        if session.entries != self.state.command_history {
            session.entries.clone_from(&self.state.command_history);
            session.dirty = session.persist;
        }
    }

    pub(super) fn deactivate_history(&mut self) {
        if !self.state.local_echo {
            self.state.input.clear();
            self.state.cursor = 0;
            self.state.cancel_output_search();
        }
        self.capture_history();
        self.command_history.active = None;
        self.replace_history(Vec::new());
    }

    fn replace_history(&mut self, entries: Vec<String>) {
        self.state.command_history = entries;
        self.state.history_position = None;
        self.state.history_draft = None;
        self.state.submitted_input = None;
        self.state.submitted_input_selected = false;
        self.state.clear_input_completion();
        // Vim search/undo/registers must not retain another character's input.
        self.state.vim = crate::input::vim::VimEditor::default();
    }

    pub(super) async fn activate_history(&mut self, name: &str) {
        let name = name.trim().to_ascii_lowercase();
        if crate::profiles::character_path(Path::new("config.toml"), &name).is_err() {
            self.deactivate_history();
            if self.config.history.persist && !name.is_empty() {
                self.state.push_output(
                    "Command history disabled for an invalid character identifier.",
                    OutputCategory::Error,
                );
            }
            return;
        }
        self.capture_history();
        let changed = self.command_history.active.as_deref() != Some(&name);
        let config = self.config.history.clone();
        if !changed
            && self.state.history_config == config
            && self
                .command_history
                .sessions
                .get(&name)
                .is_some_and(|session| !config.persist || session.attempted_load)
        {
            return;
        }
        self.state.history_config = config.clone();
        let session = self
            .command_history
            .sessions
            .entry(name.clone())
            .or_default();
        session.dirty |= config.persist && !session.persist;
        session.persist = config.persist;
        if config.persist && !session.attempted_load {
            session.attempted_load = true;
            if let Some(base) = self.config_path.clone() {
                let load_name = name.clone();
                let load_config = config.clone();
                match tokio::task::spawn_blocking(move || {
                    HistoryStore::load(&base, &load_name, &load_config)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()))
                {
                    Ok((store, mut entries)) => {
                        entries.append(&mut session.entries);
                        config.trim(&mut entries);
                        session.entries = entries;
                        session.store = Some(store);
                        session.dirty = true;
                    }
                    Err(error) => {
                        self.state.push_output(format!("Command history not loaded for {name}: {error}. Saving is disabled for this character until restart."), OutputCategory::Error);
                    }
                }
            } else {
                self.state.push_output(
                    "Command history persistence needs a configuration path; using memory only.",
                    OutputCategory::Error,
                );
            }
        }
        let before = session.entries.clone();
        config.trim(&mut session.entries);
        session.dirty |= config.persist && session.entries != before;
        let entries = session.entries.clone();
        self.command_history.active = Some(name);
        if changed {
            self.replace_history(entries);
        } else {
            self.state.command_history = entries;
            self.state.history_position = None;
            self.state.history_draft = None;
        }
    }

    pub(super) async fn refresh_history(&mut self) {
        if let Some(name) = self.command_history.active.clone() {
            self.activate_history(&name).await;
        } else {
            self.state.history_config = self.config.history.clone();
            self.state
                .history_config
                .trim(&mut self.state.command_history);
        }
    }

    pub(super) fn start_history_save(&mut self, force: bool) {
        if self.command_history.save.is_some()
            || (!force
                && self
                    .command_history
                    .next_save
                    .is_some_and(|due| due > Instant::now()))
        {
            return;
        }
        let Some((name, session)) = self
            .command_history
            .sessions
            .iter_mut()
            .find(|(_, session)| session.persist && session.dirty && session.store.is_some())
        else {
            return;
        };
        let name = name.clone();
        let Some(mut store) = session.store.take() else {
            return;
        };
        let entries = session.entries.clone();
        session.dirty = false;
        self.command_history.next_save = Some(Instant::now() + Duration::from_secs(1));
        self.command_history.saving_name = Some(name.clone());
        self.command_history.save = Some(tokio::task::spawn_blocking(move || {
            let result = store.save(entries);
            (name, store, result)
        }));
    }

    /// Failures during normal ticks are displayed; shutdown propagates them
    /// after terminal restoration so a failed final write cannot disappear.
    pub(super) async fn finish_history_save(&mut self) -> Option<String> {
        let task = self.command_history.save.take()?;
        self.command_history.saving_name = None;
        let message = match task.await {
            Ok((name, store, result)) => match result {
                Ok(()) => {
                    if let Some(session) = self.command_history.sessions.get_mut(&name) {
                        session.store = Some(store);
                    }
                    return None;
                }
                Err(error) => format!(
                    "Command history save failed for {name}: {error}. Saving is disabled for this character until restart."
                ),
            },
            Err(error) => format!("Command history worker failed: {error}"),
        };
        tracing::warn!(target: "mud_client::history", "[App.finish_history_save] {message}");
        self.state.push_output(&message, OutputCategory::Error);
        Some(message)
    }

    pub(super) async fn flush_history(&mut self) -> Option<String> {
        self.capture_history();
        let mut error = None;
        loop {
            if let Some(message) = self.finish_history_save().await {
                error.get_or_insert(message);
            }
            self.start_history_save(true);
            if self.command_history.save.is_none() {
                break;
            }
        }
        error
    }

    pub(super) fn history_persistence_status(&self) -> &'static str {
        if !self.config.history.persist {
            return "disabled";
        }
        let Some(name) = &self.command_history.active else {
            return "waiting for character identity";
        };
        if self.command_history.saving_name.as_ref() == Some(name) {
            return "enabled (save pending)";
        }
        if self
            .command_history
            .sessions
            .get(name)
            .is_some_and(|session| session.store.is_some())
        {
            "enabled"
        } else {
            "unavailable (see earlier history error; restart to retry)"
        }
    }

    pub(super) fn clear_history(&mut self) {
        self.replace_history(Vec::new());
        self.capture_history();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        events::NetworkEvent,
        history::tests::Fixture,
        network::msdp::{MsdpFrame, MsdpValue},
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn app(fixture: &Fixture, persist: bool) -> App {
        let mut config = AppConfig::default();
        config.history.persist = persist;
        config.history.max_entries = 3;
        config.lua.enabled = false;
        config.map.persistence.load_on_startup = false;
        config.map.persistence.save_on_exit = false;
        fs::write(fixture.base(), toml::to_string(&config).unwrap()).unwrap();
        App::new_with_config_source(config, Some(fixture.base()), ConfigLoadOptions::default())
    }

    async fn identify(app: &mut App, name: &str) {
        let (tx, _rx) = mpsc::channel(64);
        app.handle_network_event(
            NetworkEvent::Msdp(vec![MsdpFrame {
                variable: "CHARACTER_NAME".into(),
                value: MsdpValue::String(name.into()),
            }]),
            &tx,
        )
        .await;
    }

    async fn submit(app: &mut App, text: &str) {
        let (tx, _rx) = mpsc::channel(64);
        app.state.submitted_input_selected = false;
        app.state.input = text.into();
        app.state.cursor = text.len();
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx,
        )
        .await;
    }

    #[tokio::test]
    async fn persistent_history_is_bounded_isolated_and_restored_without_profile_files() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        submit(&mut first, "login-password").await;
        identify(&mut first, "Hero").await;
        assert!(first.state.command_history.is_empty());
        for text in [
            "look",
            "look",
            "say café",
            "score",
            "north",
            " secret",
            "password secret",
        ] {
            submit(&mut first, text).await;
        }
        assert_eq!(first.state.command_history, ["say café", "score", "north"]);
        identify(&mut first, "other").await;
        assert!(first.state.command_history.is_empty());
        assert!(first.state.submitted_input.is_none());
        submit(&mut first, "south").await;
        identify(&mut first, "hero").await;
        assert_eq!(first.state.command_history, ["say café", "score", "north"]);
        assert!(first.flush_history().await.is_none());
        let mut restarted = app(&fixture, true);
        identify(&mut restarted, "hero").await;
        assert_eq!(
            restarted.state.command_history,
            ["say café", "score", "north"]
        );
        let (tx, _rx) = mpsc::channel(4);
        restarted
            .handle_terminal_event(
                TerminalEvent::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
                &tx,
            )
            .await;
        assert_eq!(restarted.state.input, "north");
        identify(&mut restarted, "other").await;
        assert_eq!(restarted.state.command_history, ["south"]);
        let bytes = fs::read_to_string(fixture.0.join("history/hero.toml")).unwrap();
        assert!(!bytes.contains("secret") && !bytes.contains("login-password"));
    }

    #[tokio::test]
    async fn disconnect_excludes_login_and_clear_replaces_saved_history() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        identify(&mut first, "hero").await;
        submit(&mut first, "look").await;
        first.start_history_save(true);
        // Edits while a save is pending must be flushed in a subsequent snapshot.
        submit(&mut first, "score").await;
        assert!(first.flush_history().await.is_none());
        let (tx, _rx) = mpsc::channel(4);
        first
            .handle_network_event(NetworkEvent::Disconnected, &tx)
            .await;
        submit(&mut first, "private-login").await;
        first
            .handle_network_event(NetworkEvent::Connected, &tx)
            .await;
        submit(&mut first, "another-private-login").await;
        identify(&mut first, "hero").await;
        assert_eq!(first.state.command_history, ["look", "score"]);
        submit(&mut first, "/history clear").await;
        assert!(first.flush_history().await.is_none());
        let mut restarted = app(&fixture, true);
        identify(&mut restarted, "hero").await;
        assert!(restarted.state.command_history.is_empty());
    }

    #[tokio::test]
    async fn disabled_persistence_reloads_limits_and_enables_without_losing_live_history() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, false);
        identify(&mut first, "hero").await;
        submit(&mut first, "look").await;
        assert!(first.flush_history().await.is_none());
        assert!(!fixture.0.join("history").exists());
        first.config.history.persist = true;
        first.config.history.max_entries = 1;
        first.refresh_history().await;
        assert!(first.flush_history().await.is_none());
        let mut restarted = app(&fixture, true);
        identify(&mut restarted, "hero").await;
        assert_eq!(restarted.state.command_history, ["look"]);
        restarted.config.history.max_entries = 0;
        restarted.refresh_history().await;
        assert!(restarted.state.command_history.is_empty());
        submit(&mut restarted, "score").await;
        assert!(restarted.state.command_history.is_empty());
    }

    #[tokio::test]
    async fn invalid_history_is_visible_and_never_overwritten() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        fs::create_dir(fixture.0.join("history")).unwrap();
        let path = fixture.0.join("history/hero.toml");
        fs::write(&path, "broken").unwrap();
        identify(&mut first, "hero").await;
        submit(&mut first, "look").await;
        assert!(first.flush_history().await.is_none());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken");
        assert!(
            first
                .state
                .output
                .iter()
                .any(|line| line.raw.contains("Command history not loaded"))
        );
    }
    #[tokio::test]
    async fn re_enabling_previously_loaded_persistence_saves_live_commands() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        identify(&mut first, "hero").await;
        submit(&mut first, "look").await;
        assert!(first.flush_history().await.is_none());
        first.config.history.persist = false;
        first.refresh_history().await;
        submit(&mut first, "score").await;
        first.config.history.persist = true;
        first.refresh_history().await;
        assert!(first.flush_history().await.is_none());
        let mut restarted = app(&fixture, true);
        identify(&mut restarted, "hero").await;
        assert_eq!(restarted.state.command_history, ["look", "score"]);
    }

    #[tokio::test]
    async fn final_flush_reports_failure_and_status_does_not_claim_writes_work() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        identify(&mut first, "hero").await;
        submit(&mut first, "look").await;
        // Occupy the directory with a file to force the final write to fail.
        fs::write(fixture.0.join("history"), "blocked").unwrap();
        assert!(
            first
                .flush_history()
                .await
                .unwrap()
                .contains("Command history save failed")
        );
        assert!(
            first
                .history_persistence_status()
                .starts_with("unavailable")
        );
        submit(&mut first, "/history status").await;
        assert!(
            first
                .state
                .output
                .back()
                .unwrap()
                .raw
                .contains("unavailable")
        );
        submit(&mut first, "/history clear").await;
        assert!(
            first
                .state
                .output
                .back()
                .unwrap()
                .raw
                .contains("saved file was not changed")
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("history")).unwrap(),
            "blocked"
        );
    }

    #[tokio::test]
    async fn same_connection_password_entry_never_enters_history_or_command_pipeline() {
        use crate::network::telnet::{IAC, TELOPT_ECHO, TelnetEvent, TelnetParser, WILL, WONT};
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        identify(&mut first, "hero").await;
        submit(&mut first, "rent").await;
        // RoTS can return to its menu and reset a password on the same socket.
        let (tx, mut rx) = mpsc::channel(64);
        let mut parser = TelnetParser::default();
        for event in parser.push(&[IAC, WILL, TELOPT_ECHO]) {
            if let TelnetEvent::LocalEcho(enabled) = event {
                first
                    .handle_network_event(NetworkEvent::LocalEcho(enabled), &tx)
                    .await;
            }
        }
        first.state.input_mode = crate::config::InputMode::Vim;
        first.state.input = "${secret};raw-password".into();
        first.state.cursor = first.state.input.len();
        first
            .handle_terminal_event(
                TerminalEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                &tx,
            )
            .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("${secret};raw-password".into())
        );
        assert!(first.state.command_history.is_empty());
        assert!(first.state.submitted_input.is_none());
        for event in parser.push(&[IAC, WONT, TELOPT_ECHO]) {
            if let TelnetEvent::LocalEcho(enabled) = event {
                first
                    .handle_network_event(NetworkEvent::LocalEcho(enabled), &tx)
                    .await;
            }
        }
        // REPORT forces an unchanged name to be sent when RoTS reenters play.
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendRaw(crate::network::telnet::wrap_msdp(
                &crate::network::msdp::encode_pair("REPORT", "CHARACTER_NAME")
            ))
        );
        first.state.input_mode = crate::config::InputMode::Standard;
        submit(&mut first, "menu-choice").await;
        assert!(first.flush_history().await.is_none());
        identify(&mut first, "hero").await;
        assert_eq!(first.state.command_history, ["rent"]);
        let contents = fs::read_to_string(fixture.0.join("history/hero.toml")).unwrap();
        assert!(!contents.contains("password") && !contents.contains("menu-choice"));
    }
    #[tokio::test]
    async fn connection_changes_discard_partially_typed_passwords() {
        for event in [
            NetworkEvent::Disconnected,
            NetworkEvent::Error("network failed".into()),
            NetworkEvent::Connected,
        ] {
            let fixture = Fixture::new();
            let mut first = app(&fixture, true);
            identify(&mut first, "hero").await;
            let (tx, _rx) = mpsc::channel(4);
            first
                .handle_network_event(NetworkEvent::LocalEcho(false), &tx)
                .await;
            first.state.input = "unfinished secret".into();
            first.state.cursor = first.state.input.len();
            first.handle_network_event(event, &tx).await;
            assert!(first.state.local_echo);
            assert!(first.state.input.is_empty());
            assert_eq!(first.state.cursor, 0);
            identify(&mut first, "hero").await;
            assert!(first.state.command_history.is_empty());
        }
    }
    #[tokio::test]
    async fn password_mode_has_explicit_quit_and_reconnect_recovery() {
        let fixture = Fixture::new();
        let mut first = app(&fixture, true);
        first.config.keybindings = toml::from_str::<AppConfig>("[[keybindings.bindings]]\nkey='F8'\naction='reconnect'\n[[keybindings.bindings]]\nkey='F9'\naction='quit'\n").unwrap().keybindings;
        first.keybindings =
            crate::keybindings::KeybindingEngine::new(&first.config.keybindings).unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        first
            .handle_network_event(NetworkEvent::LocalEcho(false), &tx)
            .await;
        assert!(
            !first
                .handle_terminal_event(
                    TerminalEvent::Key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE)),
                    &tx
                )
                .await
        );
        assert_eq!(rx.try_recv().unwrap(), ClientCommand::Reconnect);
        assert!(
            first
                .handle_terminal_event(
                    TerminalEvent::Key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE)),
                    &tx
                )
                .await
        );
        assert!(
            first
                .handle_terminal_event(
                    TerminalEvent::Key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL)),
                    &tx
                )
                .await
        );
        assert!(first.state.command_history.is_empty());
    }
}
