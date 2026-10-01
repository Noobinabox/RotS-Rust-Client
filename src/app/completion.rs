use super::App;

impl App {
    /// Snapshot live choices only when completion is requested, including changes since the last Tab.
    pub(super) fn refresh_command_completions(&mut self) {
        let draft = if self.state.submitted_input_selected {
            self.state.submitted_input.as_deref().unwrap_or_default()
        } else {
            self.state
                .input
                .get(..self.state.cursor)
                .unwrap_or_default()
        };
        let context_start =
            crate::completion::input_context(draft, draft.len()).map_or(0, |context| context.start);
        let line = draft[context_start..].trim_start();
        let is_slash_command = line.starts_with('/');
        let is_map_command = line.split_whitespace().next() == Some("/map");
        let choices = &mut self.state.command_completions.0;
        choices.clear();
        if !is_slash_command {
            return;
        }
        choices.insert(
            "/theme use".into(),
            self.config
                .themes
                .keys()
                .map(|name| crate::ui::theme::normalize_theme_name(name))
                .filter(|name| !name.chars().any(char::is_whitespace))
                .collect(),
        );
        choices.insert(
            "/alias unset".into(),
            self.aliases
                .runtime_configs()
                .into_iter()
                .filter_map(|r| argument(&r.pattern))
                .collect(),
        );
        let triggers: Vec<_> = self
            .triggers
            .runtime_configs()
            .into_iter()
            .filter_map(|r| argument(&r.pattern))
            .collect();
        choices.insert("/trigger unset".into(), triggers.clone());
        choices.insert("/triggers unset".into(), triggers);
        choices.insert(
            "/highlight unset".into(),
            self.highlights
                .runtime_configs()
                .into_iter()
                .filter_map(|r| argument(&r.pattern))
                .collect(),
        );
        choices.insert(
            "/substitute unset".into(),
            self.substitutions
                .runtime_configs()
                .into_iter()
                .filter_map(|r| argument(&r.pattern))
                .collect(),
        );
        choices.insert(
            "/handler unset".into(),
            self.events
                .runtime_configs()
                .into_iter()
                .filter_map(|r| argument(&r.event))
                .collect(),
        );
        choices.insert(
            "/variable unset".into(),
            self.variables.runtime_values().into_keys().collect(),
        );
        choices.insert(
            "/timer cancel".into(),
            self.lua_timers
                .keys()
                .filter(|name| !name.chars().any(char::is_whitespace))
                .cloned()
                .collect(),
        );
        choices.insert(
            "/macro remove".into(),
            self.macros
                .runtime_configs()
                .into_iter()
                .filter_map(|r| argument(&r.key))
                .collect(),
        );
        if !is_map_command {
            return;
        }
        let destinations: Vec<_> = self
            .state
            .map
            .rooms
            .keys()
            .chain(self.state.map.landmarks.keys())
            .filter(|s| !s.chars().any(char::is_whitespace))
            .cloned()
            .collect();
        for command in ["/map goto", "/map find", "/map run"] {
            choices.insert(command.into(), destinations.clone());
        }
        choices.insert(
            "/map unlandmark".into(),
            self.state
                .map
                .landmarks
                .keys()
                .filter(|s| !s.chars().any(char::is_whitespace) && !s.contains(['*', '?']))
                .cloned()
                .collect(),
        );
    }
}

// Definition commands accept balanced braced fields; avoid offering unsafe delimiters.
fn argument(value: &str) -> Option<String> {
    if value.contains(['{', '}', '\n', '\r']) {
        None
    } else {
        Some(format!("{{{}}}", value.replace('\\', "\\\\")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TerminalEvent as Event;
    use crate::{commands::ClientCommand, config::AppConfig};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tokio::sync::mpsc;

    async fn complete_marked_fields(app: &mut App, draft: &str, tx: &mpsc::Sender<ClientCommand>) {
        app.state.input = draft.into();
        while let Some(cursor) = app.state.input.find('|') {
            app.state.clear_input_completion();
            app.state.input.remove(cursor);
            app.state.cursor = cursor;
            app.handle_terminal_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                tx,
            )
            .await;
            assert!(app.state.input_completion.active, "{}", app.state.input);
        }
    }

    #[tokio::test]
    async fn completed_trigger_updates_map_defines_alias_and_sends_only_when_fired() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.state.map.execute("create").unwrap();
        app.push_mud_output_line("renew sanctuary", &tx).await;
        let original_room = app.state.map.rooms["1"].clone();

        complete_marked_fields(&mut app,
            "/trigger plain {Arrived} {/map set roomterrain Fo|} {/map set roomnote san|} {/alias rr {cast re|}} {say re|}", &tx).await;
        assert_eq!(
            app.state.input,
            "/trigger plain {Arrived} {/map set roomterrain Forest} {/map set roomnote sanctuary} {/alias rr {cast renew}} {say renew}"
        );
        assert_eq!(app.state.map.rooms["1"], original_room);
        assert!(app.aliases.runtime_configs().is_empty());
        assert!(app.triggers.runtime_configs().is_empty());
        assert!(rx.try_recv().is_err());

        app.handle_terminal_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert_eq!(app.triggers.runtime_configs().len(), 1);
        assert!(app.aliases.runtime_configs().is_empty());
        assert_eq!(app.state.map.rooms["1"], original_room);
        assert!(rx.try_recv().is_err());
        app.handle_network_event(
            crate::events::NetworkEvent::Text("Nothing happens\n".into()),
            &tx,
        )
        .await;
        assert_eq!(app.state.map.rooms["1"], original_room);
        assert!(rx.try_recv().is_err());

        app.handle_network_event(crate::events::NetworkEvent::Text("Arrived\n".into()), &tx)
            .await;
        assert_eq!(app.state.map.rooms["1"].terrain, "Forest");
        assert_eq!(app.state.map.rooms["1"].note, "sanctuary");
        let aliases = app.aliases.runtime_configs();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].pattern, "rr");
        assert_eq!(aliases[0].commands, ["cast renew"]);
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("say renew".into())
        );
        assert!(rx.try_recv().is_err());
        app.handle_command(ClientCommand::SendText("rr".into()), &tx)
            .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("cast renew".into())
        );
        assert!(rx.try_recv().is_err());
        assert!(
            !app.state
                .output
                .iter()
                .any(|line| line.category == crate::state::OutputCategory::Error)
        );
    }

    #[tokio::test]
    async fn completed_alias_mixes_local_map_changes_and_mud_actions_on_invocation() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(8);
        app.state.map.execute("create").unwrap();
        app.push_mud_output_line("renew sanctuary", &tx).await;
        complete_marked_fields(
            &mut app,
            "/alias visit {/map set roomnote san|} {say re|}",
            &tx,
        )
        .await;
        assert_eq!(
            app.state.input,
            "/alias visit {/map set roomnote sanctuary} {say renew}"
        );
        app.handle_terminal_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert!(app.state.map.rooms["1"].note.is_empty());
        assert!(rx.try_recv().is_err());
        app.handle_command(ClientCommand::SendText("visit".into()), &tx)
            .await;
        assert_eq!(app.state.map.rooms["1"].note, "sanctuary");
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("say renew".into())
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn completed_handler_runs_map_and_mud_actions_only_after_its_event() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(8);
        app.state.map.execute("create").unwrap();
        app.push_mud_output_line("renew sanctuary", &tx).await;
        complete_marked_fields(
            &mut app,
            "/handler {CampReady} {/map set roomnote san|} {cast re|}",
            &tx,
        )
        .await;
        assert_eq!(
            app.state.input,
            "/handler {CampReady} {/map set roomnote sanctuary} {cast renew}"
        );
        app.handle_terminal_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert_eq!(app.events.runtime_configs().len(), 1);
        assert!(app.state.map.rooms["1"].note.is_empty());
        assert!(rx.try_recv().is_err());
        app.handle_command(ClientCommand::SendText("/event {CampReady}".into()), &tx)
            .await;
        assert_eq!(app.state.map.rooms["1"].note, "sanctuary");
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("cast renew".into())
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn nested_completion_refreshes_dynamic_context_without_execution() {
        let mut app = App::new(AppConfig::default());
        app.state.map.execute("create").unwrap();
        app.state.map.execute("landmark Home 1").unwrap();
        app.handle_alias_command("hello look").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        for (draft, expected) in [
            ("/alias rr {/map goto Ho}", "/alias rr {/map goto Home}"),
            (
                "/alias rr {/alias unset {hel}}",
                "/alias rr {/alias unset {hello}}",
            ),
        ] {
            app.state.clear_input_completion();
            app.state.input = draft.into();
            app.state.cursor = draft.find('}').unwrap();
            app.handle_terminal_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &tx,
            )
            .await;
            assert_eq!(app.state.input, expected);
        }
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn destructive_completions_exclude_wildcards_and_unusable_timer_names() {
        let mut app = App::new(AppConfig::default());
        app.state.map.execute("create").unwrap();
        for name in ["camp*", "camp?", "camp-east"] {
            app.state
                .map
                .execute(&format!("landmark {name} 1"))
                .unwrap();
        }
        app.state.input = "/map unlandmark camp".into();
        app.state.cursor = app.state.input.len();
        app.refresh_command_completions();
        assert_eq!(
            app.state
                .command_completions
                .matches("/map unlandmark ", "camp"),
            ["camp-east"]
        );
        for name in ["heal pulse", "heal"] {
            app.lua_timers.insert(
                name.into(),
                super::super::LuaTimer {
                    interval: std::time::Duration::from_secs(1),
                    next_due: std::time::Instant::now(),
                    callback: "callback".into(),
                    repeat: false,
                    tick_count: 0,
                },
            );
        }
        app.state.input = "/timer cancel heal".into();
        app.state.cursor = app.state.input.len();
        app.refresh_command_completions();
        assert_eq!(
            app.state
                .command_completions
                .matches("/timer cancel ", "heal"),
            ["heal"]
        );
    }

    #[tokio::test]
    async fn removal_completions_round_trip_through_command_parsers() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(8);
        for (definition, draft, context) in [
            (
                "/trigger plain {Danger} {look}",
                "/trigger unset D",
                "/trigger unset",
            ),
            (
                "/highlight {Danger} {red}",
                "/highlight unset D",
                "/highlight unset",
            ),
            (
                "/substitute plain {Danger} {warning}",
                "/substitute unset D",
                "/substitute unset",
            ),
            (
                "/handler {Danger} {look}",
                "/handler unset D",
                "/handler unset",
            ),
            ("/macro {F5} {look}", "/macro remove F", "/macro remove"),
            (
                "/alias {hello world} {look}",
                "/alias unset {hello w",
                "/alias unset",
            ),
        ] {
            app.handle_command(ClientCommand::SendText(definition.into()), &tx)
                .await;
            app.state.clear_input_completion();
            app.state.input = draft.into();
            app.state.cursor = draft.len();
            app.refresh_command_completions();
            assert_eq!(
                app.state.command_completions.0[context].len(),
                1,
                "{definition}"
            );
            app.state.complete_input(false);
            assert!(app.state.input_completion.active, "{draft}");
            let completed = app.state.input.clone();
            app.handle_command(ClientCommand::SendText(completed), &tx)
                .await;
            app.refresh_command_completions();
            assert!(
                app.state.command_completions.0[context].is_empty(),
                "{draft}"
            );
        }
        assert!(rx.try_recv().is_err());
        let value = "regex\\word";
        assert_eq!(
            super::super::parse_braced_fields(&argument(value).unwrap(), "test").unwrap(),
            [value]
        );
    }

    #[test]
    fn custom_theme_completion_normalizes_names_and_rejects_newlines() {
        let mut config = AppConfig::default();
        config
            .themes
            .insert("Home Theme".into(), config.colors.clone());
        config
            .themes
            .insert("night\nsay hello".into(), config.colors.clone());
        let mut app = App::new(config);
        app.state.input = "/theme use ".into();
        app.state.cursor = app.state.input.len();
        app.refresh_command_completions();
        assert_eq!(
            app.state.command_completions.matches("/theme use ", "home"),
            ["home-theme"]
        );
        assert!(
            app.state
                .command_completions
                .matches("/theme use ", "night")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn completion_refreshes_live_choices_without_sending_commands() {
        let mut config = AppConfig::default();
        config.themes.insert("home".into(), config.colors.clone());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(8);
        app.handle_command(ClientCommand::SendText("/alias hello look".into()), &tx)
            .await;
        for (input, expected) in [
            ("/theme use h", vec!["haradrim", "hobbit", "home", "human"]),
            ("/alias unset h", vec!["{hello}"]),
        ] {
            app.state.clear_input_completion();
            app.state.input = input.into();
            app.state.cursor = input.len();
            app.handle_terminal_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &tx,
            )
            .await;
            assert_eq!(app.state.input_completion.matches, expected);
        }
        app.handle_command(ClientCommand::SendText("/alias unset hello".into()), &tx)
            .await;
        app.state.input = "/alias unset h".into();
        app.state.cursor = app.state.input.len();
        app.handle_terminal_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert!(!app.state.input_completion.active);
        assert!(rx.try_recv().is_err());
    }
}
