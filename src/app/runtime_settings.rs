use super::App;
use crate::{persistence::RuntimeSettings, state::OutputCategory};

impl App {
    fn runtime_settings(&self) -> RuntimeSettings {
        RuntimeSettings {
            variables: self.variables.runtime_values(),
            macros: self.macros.runtime_configs(),
            aliases: self.aliases.runtime_configs(),
            triggers: self.triggers.runtime_configs(),
            highlights: self.highlights.runtime_configs(),
            ..RuntimeSettings::default()
        }
    }

    pub(super) fn start_runtime_save(&mut self) -> Result<String, String> {
        if self.runtime_save.is_some() {
            return Err(
                "A runtime save is already pending; wait for its completion before saving again."
                    .into(),
            );
        }
        let path = self
            .runtime_store
            .path()
            .ok_or("No active configuration path; runtime settings cannot be saved.")?
            .to_owned();
        let snapshot = self.runtime_settings();
        let count = snapshot.count();
        let config = self.config.clone();
        let mut store = self.runtime_store.clone();
        self.runtime_save = Some(tokio::task::spawn_blocking(move || {
            snapshot.compile(&config)?;
            store.save(&snapshot)?;
            Ok(store)
        }));
        Ok(format!(
            "Saving {count} runtime rules and variables to {}. Wait for Saved runtime settings before exiting; later edits need another /save.",
            path.display()
        ))
    }

    /// Await only completed tasks during normal ticks; graceful exit also waits
    /// so an explicitly requested save is not silently abandoned.
    pub(super) async fn finish_runtime_save(&mut self) -> Option<String> {
        let task = self.runtime_save.take()?;
        let result = match task.await {
            Ok(Ok(store)) => {
                self.runtime_store = store;
                self.state.push_output(
                    "Saved runtime settings. config.toml was not changed.",
                    OutputCategory::System,
                );
                return None;
            }
            Ok(Err(error)) => error.to_string(),
            Err(_) => {
                "background save worker failed; check the saved file before retrying".to_owned()
            }
        };
        let message = format!("Runtime save failed: {result}");
        self.state.push_output(&message, OutputCategory::Error);
        Some(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::ClientCommand,
        config::{AppConfig, ConfigLoadOptions},
        macros::MacroRule,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    use tokio::sync::mpsc;

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        config: AppConfig,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "mud-runtime-app-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let mut config = AppConfig::default();
            config.map.persistence.load_on_startup = false;
            config.map.persistence.save_on_exit = false;
            config.macros.rules.push(MacroRule {
                key: "F5".into(),
                command: "score".into(),
                ..MacroRule::default()
            });
            fs::write(root.join("config.toml"), toml::to_string(&config).unwrap()).unwrap();
            Self { root, config }
        }
        fn app(&self) -> App {
            App::new_with_config_source(
                self.config.clone(),
                Some(self.root.join("config.toml")),
                ConfigLoadOptions::default(),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn character_app(fixture: &Fixture, name: &str) -> App {
        let base = fixture.root.join("config.toml");
        let profile = crate::profiles::character_path(&base, name).unwrap();
        let config = AppConfig::load_with_character(
            Some(base.clone()),
            Some(&profile),
            ConfigLoadOptions::default(),
        )
        .unwrap();
        App::new_with_character_source(
            config,
            Some(base),
            ConfigLoadOptions::default(),
            Some(profile),
        )
    }

    fn profiles(fixture: &Fixture) {
        fs::create_dir(fixture.root.join("characters")).unwrap();
        for name in ["aragorn", "gimli"] {
            fs::write(
                fixture.root.join(format!("characters/{name}.toml")),
                format!("[panels.output]\ntitle='{name}'"),
            )
            .unwrap();
        }
    }

    async fn report_character(app: &mut App, name: &str) {
        let (tx, _rx) = mpsc::channel(8);
        app.handle_network_event(
            crate::events::NetworkEvent::Msdp(vec![
                crate::network::msdp::MsdpFrame {
                    variable: "CHARACTER_NAME".into(),
                    value: crate::network::msdp::MsdpValue::String(name.into()),
                },
                crate::network::msdp::MsdpFrame {
                    variable: "HEALTH".into(),
                    value: crate::network::msdp::MsdpValue::String("42".into()),
                },
            ]),
            &tx,
        )
        .await;
    }

    #[tokio::test]
    async fn msdp_switches_profiles_and_preserves_unsaved_edits_without_leaking() {
        let fixture = Fixture::new();
        profiles(&fixture);
        let mut app = fixture.app();
        app.handle_macro_command("{F6} {shared}").unwrap();
        report_character(&mut app, "Aragorn").await;
        assert_eq!(app.config.panels.output.title, "aragorn");
        assert_eq!(app.state.character.health, Some(42));
        assert!(app.runtime_settings().macros.is_empty());
        app.handle_macro_command("{F7} {aragorn}").unwrap();
        report_character(&mut app, "Gimli").await;
        assert_eq!(app.config.panels.output.title, "gimli");
        assert!(app.runtime_settings().macros.is_empty());
        app.handle_macro_command("{F8} {gimli}").unwrap();
        report_character(&mut app, "Aragorn").await;
        assert_eq!(app.runtime_settings().macros[0].command, "aragorn");
        let output_count = app.state.output.len();
        report_character(&mut app, "ARAGORN").await;
        assert_eq!(app.state.output.len(), output_count);
        assert_eq!(app.runtime_settings().macros[0].command, "aragorn");
        report_character(&mut app, "NoProfile").await;
        assert!(app.character_path.is_none());
        assert_eq!(app.runtime_settings().macros[0].command, "shared");
        assert_eq!(
            app.state.output.len(),
            output_count,
            "missing profile must be silent"
        );
    }

    #[tokio::test]
    async fn pending_save_finishes_in_outgoing_namespace_before_msdp_switch() {
        let fixture = Fixture::new();
        profiles(&fixture);
        let mut app = fixture.app();
        report_character(&mut app, "Aragorn").await;
        app.handle_macro_command("{F7} {aragorn}").unwrap();
        app.start_runtime_save().unwrap();
        report_character(&mut app, "Gimli").await;
        assert!(app.runtime_save.is_none());
        app.handle_macro_command("{F7} {gimli}").unwrap();
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        assert_eq!(
            character_app(&fixture, "aragorn").runtime_settings().macros[0].command,
            "aragorn"
        );
        assert_eq!(
            character_app(&fixture, "gimli").runtime_settings().macros[0].command,
            "gimli"
        );
    }

    #[tokio::test]
    async fn invalid_profile_falls_back_without_losing_previous_character_rules() {
        let fixture = Fixture::new();
        profiles(&fixture);
        let mut app = character_app(&fixture, "aragorn");
        app.handle_macro_command("{F7} {keep}").unwrap();
        fs::write(fixture.root.join("characters/gimli.toml"), "[bad").unwrap();
        report_character(&mut app, "Gimli").await;
        assert!(app.character_path.is_none());
        assert!(app.runtime_settings().macros.is_empty());
        report_character(&mut app, "Aragorn").await;
        assert_eq!(app.runtime_settings().macros[0].command, "keep");
    }

    #[tokio::test]
    async fn unsafe_and_absent_names_cannot_load_files_or_leak_profile_rules() {
        let fixture = Fixture::new();
        profiles(&fixture);
        let mut app = fixture.app();
        let initial = app.state.output.len();
        report_character(&mut app, "Unknown").await;
        assert_eq!(app.state.output.len(), initial);
        report_character(&mut app, "Aragorn").await;
        app.handle_macro_command("{F7} {private}").unwrap();
        let before = app.state.output.len();
        report_character(&mut app, "../outside").await;
        assert!(app.character_path.is_none());
        assert!(app.runtime_settings().macros.is_empty());
        assert_eq!(app.state.output.len(), before);
    }

    #[tokio::test]
    async fn unprofiled_character_change_resets_stale_game_state_but_duplicates_do_not() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        report_character(&mut app, "First").await;
        app.state.character.health_max = Some(1000);
        app.state.opponent.name = Some("old opponent".into());
        report_character(&mut app, "FIRST").await;
        assert_eq!(app.state.character.health_max, Some(1000));
        report_character(&mut app, "Second").await;
        assert_eq!(app.state.character.health_max, None);
        assert_eq!(app.state.opponent.name, None);
        assert_eq!(app.state.character.health, Some(42));
    }

    #[tokio::test]
    async fn reload_discovers_created_profile_and_silently_handles_deleted_profile() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        report_character(&mut app, "Aragorn").await;
        profiles(&fixture);
        let (tx, _rx) = mpsc::channel(8);
        app.handle_local_command("/reload", &tx, &mut Vec::new())
            .await;
        assert_eq!(app.config.panels.output.title, "aragorn");
        assert_eq!(app.state.character.health, Some(42));
        assert!(app.state.raw_msdp.contains_key("CHARACTER_NAME"));
        fs::remove_file(fixture.root.join("characters/aragorn.toml")).unwrap();
        app.handle_local_command("/reload", &tx, &mut Vec::new())
            .await;
        assert!(app.character_path.is_none());
        assert!(
            app.state
                .output
                .iter()
                .all(|line| line.category != OutputCategory::Error)
        );
    }

    #[tokio::test]
    async fn msdp_switch_cancels_timers_and_path_recording() {
        let fixture = Fixture::new();
        profiles(&fixture);
        let mut app = fixture.app();
        report_character(&mut app, "Aragorn").await;
        app.state.path.mapping = true;
        app.lua_timers.insert(
            "old".into(),
            super::super::LuaTimer {
                interval: std::time::Duration::from_secs(1),
                next_due: std::time::Instant::now(),
                callback: "old_character".into(),
                repeat: true,
                tick_count: 0,
            },
        );
        report_character(&mut app, "Gimli").await;
        assert!(app.lua_timers.is_empty());
        assert!(!app.state.path.mapping);
    }

    #[tokio::test]
    async fn incoming_profile_lua_initialization_sees_new_msdp_identity() {
        let fixture = Fixture::new();
        profiles(&fixture);
        fs::create_dir(fixture.root.join("incoming_scripts")).unwrap();
        fs::write(fixture.root.join("incoming_scripts/init.lua"), "local who = client.msdp.get('CHARACTER_NAME')\nlocal hp = client.character.get().health\nfunction captured() client.send(who .. ':' .. hp) end").unwrap();
        fs::write(
            fixture.root.join("characters/gimli.toml"),
            "[lua]\nenabled=true\nscript_dir='incoming_scripts'\nentrypoint='init.lua'",
        )
        .unwrap();
        let mut app = fixture.app();
        report_character(&mut app, "Aragorn").await;
        app.state.character.health = Some(999);
        report_character(&mut app, "Gimli").await;
        let result = app
            .lua
            .call_hook(
                "captured",
                crate::scripting::lua::LuaHookContext::default(),
                &app.state,
                &app.variables,
            )
            .unwrap();
        assert_eq!(
            result.actions,
            vec![crate::scripting::lua::LuaAction::Send("Gimli:42".into())]
        );
    }

    #[tokio::test]
    async fn character_snapshots_are_isolated_from_each_other_and_unprofiled_saves() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.root.join("characters")).unwrap();
        for name in ["aragorn", "gimli"] {
            fs::write(fixture.root.join(format!("characters/{name}.toml")), "").unwrap();
        }
        let mut shared = fixture.app();
        shared.handle_macro_command("{F6} {shared}").unwrap();
        shared.start_runtime_save().unwrap();
        assert!(shared.finish_runtime_save().await.is_none());
        let mut aragorn = character_app(&fixture, "aragorn");
        assert!(aragorn.runtime_settings().macros.is_empty());
        aragorn.handle_macro_command("{F7} {aragorn}").unwrap();
        aragorn.start_runtime_save().unwrap();
        assert!(aragorn.finish_runtime_save().await.is_none());
        let mut gimli = character_app(&fixture, "gimli");
        assert!(gimli.runtime_settings().macros.is_empty());
        gimli.handle_macro_command("{F8} {gimli}").unwrap();
        gimli.start_runtime_save().unwrap();
        assert!(gimli.finish_runtime_save().await.is_none());
        assert_eq!(fixture.app().runtime_settings().macros[0].command, "shared");
        assert_eq!(
            character_app(&fixture, "Aragorn").runtime_settings().macros[0].command,
            "aragorn"
        );
        assert_eq!(
            character_app(&fixture, "gimli").runtime_settings().macros[0].command,
            "gimli"
        );
    }

    #[test]
    fn reload_keeps_character_selection_live_rules_and_shared_resource_root() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.root.join("characters")).unwrap();
        let profile = fixture.root.join("characters/aragorn.toml");
        fs::write(&profile, "[panels.output]\ntitle='Aragorn'").unwrap();
        let mut app = character_app(&fixture, "aragorn");
        assert_eq!(app.config.panels.output.title, "Aragorn");
        assert_eq!(app.config_path, Some(fixture.root.join("config.toml")));
        app.handle_macro_command("{F7} {unsaved}").unwrap();
        fs::write(&profile, "[panels.output]\ntitle='Ranger'").unwrap();
        app.reload_config().unwrap();
        assert_eq!(app.config.panels.output.title, "Ranger");
        assert_eq!(app.runtime_settings().macros[0].command, "unsaved");
        fs::write(&profile, "[connection]\nhost='other'").unwrap();
        assert!(app.reload_config().is_err());
        assert_eq!(app.config.panels.output.title, "Ranger");
        assert_eq!(app.runtime_settings().macros[0].command, "unsaved");
        fs::remove_file(&profile).unwrap();
        assert!(app.reload_config().is_err());
        assert_eq!(app.config.panels.output.title, "Ranger");
    }

    #[tokio::test]
    async fn save_command_restores_all_four_rule_types_without_network_commands() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.handle_macro_command("{F5} {look}").unwrap();
        app.handle_macro_command("mode on").unwrap();
        app.handle_alias_command("{rr} {look}").unwrap();
        app.handle_trigger_command("plain {hungry} {eat bread}")
            .unwrap();
        app.handle_highlights_command("{danger} {red}").unwrap();
        let expected = app.runtime_settings();
        let (tx, mut rx) = mpsc::channel(8);
        app.handle_command(ClientCommand::SendText("/save".into()), &tx)
            .await;
        assert!(app.runtime_save.is_some());
        assert!(app.start_runtime_save().unwrap_err().contains("pending"));
        assert!(app.finish_runtime_save().await.is_none());
        assert!(rx.try_recv().is_err());
        let mut restarted = fixture.app();
        assert_eq!(restarted.runtime_settings(), expected);
        assert!(!restarted.macros.printable_mode);
        assert_eq!(restarted.aliases.expand("rr").unwrap(), ["look"]);
        assert_eq!(
            restarted
                .triggers
                .evaluate("hungry", OutputCategory::Normal)
                .commands,
            ["eat bread"]
        );
        assert_eq!(
            restarted
                .highlights
                .style_for("danger", OutputCategory::Normal)
                .unwrap()
                .foreground
                .as_deref(),
            Some("red")
        );
        assert!(restarted.macros.listing().contains("look"));
        assert!(restarted.macros.listing().contains("score"));
    }

    #[tokio::test]
    async fn removals_survive_reload_and_are_persisted_only_on_save() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.handle_macro_command("{F5} {look}").unwrap();
        app.handle_alias_command("{rr} {look}").unwrap();
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        app.handle_macro_command("remove {F5}").unwrap();
        app.handle_alias_command("clear").unwrap();
        app.reload_config().unwrap();
        assert_eq!(app.runtime_settings().count(), 0);
        assert_eq!(fixture.app().runtime_settings().count(), 2);
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        let restarted = fixture.app();
        assert_eq!(restarted.runtime_settings().count(), 0);
        assert!(restarted.macros.listing().contains("score"));
    }

    #[tokio::test]
    async fn edits_after_save_request_are_not_overwritten_by_completion() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.handle_macro_command("{F5} {look}").unwrap();
        app.start_runtime_save().unwrap();
        app.handle_macro_command("{F5} {north}").unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        assert_eq!(app.macros.runtime_configs()[0].command, "north");
        assert_eq!(fixture.app().macros.runtime_configs()[0].command, "look");
    }

    #[tokio::test]
    async fn corrupt_snapshot_restores_nothing_and_is_not_overwritten() {
        let fixture = Fixture::new();
        let app = fixture.app();
        let path = app.runtime_store.path().unwrap().to_owned();
        fs::write(&path, "version=1\n[[macros]]\nkey='F6'\ncommand='north'\n[[aliases]]\nname='broken'\npattern='['\ncommands=['look']\n").unwrap();
        let before = fs::read(&path).unwrap();
        let mut restarted = fixture.app();
        assert_eq!(restarted.runtime_settings().count(), 0);
        assert!(restarted.macros.listing().contains("score"));
        assert!(
            restarted
                .state
                .output
                .iter()
                .any(|line| line.normalized.contains("not loaded"))
        );
        restarted.start_runtime_save().unwrap();
        assert!(
            restarted
                .finish_runtime_save()
                .await
                .unwrap()
                .contains("startup load failed")
        );
        assert_eq!(fs::read(path).unwrap(), before);
    }

    #[tokio::test]
    async fn save_restores_variables_before_dependent_aliases_and_triggers() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        let path = app.runtime_store.path().unwrap().to_owned();
        app.handle_variable_command("{z_food} {bread}").unwrap();
        app.handle_variable_command("{food} {${z_food}}").unwrap();
        app.handle_alias_command("{eatfood} {eat ${food}}").unwrap();
        app.handle_trigger_command("plain {hungry} {eat ${food}}")
            .unwrap();
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        let mut restarted = fixture.app();
        assert_eq!(restarted.variables.expand("${food}").unwrap(), "bread");
        assert_eq!(restarted.variables.runtime_values()["food"], "${z_food}");
        assert_eq!(restarted.aliases.expand("eatfood").unwrap(), ["eat bread"]);
        assert_eq!(
            restarted
                .triggers
                .evaluate("hungry", OutputCategory::Normal)
                .commands,
            ["eat bread"]
        );
        restarted.handle_alias_command("unset {eatfood}").unwrap();
        restarted.handle_trigger_command("clear").unwrap();
        restarted.handle_variable_command("unset {food}").unwrap();
        restarted.handle_variable_command("unset {z_food}").unwrap();
        restarted.start_runtime_save().unwrap();
        assert!(restarted.finish_runtime_save().await.is_none());
        assert!(fixture.app().variables.runtime_values().is_empty());
        assert!(fs::read_to_string(path).unwrap().contains("variables"));
    }

    #[tokio::test]
    async fn saved_variables_are_isolated_and_available_to_profile_lua_at_initialization() {
        let fixture = Fixture::new();
        profiles(&fixture);
        fs::create_dir(fixture.root.join("var_scripts")).unwrap();
        fs::write(
            fixture.root.join("var_scripts/init.lua"),
            "local food = client.var.get('food')\nfunction captured_food() client.send(food) end",
        )
        .unwrap();
        fs::write(fixture.root.join("characters/aragorn.toml"), "[lua]\nenabled=true\nscript_dir='var_scripts'\nentrypoint='init.lua'\n[variables.values]\nfood='configured'").unwrap();
        let mut aragorn = character_app(&fixture, "aragorn");
        aragorn.handle_variable_command("{food} {bread}").unwrap();
        aragorn.start_runtime_save().unwrap();
        assert!(aragorn.finish_runtime_save().await.is_none());
        let mut restart = character_app(&fixture, "aragorn");
        let result = restart
            .lua
            .call_hook(
                "captured_food",
                crate::scripting::lua::LuaHookContext::default(),
                &restart.state,
                &restart.variables,
            )
            .unwrap();
        assert_eq!(
            result.actions,
            [crate::scripting::lua::LuaAction::Send("bread".into())]
        );
        let mut app = fixture.app();
        report_character(&mut app, "Aragorn").await;
        assert_eq!(app.variables.expand("${food}").unwrap(), "bread");
        let result = app
            .lua
            .call_hook(
                "captured_food",
                crate::scripting::lua::LuaHookContext::default(),
                &app.state,
                &app.variables,
            )
            .unwrap();
        assert_eq!(
            result.actions,
            [crate::scripting::lua::LuaAction::Send("bread".into())]
        );
        report_character(&mut app, "Gimli").await;
        assert!(app.variables.expand("${food}").is_err());
        report_character(&mut app, "Aragorn").await;
        app.handle_variable_command("unset {food}").unwrap();
        app.start_runtime_save().unwrap();
        assert!(app.finish_runtime_save().await.is_none());
        assert_eq!(
            character_app(&fixture, "aragorn")
                .variables
                .expand("${food}")
                .unwrap(),
            "configured"
        );
    }

    #[tokio::test]
    async fn invalid_save_arguments_are_local_errors() {
        let fixture = Fixture::new();
        let mut app = fixture.app();
        let (tx, mut rx) = mpsc::channel(8);
        for command in ["/save extra", "/save\textra"] {
            app.handle_command(ClientCommand::SendText(command.into()), &tx)
                .await;
        }
        assert!(app.runtime_save.is_none());
        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("usage: /save"))
        );
    }
}
