use super::*;

/// A display decision belongs to one server line, never to the next queued echo.
pub(super) struct IncomingLine {
    id: u64,
    prefix: String,
    pub replacement: Option<String>,
    pub gag: bool,
}

impl App {
    pub(super) fn begin_line_edit(
        &mut self,
        raw: &str,
        category: OutputCategory,
        style: Option<crate::state::OutputStyle>,
    ) {
        self.incoming_sequence = self.incoming_sequence.wrapping_add(1);
        let id = self.incoming_sequence;
        self.state.push_incoming_line(id, raw, category, style);
        self.state.protected_output_id = Some(id);
        let prefix = self.source_style.prefix();
        self.source_style.consume(raw);
        self.incoming_line = Some(IncomingLine {
            id,
            prefix,
            replacement: None,
            gag: false,
        });
    }

    pub(super) fn finish_line_edit(
        &mut self,
        raw: &str,
        category: OutputCategory,
        colors: &AnsiColors,
    ) {
        let Some(line) = self.incoming_line.take() else {
            return;
        };
        self.state.protected_output_id = None;
        if line.gag {
            self.state.edit_incoming_line(line.id, None, line.prefix);
            return;
        }
        let candidate = format!(
            "{}{}",
            line.prefix,
            line.replacement.as_deref().unwrap_or(raw)
        );
        let display = match self
            .substitutions
            .apply(&candidate, category, colors, &self.variables)
        {
            Ok(display) if display == candidate => {
                line.replacement.unwrap_or_else(|| raw.to_owned())
            }
            Ok(display) => display,
            Err(error) => {
                self.state.push_output(
                    format!("Substitution failed: {error}"),
                    OutputCategory::Error,
                );
                raw.to_owned()
            }
        };
        self.state
            .edit_incoming_line(line.id, Some(display), line.prefix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MatchType, TriggerRuleConfig};

    fn app(hooks: &[&str]) -> App {
        let mut config = AppConfig::default();
        config.lua.script_dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
        config.lua.scripts = Some(vec!["output_edits.lua".into()]);
        for (index, hook) in hooks.iter().enumerate() {
            config.triggers.rules.push(TriggerRuleConfig {
                name: format!("hook{index}"),
                match_type: MatchType::Plain,
                pattern: "orc".into(),
                lua: Some((*hook).into()),
                ..Default::default()
            });
        }
        App::new(config)
    }

    #[tokio::test]
    async fn replace_preserves_original_and_notifications_and_does_not_retrigger() {
        let mut app = app(&["replace_line", "remember_original"]);
        let (tx, mut rx) = mpsc::channel(4);
        app.push_mud_output_line("An orc stands here.", &tx).await;
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.variables.expand("${observed}").unwrap(),
            "An orc stands here."
        );
        let original = app
            .state
            .output
            .iter()
            .find(|line| line.source_id.is_some())
            .unwrap();
        assert_eq!(original.raw, "An orc stands here.");
        assert_eq!(plain_text(&original.normalized), "(1) An orc stands here.");
        assert_eq!(
            app.state.output.back().unwrap().normalized,
            "after replacement"
        );
    }

    #[tokio::test]
    async fn gag_keeps_echo_and_source_color_for_next_line() {
        let mut app = app(&["gag_line", "remember_original"]);
        let (tx, _) = mpsc::channel(4);
        app.push_mud_output_line("\x1b[31morc", &tx).await;
        assert_eq!(app.state.output.len(), 1);
        assert_eq!(app.state.output[0].normalized, "numbered echo");
        assert_eq!(app.variables.expand("${observed}").unwrap(), "orc");
        app.push_mud_output_line("next server line", &tx).await;
        assert!(
            app.state
                .output
                .back()
                .unwrap()
                .source_prefix
                .as_ref()
                .unwrap()
                .contains("31")
        );
        assert_eq!(
            app.state.output.back().unwrap().normalized,
            "next server line"
        );
    }

    #[tokio::test]
    async fn failed_hook_retains_original_and_gag_wins_over_replace() {
        let (tx, _) = mpsc::channel(4);
        let mut failed = app(&["failed_edit"]);
        failed.push_mud_output_line("orc", &tx).await;
        assert!(
            failed
                .state
                .output
                .iter()
                .any(|line| line.normalized == "orc")
        );
        let mut hidden = app(&["replace_then_gag"]);
        hidden.push_mud_output_line("orc", &tx).await;
        assert!(hidden.state.output.is_empty());
    }

    #[tokio::test]
    async fn clear_or_eviction_cannot_redirect_a_replacement_to_an_echo() {
        let (tx, _) = mpsc::channel(4);
        let mut app = app(&["clear_then_replace"]);
        app.state.set_scrollback_limit(1);
        app.push_mud_output_line("orc", &tx).await;
        assert!(
            !app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("must not replace"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "surviving echo")
        );
    }

    #[tokio::test]
    async fn shipped_targeting_hooks_number_rots_colors_and_send_selected_mob() {
        let mut config: AppConfig = toml::from_str(include_str!("../../config.toml")).unwrap();
        config.lua.script_dir = format!("{}/scripts", env!("CARGO_MANIFEST_DIR"));
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(8);
        app.push_mud_output_line("\x1b[01m\x1b[33mA Forest\x1b[0m", &tx)
            .await;
        app.push_mud_output_line("\x1b[01m\x1b[36mA hungry wolf stands here.\x1b[0m", &tx)
            .await;
        app.push_mud_output_line("\x1b[01m\x1b[36mA grey wolf stands here.\x1b[0m", &tx)
            .await;
        assert_eq!(
            plain_text(&app.state.output.back().unwrap().normalized),
            "(2) A grey wolf stands here."
        );
        app.handle_command(ClientCommand::SendText("k2".into()), &tx)
            .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("kill 2.wolf".into())
        );
        app.handle_alias_command("{p} {cast 'poison' {1}}").unwrap();
        app.handle_command(ClientCommand::SendText("p2".into()), &tx)
            .await;
        assert_eq!(
            rx.try_recv().unwrap(),
            ClientCommand::SendText("cast 'poison' 2.wolf".into())
        );
    }

    #[tokio::test]
    async fn execute_recursion_has_a_root_budget_for_typed_aliases() {
        let mut app = app(&[]);
        app.aliases
            .add_runtime_config(crate::config::AliasRuleConfig {
                name: "recursive".into(),
                pattern: "recursive".into(),
                lua: Some("recurse_execute".into()),
                ..Default::default()
            })
            .unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        app.handle_command(ClientCommand::SendText("recursive".into()), &tx)
            .await;
        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("exceeded maximum command count"))
        );
    }

    #[tokio::test]
    async fn original_remains_in_lua_snapshot_after_substitution() {
        let mut app = app(&[]);
        app.handle_substitution_command("plain {orc} {goblin}")
            .unwrap();
        let (tx, _) = mpsc::channel(4);
        app.push_mud_output_line("orc", &tx).await;
        app.run_lua_hook(
            "snapshot_original",
            LuaHookContext::default(),
            &tx,
            &mut Vec::new(),
        )
        .await;
        assert_eq!(app.variables.expand("${snapshot}").unwrap(), "orc");
        assert_eq!(plain_text(&app.state.output[0].normalized), "goblin");
    }

    #[tokio::test]
    async fn substitutions_follow_lua_and_preserve_prompts() {
        let (tx, _) = mpsc::channel(4);
        let mut app = app(&["replace_line"]);
        app.handle_substitution_command("plain {(1)} {[target]}")
            .unwrap();
        app.handle_network_event(NetworkEvent::Prompt("orc>".into()), &tx)
            .await;
        let prompt = app
            .state
            .output
            .iter()
            .find(|line| line.category == OutputCategory::Prompt)
            .unwrap();
        assert_eq!(plain_text(&prompt.normalized), "[target] orc>");
        assert_eq!(prompt.raw, "orc>");
    }
}
