use super::{App, parse_braced_fields, split_first_token, strip_subcommand};
use crate::macros::MacroRule;

impl App {
    pub(super) fn handle_macro_command(&mut self, input: &str) -> Result<String, String> {
        let mut input = input.trim();
        if input.is_empty() {
            return Ok(self.macros.listing());
        }
        if input == "clear" {
            return Ok(format!("Removed {} runtime macros.", self.macros.clear()));
        }
        if let Some(mode) = strip_subcommand(input, "mode") {
            self.macros.printable_mode = match mode {
                "on" => true,
                "off" => false,
                _ => return Err("usage: /macro mode on|off".into()),
            };
            return Ok(format!(
                "Printable-key macro mode {mode}. Ctrl+C leaves this mode."
            ));
        }
        if let Some(rest) = strip_subcommand(input, "remove") {
            let fields = parse_braced_fields(rest, "macro remove")?;
            if fields.len() != 1 {
                return Err("usage: /macro remove {key}".into());
            }
            return if self.macros.remove(&fields[0])? {
                Ok(format!(
                    "Removed runtime macro `{}`; any configured binding is restored.",
                    fields[0]
                ))
            } else {
                Err(format!(
                    "No runtime macro for `{}`. Edit config.toml to remove configured bindings.",
                    fields[0]
                ))
            };
        }
        let mut rule = MacroRule::default();
        while input.starts_with("--") {
            let Some((flag, rest)) = split_first_token(input) else {
                break;
            };
            match flag {
                "--override" => rule.override_builtin = true,
                "--repeat" => rule.allow_repeat = true,
                _ => return Err(format!("Unknown macro option `{flag}`")),
            }
            input = rest.trim_start();
        }
        let fields = parse_braced_fields(input, "macro")?;
        if fields.len() != 2 {
            return Err("usage: /macro [--override] [--repeat] {key} {command}".into());
        }
        rule.key = fields[0].clone();
        rule.command = fields[1].clone();
        self.macros.add(rule)?;
        Ok(format!(
            "Macro `{}` added for this session. Printable keys require /macro mode on.",
            fields[0]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::ClientCommand,
        config::{AppConfig, ConfigLoadOptions},
        events::TerminalEvent,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn numpad_macro_preserves_top_row_digits_and_main_enter() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.handle_macro_command("{Numpad8} {north}").unwrap();
        app.handle_macro_command("{NumpadEnter} {look}").unwrap();
        app.state.input = "say ".into();
        app.state.cursor = 4;
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert_eq!(app.state.input, "say 8");
        for (code, command) in [
            (KeyCode::Char('8'), "north"),
            (KeyCode::Up, "north"),
            (KeyCode::Enter, "look"),
        ] {
            let mut key = KeyEvent::new(code, KeyModifiers::NONE);
            key.state = crossterm::event::KeyEventState::KEYPAD;
            app.handle_terminal_event(TerminalEvent::Key(key), &tx)
                .await;
            assert_eq!(
                rx.try_recv().unwrap(),
                ClientCommand::SendText(command.into())
            );
            assert_eq!(app.state.input, "say 8");
        }
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("say 8".into())
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn macro_uses_alias_variables_local_and_multi_command_pipeline_without_editing_draft() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        for text in [
            "/variable {target} {bear}",
            "/alias {attack} {kill ${target}}",
            "/macro {F5} {attack;get bread bag;eat bread;/echo ${target}}",
            "/variable {target} {wolf}",
        ] {
            app.handle_command(ClientCommand::SendText(text.into()), &tx)
                .await;
        }
        app.state.input = "say unfinished".into();
        app.state.cursor = 3;
        app.state.command_history.push("score".into());
        let history = app.state.command_history.clone();
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
            &tx,
        )
        .await;
        for expected in ["kill wolf", "get bread bag", "eat bread"] {
            assert_eq!(
                rx.try_recv().unwrap(),
                ClientCommand::SendText(expected.into())
            );
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(app.state.input, "say unfinished");
        assert_eq!(app.state.cursor, 3);
        assert_eq!(app.state.command_history, history);
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "wolf")
        );
    }

    #[tokio::test]
    async fn macro_mode_search_and_ctrl_c_keep_editing_available() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.handle_macro_command("{n} {north}").unwrap();
        let n = TerminalEvent::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_terminal_event(n.clone(), &tx).await;
        assert_eq!(app.state.input, "n");
        assert!(rx.try_recv().is_err());
        app.handle_macro_command("mode on").unwrap();
        app.handle_terminal_event(n.clone(), &tx).await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("north".into())
        );
        assert_eq!(app.state.input, "n");
        app.state.start_output_search();
        app.handle_terminal_event(n, &tx).await;
        assert_eq!(app.state.output_view.search_input, "n");
        assert!(rx.try_recv().is_err());
        app.state.cancel_output_search();
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            &tx,
        )
        .await;
        assert!(!app.macros.printable_mode);
        assert!(app.state.input.is_empty());
    }

    #[tokio::test]
    async fn macro_repeat_is_consumed_and_release_does_not_submit_input() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.handle_macro_command("--override {Enter} {look}")
            .unwrap();
        app.state.input = "draft".into();
        app.state.cursor = 5;
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            app.handle_terminal_event(
                TerminalEvent::Key(KeyEvent::new_with_kind(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                    kind,
                )),
                &tx,
            )
            .await;
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(app.state.input, "draft");
        app.handle_macro_command("--override --repeat {Enter} {look}")
            .unwrap();
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            )),
            &tx,
        )
        .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("look".into())
        );
    }

    #[tokio::test]
    async fn macro_missing_variables_and_invalid_definitions_do_not_send_commands() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.handle_command(
            ClientCommand::SendText("/macro {F5} {kill ${missing}}".into()),
            &tx,
        )
        .await;
        assert!(app.macros.listing().contains("${missing}"));
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
            &tx,
        )
        .await;
        assert!(rx.try_recv().is_err());
        for input in [
            "{F2} {look}",
            "--override {Ctrl+C} {look}",
            "{F5}",
            "{F5} {}",
            "mode maybe",
            "--unknown {F5} {look}",
            "remove {F5} {extra}",
        ] {
            assert!(app.handle_macro_command(input).is_err(), "{input}");
        }
        assert!(app.macros.listing().contains("${missing}"));
    }

    #[test]
    fn macro_reload_preserves_runtime_bindings_and_rejects_invalid_config() {
        let path =
            std::env::temp_dir().join(format!("mud-client-macros-{}.toml", std::process::id()));
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        app.handle_macro_command("{F5} {runtime}").unwrap();
        app.handle_macro_command("mode on").unwrap();
        std::fs::write(
            &path,
            "[[macros.rules]]\nkey = 'F5'\ncommand = 'configured'\n",
        )
        .unwrap();
        app.reload_config().unwrap();
        assert!(app.macros.printable_mode);
        assert!(app.macros.listing().contains("shadowed"));
        app.handle_macro_command("remove {f5}").unwrap();
        assert!(app.macros.listing().contains("configured"));
        std::fs::write(&path, "[[macros.rules]]\nkey = 'Ctrl+C'\ncommand = 'bad'\n").unwrap();
        assert!(app.reload_config().is_err());
        assert!(app.macros.listing().contains("configured"));
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn macro_movement_repeat_help_and_quit_use_existing_command_behavior() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(16);
        app.state.map.create();
        app.state.map.execute("dig w").unwrap();
        app.state.map.execute("door w closed stone door").unwrap();
        app.handle_macro_command("{F5} {west;#2 {look}}").unwrap();
        app.handle_terminal_event(
            TerminalEvent::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
            &tx,
        )
        .await;
        for text in ["open stone door w", "w", "look", "look"] {
            assert_eq!(rx.try_recv().unwrap(), ClientCommand::SendText(text.into()));
        }
        app.handle_command(ClientCommand::SendText("/help macro".into()), &tx)
            .await;
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "# Macro Help")
        );
        app.handle_macro_command("{F5} {/quit}").unwrap();
        assert!(
            app.handle_terminal_event(
                TerminalEvent::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
                &tx
            )
            .await
        );
        assert!(rx.try_recv().is_err());
    }
}
