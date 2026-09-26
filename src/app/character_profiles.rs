use super::*;

/// Live automation is parked per profile so account-menu switches neither lose
/// unsaved edits nor carry another character's commands into the new session.
pub(super) struct CharacterSession {
    config: AppConfig,
    aliases: AliasEngine,
    macros: crate::macros::MacroEngine,
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
            self.state.raw_msdp.clear();
            self.state.character = Default::default();
            self.state.opponent = Default::default();
            self.state.group = Default::default();
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
