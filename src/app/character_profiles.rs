use super::*;

/// Live automation is parked per profile so account-menu switches neither lose
/// unsaved edits nor carry another character's commands into the new session.
pub(super) struct CharacterSession {
    config: AppConfig,
    aliases: AliasEngine,
    macros: crate::macros::MacroEngine,
    keybindings: crate::keybindings::KeybindingEngine,
    triggers: TriggerEngine,
    events: EventEngine,
    variables: VariableStore,
    highlights: HighlightEngine,
    substitutions: crate::scripting::substitutions::SubstitutionEngine,
    lua: LuaEngine,
    runtime_store: crate::persistence::RuntimeStore,
}

impl CharacterSession {
    fn disabled(app: &App) -> Self {
        Self {
            config: app.config.clone(),
            aliases: AliasEngine::disabled(),
            macros: crate::macros::MacroEngine::default(),
            keybindings: crate::keybindings::KeybindingEngine::default(),
            triggers: TriggerEngine::disabled(),
            events: EventEngine::disabled(),
            variables: VariableStore::empty(),
            highlights: HighlightEngine::disabled(),
            substitutions: crate::scripting::substitutions::SubstitutionEngine::disabled(),
            lua: LuaEngine::disabled(),
            runtime_store: crate::persistence::RuntimeStore::new(None),
        }
    }
    fn load(app: &App, path: Option<&Path>) -> std::result::Result<Self, String> {
        let config =
            AppConfig::load_with_character(app.config_path.clone(), path, app.config_load_options)
                .map_err(|error| error.to_string())?;
        let mut runtime_store =
            crate::persistence::RuntimeStore::new(path.or(app.config_path.as_deref()));
        let saved = runtime_store
            .load(&config)
            .map_err(|error| error.to_string())?;
        let variables = match &saved {
            Some(saved) => saved.variables.clone(),
            None => VariableStore::new(&config.variables).map_err(|error| error.to_string())?,
        };
        let mut session = Self {
            aliases: AliasEngine::new_with_variables(&config.aliases, &variables)
                .map_err(|error| error.to_string())?,
            macros: crate::macros::MacroEngine::new(&config.macros)?,
            keybindings: crate::keybindings::KeybindingEngine::new(&config.keybindings)?,
            triggers: TriggerEngine::new_with_variables(&config.triggers, &variables)
                .map_err(|error| error.to_string())?,
            events: EventEngine::new_with_variables(&config.events, &variables)
                .map_err(|error| error.to_string())?,
            highlights: HighlightEngine::new(&config.highlights)
                .map_err(|error| error.to_string())?,
            substitutions: crate::scripting::substitutions::SubstitutionEngine::new(
                &config.substitutions,
            )
            .map_err(|error| error.to_string())?,
            lua: LuaEngine::new(
                &config.lua,
                app.config_path.as_deref(),
                &app.state,
                &variables,
            ),
            variables,
            runtime_store,
            config,
        };
        if let Some(saved) = saved {
            session.aliases = saved.aliases;
            session.macros = saved.macros;
            session.triggers = saved.triggers;
            session.highlights = saved.highlights;
            session.substitutions = saved.substitutions;
        }
        Ok(session)
    }

    fn swap(&mut self, app: &mut App) {
        std::mem::swap(&mut self.config, &mut app.config);
        std::mem::swap(&mut self.aliases, &mut app.aliases);
        std::mem::swap(&mut self.macros, &mut app.macros);
        std::mem::swap(&mut self.keybindings, &mut app.keybindings);
        std::mem::swap(&mut self.triggers, &mut app.triggers);
        std::mem::swap(&mut self.events, &mut app.events);
        std::mem::swap(&mut self.variables, &mut app.variables);
        std::mem::swap(&mut self.highlights, &mut app.highlights);
        std::mem::swap(&mut self.substitutions, &mut app.substitutions);
        std::mem::swap(&mut self.lua, &mut app.lua);
        std::mem::swap(&mut self.runtime_store, &mut app.runtime_store);
    }
}

impl App {
    pub(super) async fn select_msdp_character(
        &mut self,
        name: &str,
        frames: &[crate::network::msdp::MsdpFrame],
    ) {
        self.select_character(name, false, frames).await;
    }

    pub(super) async fn refresh_character_selection(&mut self) {
        let name = self.msdp_character.clone().or_else(|| {
            self.character_path
                .as_deref()
                .and_then(Path::file_stem)
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        });
        if let Some(name) = name {
            self.select_character(&name, true, &[]).await;
        }
    }

    async fn select_character(
        &mut self,
        name: &str,
        force: bool,
        frames: &[crate::network::msdp::MsdpFrame],
    ) {
        let name = name.trim().to_ascii_lowercase();
        let had_identity = self.msdp_character.is_some();
        let changed = self.msdp_character.as_deref() != Some(&name);
        if name.is_empty() || (!changed && !force) {
            return;
        }
        let Some(base) = self.config_path.as_deref() else {
            return;
        };
        // Never let a server-provided name select an arbitrary filesystem path.
        let candidate = crate::profiles::character_path(base, &name).ok();
        self.msdp_character = Some(name);
        let mut path = match candidate
            .map(crate::profiles::existing_character_path)
            .transpose()
        {
            Ok(path) => path.flatten(),
            Err(error) => {
                self.state.push_output(
                    format!("Character profile could not be loaded: {error}"),
                    OutputCategory::Error,
                );
                None
            }
        };
        if changed {
            self.state.vim.reset();
            self.lua_timers.clear();
            self.state.path = Default::default();
            self.state.script_events.clear();
            // The first identity can arrive after class/vital reports. Those
            // belong to this login; only a subsequent identity replaces them.
            if had_identity {
                self.state.raw_msdp.clear();
                self.state.character = Default::default();
                self.state.opponent = Default::default();
                self.state.group = Default::default();
            }
            // Profile entrypoints must see the incoming identity, not the
            // outgoing character (or a blank identity awaiting this batch).
            self.state
                .apply_msdp_frames(frames, &self.config.msdp.mapping);
        }
        if path == self.character_path {
            return;
        }
        // A pending save owns the outgoing snapshot. Finish it before parking
        // its store so completion cannot overwrite the incoming profile's store.
        self.finish_runtime_save().await;
        let mut next = match self.character_sessions.remove(&path) {
            Some(session) => session,
            None => match CharacterSession::load(self, path.as_deref()) {
                Ok(session) => session,
                Err(error) => {
                    self.state.push_output(
                        format!("Character profile could not be loaded: {error}"),
                        OutputCategory::Error,
                    );
                    // Fall back to shared automation, never the outgoing
                    // character's rules. Keep the outgoing session intact.
                    path = None;
                    if self.character_path.is_none() {
                        return;
                    }
                    match self.character_sessions.remove(&None) {
                        Some(session) => session,
                        None => CharacterSession::load(self, None).unwrap_or_else(|error| {
                            self.state.push_output(format!("Shared configuration could not be loaded; automation disabled: {error}"), OutputCategory::Error);
                            CharacterSession::disabled(self)
                        }),
                    }
                }
            },
        };
        next.swap(self);
        // Early reports may use variable names understood only by this profile.
        // Rebuild the typed sheet with its mapping, keeping the raw report set.
        let reports = self
            .state
            .raw_msdp
            .iter()
            .map(|(variable, value)| crate::network::msdp::MsdpFrame {
                variable: variable.clone(),
                value: value.clone(),
            })
            .collect::<Vec<_>>();
        self.state.character = Default::default();
        self.state.opponent = Default::default();
        self.state.group = Default::default();
        self.state
            .apply_msdp_frames(&reports, &self.config.msdp.mapping);
        if self.state.character.name.is_none() {
            // Identity selection also accepts the canonical server variable
            // when a profile has configured a different name mapping.
            self.state.character.name = self
                .state
                .raw_msdp
                .get("CHARACTER_NAME")
                .and_then(crate::network::msdp::MsdpValue::as_string)
                .map(str::to_owned);
        }
        let previous = std::mem::replace(&mut self.character_path, path);
        self.character_sessions.insert(previous, next);
        self.lua_timers.clear();
        self.state.path = Default::default();
        self.state.script_events.clear();
        self.theme = Theme::from_config(&self.config.colors);
        self.state.input_mode = self.config.terminal.input_mode;
        self.state.vim.reset();
        self.animations.configure(&self.config.animation);
        self.panel_cache.clear();
        self.full_hd_overrides = Default::default();
        self.ultrawide_overrides = Default::default();
        self.stacked_overrides = Default::default();
        self.state
            .set_scrollback_limit(self.config.layout.scrollback_lines);
        if let Some(path) = &self.character_path {
            self.state.push_output(
                format!("Character profile: {}", path.display()),
                OutputCategory::System,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::msdp::{MsdpFrame, MsdpValue};
    use ratatui::{buffer::Buffer, layout::Rect};

    fn frame(variable: &str, value: &str) -> MsdpFrame {
        MsdpFrame {
            variable: variable.into(),
            value: MsdpValue::String(value.into()),
        }
    }

    fn character_panel(app: &App) -> String {
        let area = Rect::new(0, 0, 80, 16);
        let mut buffer = Buffer::empty(area);
        crate::ui::character::render_character_panel(
            area,
            &mut buffer,
            &app.state,
            &app.theme,
            &app.config.gauges,
            "Character",
        );
        buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .replace('\u{a0}', " ")
    }

    #[tokio::test]
    async fn character_details_survive_every_initial_report_order_and_late_updates() {
        // No profile exists: this still exercises automatic identity selection.
        let path = std::env::temp_dir().join(format!(
            "mud-character-report-order-{}/config.toml",
            std::process::id()
        ));
        let (tx, _rx) = mpsc::channel(4);
        for class in ["MAGE_LEVEL", "MYSTIC_LEVEL"] {
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let mut app = App::new(AppConfig::default());
                app.config_path = Some(path.clone());
                let batches = [
                    vec![frame("CHARACTER_NAME", "hero")],
                    vec![frame(class, "57")],
                    vec![frame("SPELL_POWER", "7"), frame("WILLPOWER", "44")],
                ];
                app.handle_network_event(NetworkEvent::Connected, &tx).await;
                let _ = character_panel(&app); // Paint the initially empty sheet.
                for index in order {
                    app.handle_network_event(NetworkEvent::Msdp(batches[index].clone()), &tx)
                        .await;
                    let _ = character_panel(&app);
                }
                let expected = if class == "MAGE_LEVEL" {
                    "Spell Power 7"
                } else {
                    "Willpower 44"
                };
                assert!(
                    character_panel(&app).contains(expected),
                    "{class}: {order:?}"
                );
                assert!(app.state.raw_msdp.contains_key(class));
                app.handle_network_event(
                    NetworkEvent::Msdp(vec![frame("SPELL_POWER", "9"), frame("WILLPOWER", "55")]),
                    &tx,
                )
                .await;
                let expected = if class == "MAGE_LEVEL" {
                    "Spell Power 9"
                } else {
                    "Willpower 55"
                };
                assert!(character_panel(&app).contains(expected));
            }
        }
    }

    #[tokio::test]
    async fn early_reports_use_the_selected_profiles_mapping() {
        let root = std::env::temp_dir().join(format!(
            "mud-character-custom-reports-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("characters")).unwrap();
        let path = root.join("config.toml");
        std::fs::write(
            &path,
            "[lua]\nenabled=false\n[map.persistence]\nload_on_startup=false\n",
        )
        .unwrap();
        std::fs::write(
            root.join("characters/hero.toml"),
            "[msdp.mapping]\ncharacter_name='HERO_NAME'\nmage_level='HERO_MAGE_LEVEL'\nspell_power='HERO_POWER'\n",
        )
        .unwrap();
        let config = AppConfig::load(Some(path.clone())).unwrap();
        let mut app = App::new_with_config_source(config, Some(path), ConfigLoadOptions::default());
        let (tx, _rx) = mpsc::channel(4);
        app.handle_network_event(
            NetworkEvent::Msdp(vec![
                frame("HERO_MAGE_LEVEL", "57"),
                frame("HERO_POWER", "7"),
            ]),
            &tx,
        )
        .await;
        assert_eq!(app.state.character.mage_level, None);
        app.handle_network_event(
            NetworkEvent::Msdp(vec![frame("CHARACTER_NAME", "hero")]),
            &tx,
        )
        .await;
        assert!(character_panel(&app).contains("Spell Power 7"));
        assert_eq!(app.state.character.name.as_deref(), Some("hero"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn character_switch_and_reconnect_discard_previous_character_reports() {
        let mut app = App::new(AppConfig::default());
        app.config_path = Some(std::env::temp_dir().join(format!(
            "mud-character-report-reset-{}/config.toml",
            std::process::id()
        )));
        let (tx, _rx) = mpsc::channel(4);
        app.handle_network_event(
            NetworkEvent::Msdp(vec![
                frame("CHARACTER_NAME", "mage"),
                frame("MAGE_LEVEL", "57"),
                frame("SPELL_POWER", "7"),
            ]),
            &tx,
        )
        .await;
        assert!(character_panel(&app).contains("Spell Power 7"));
        app.handle_network_event(
            NetworkEvent::Msdp(vec![
                frame("CHARACTER_NAME", "mystic"),
                frame("MYSTIC_LEVEL", "58"),
                frame("WILLPOWER", "44"),
            ]),
            &tx,
        )
        .await;
        assert!(character_panel(&app).contains("Willpower 44"));
        assert_eq!(app.state.character.mage_level, None);
        assert!(!app.state.raw_msdp.contains_key("SPELL_POWER"));
        app.handle_network_event(NetworkEvent::Disconnected, &tx)
            .await;
        app.handle_network_event(NetworkEvent::Connected, &tx).await;
        assert_eq!(app.state.character, Default::default());
        assert!(app.state.raw_msdp.is_empty());
        // A new login can again deliver its levels before the identity.
        app.handle_network_event(
            NetworkEvent::Msdp(vec![frame("MAGE_LEVEL", "57"), frame("SPELL_POWER", "9")]),
            &tx,
        )
        .await;
        app.handle_network_event(
            NetworkEvent::Msdp(vec![frame("CHARACTER_NAME", "mage")]),
            &tx,
        )
        .await;
        assert!(character_panel(&app).contains("Spell Power 9"));
        assert_eq!(app.state.character.mystic_level, None);
    }
}
