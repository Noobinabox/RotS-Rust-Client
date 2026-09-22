use std::{
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::Local;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Frame, layout::Rect};
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};

mod formatting;
mod local_commands;
mod macros;
mod mouse;
mod rendering;
mod social;

pub use self::rendering::render;
use self::{
    formatting::{format_msdp_value, markdown_inline, push_output_lines},
    local_commands::{
        is_recognized_local_command, parse_echo, preserves_variable_templates, split_first_token,
        split_game_commands,
    },
    mouse::{divider_at, mouse_over_output, rect_contains},
    rendering::{layout_minimums, pane_min_width, render_with_overrides},
    social::classify_social_line,
};

use crate::{
    animation::scheduler::AnimationScheduler,
    color::{AnsiColorState, AnsiColors, parse_ansi_color},
    commands::ClientCommand,
    config::{
        AppConfig, ConfigLoadOptions, HighlightRuleConfig, MatchType, OutputCategoryConfig,
        PaneRole,
    },
    error::Result,
    events::{AppEvent, NetworkEvent, ScriptEvent, TerminalEvent, TimerEvent},
    input::{InputAction, handle_key},
    map::is_movement_command,
    network::connection::run_connection,
    scripting::{
        aliases::AliasEngine,
        events::EventEngine,
        highlights::HighlightEngine,
        lua::{LuaAction, LuaEngine, LuaHookContext, LuaLogLevel, LuaOutputOptions},
        triggers::TriggerEngine,
        variables::{VariableSource, VariableStore},
    },
    state::{
        AppState, ConnectionStatus, OutputCategory, OutputStyle, SearchDirection,
        is_casting_spinner_line, plain_text,
    },
    terminal::{TerminalGuard, spawn_terminal_events},
    ui::{
        character::{ResponsivePanelConfig, social_scroll_max, status_dashboard_social_area},
        layout::{
            Divider, LayoutOverrides, MIN_CENTER_WIDTH, ResolvedLayout, UiMode, mode_for,
            resolve_layout,
        },
        map::map_snapshot_lines,
        theme::{Theme, parse_color},
    },
};

pub struct App {
    config: AppConfig,
    config_path: Option<PathBuf>,
    config_load_options: ConfigLoadOptions,
    state: AppState,
    theme: Theme,
    aliases: AliasEngine,
    macros: crate::macros::MacroEngine,
    triggers: TriggerEngine,
    events: EventEngine,
    variables: VariableStore,
    highlights: HighlightEngine,
    lua: LuaEngine,
    animations: AnimationScheduler,
    last_terminal_area: Rect,
    full_hd_overrides: LayoutOverrides,
    ultrawide_overrides: LayoutOverrides,
    stacked_overrides: LayoutOverrides,
    active_divider: Option<Divider>,
    dispatching_handler_commands: bool,
    mud_ansi_colors: AnsiColorState,
    lua_timers: BTreeMap<String, LuaTimer>,
}

struct LuaTimer {
    interval: Duration,
    next_due: Instant,
    callback: String,
    repeat: bool,
    tick_count: u64,
}

struct CommandBudget {
    remaining: usize,
    limit: usize,
    source: &'static str,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self::new_with_options(config, ConfigLoadOptions::default())
    }

    pub fn new_with_options(config: AppConfig, config_load_options: ConfigLoadOptions) -> Self {
        Self::new_with_config_source(config, None, config_load_options)
    }

    pub fn new_with_config_source(
        config: AppConfig,
        config_path: Option<PathBuf>,
        config_load_options: ConfigLoadOptions,
    ) -> Self {
        let theme = Theme::from_config(&config.colors);
        let mut state = AppState::new(&config);
        Self::load_startup_map(&mut state, &config, config_path.as_deref());
        let variables = VariableStore::new(&config.variables).unwrap_or_else(|error| {
            tracing::error!(target: "mud_client::app", error = %error, "variable setup failed; variables disabled");
            VariableStore::empty()
        });
        let aliases =
            AliasEngine::new_with_variables(&config.aliases, &variables).unwrap_or_else(|error| {
            tracing::error!(target: "mud_client::app", error = %error, "alias setup failed; aliases disabled");
            AliasEngine::disabled()
        });
        let triggers = TriggerEngine::new_with_variables(&config.triggers, &variables).unwrap_or_else(|error| {
            tracing::error!(target: "mud_client::app", error = %error, "trigger setup failed; triggers disabled");
            TriggerEngine::disabled()
        });
        let events = EventEngine::new_with_variables(&config.events, &variables).unwrap_or_else(|error| {
            tracing::error!(target: "mud_client::app", error = %error, "event setup failed; events disabled");
            EventEngine::disabled()
        });
        let highlights = HighlightEngine::new(&config.highlights).unwrap_or_else(|error| {
            tracing::error!(target: "mud_client::app", error = %error, "highlight setup failed; highlights disabled");
            HighlightEngine::disabled()
        });
        let lua = LuaEngine::new(&config.lua, config_path.as_deref(), &state, &variables);
        let macros = crate::macros::MacroEngine::new(&config.macros).unwrap_or_else(|error| {
            state.push_output(
                format!("Macro setup failed: {error}"),
                OutputCategory::Error,
            );
            crate::macros::MacroEngine::default()
        });
        let animations = AnimationScheduler::new(&config.animation);
        Self {
            config,
            config_path,
            config_load_options,
            state,
            theme,
            aliases,
            macros,
            triggers,
            events,
            variables,
            highlights,
            lua,
            animations,
            last_terminal_area: Rect::default(),
            full_hd_overrides: LayoutOverrides::default(),
            ultrawide_overrides: LayoutOverrides::default(),
            stacked_overrides: LayoutOverrides::default(),
            active_divider: None,
            dispatching_handler_commands: false,
            mud_ansi_colors: AnsiColorState::default(),
            lua_timers: BTreeMap::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let (_guard, mut terminal) = TerminalGuard::enter(self.config.terminal.mouse)?;
        let initial_size = terminal.size()?;
        self.last_terminal_area = Rect::new(0, 0, initial_size.width, initial_size.height);
        let initial_window_size = self.mud_window_size();
        let (event_tx, mut event_rx) = mpsc::channel(512);
        let (network_tx, mut network_rx) = mpsc::channel(256);
        let (command_tx, command_rx) = mpsc::channel(256);
        let mut network_handle = self.spawn_connection(network_tx, command_rx, initial_window_size);
        spawn_terminal_events(self.config.terminal.tick_rate(), event_tx.clone());
        tokio::spawn({
            let event_tx = event_tx.clone();
            async move {
                while let Some(event) = network_rx.recv().await {
                    if event_tx.send(AppEvent::Network(event)).await.is_err() {
                        break;
                    }
                }
            }
        });

        self.state.connection = ConnectionStatus::Connecting;
        self.state.push_output(
            format!(
                "Connecting to {}:{}",
                self.config.connection.host, self.config.connection.port
            ),
            OutputCategory::System,
        );

        let mut tick = tokio::time::interval(self.config.terminal.tick_rate());
        let mut should_quit = false;
        let mut needs_draw = true;

        while !should_quit {
            if needs_draw {
                terminal.draw(|frame| {
                    self.last_terminal_area = frame.area();
                    self.render(frame);
                })?;
                needs_draw = false;
            }

            tokio::select! {
                _ = tick.tick() => {
                    self.handle_app_event(AppEvent::Timer(TimerEvent::Tick), &command_tx).await;
                    needs_draw = true;
                }
                event = event_rx.recv() => {
                    if let Some(event) = event {
                        should_quit = self.handle_app_event(event, &command_tx).await;
                        needs_draw = true;
                        while !should_quit {
                            match event_rx.try_recv() {
                                Ok(event) => {
                                    should_quit = self.handle_app_event(event, &command_tx).await;
                                }
                                Err(mpsc::error::TryRecvError::Empty) => break,
                                Err(mpsc::error::TryRecvError::Disconnected) => {
                                    should_quit = true;
                                    break;
                                }
                            }
                        }
                    } else {
                        should_quit = true;
                    }
                }
            }
        }

        let _ = command_tx.send(ClientCommand::Quit).await;
        if timeout(self.config.terminal.tick_rate(), &mut network_handle)
            .await
            .is_err()
        {
            network_handle.abort();
        }
        self.save_exit_map();
        Ok(())
    }

    fn load_startup_map(state: &mut AppState, config: &AppConfig, config_path: Option<&Path>) {
        let persistence = &config.map.persistence;
        if !persistence.load_on_startup {
            return;
        }
        let path = Self::resolve_map_path(config_path, &persistence.path);
        match state.map.read_from_path(&path) {
            Ok(()) => state.push_output(
                format!("Read map from {}.", path.display()),
                OutputCategory::System,
            ),
            Err(error) => state.push_output(
                format!("Failed to read map from {}: {error}", path.display()),
                OutputCategory::Error,
            ),
        }
    }

    fn save_exit_map(&self) {
        let persistence = &self.config.map.persistence;
        if !persistence.save_on_exit {
            return;
        }
        let path = Self::resolve_map_path(self.config_path.as_deref(), &persistence.path);
        if let Err(error) = self.state.map.write_to_path(&path) {
            tracing::error!(target: "mud_client::app", path = %path.display(), error = %error, "failed to save map on exit");
        }
    }

    fn resolve_map_path(config_path: Option<&Path>, value: &str) -> PathBuf {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return path;
        }
        config_path
            .and_then(Path::parent)
            .map(|parent| parent.join(&path))
            .unwrap_or(path)
    }

    fn spawn_connection(
        &self,
        network_tx: mpsc::Sender<NetworkEvent>,
        command_rx: mpsc::Receiver<ClientCommand>,
        initial_window_size: Option<(u16, u16)>,
    ) -> JoinHandle<()> {
        let config = self.config.clone();
        tokio::spawn(async move {
            run_connection(config, network_tx, command_rx, initial_window_size).await;
        })
    }

    async fn handle_app_event(
        &mut self,
        event: AppEvent,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) -> bool {
        match event {
            AppEvent::Terminal(event) => self.handle_terminal_event(event, command_tx).await,
            AppEvent::Network(event) => {
                self.handle_network_event(event, command_tx).await;
                false
            }
            AppEvent::Timer(TimerEvent::Tick) => {
                self.state.tick_output_status();
                let now = Instant::now();
                self.animations.tick(now);
                self.run_due_lua_timers(now, command_tx).await;
                false
            }
            AppEvent::Script(event) => {
                self.dispatch_script_event(event, "application", command_tx, &mut Vec::new())
                    .await;
                false
            }
            AppEvent::Command(command) => self.handle_command(command, command_tx).await,
        }
    }

    async fn handle_command(
        &mut self,
        command: ClientCommand,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) -> bool {
        if command == ClientCommand::Quit {
            return true;
        }
        match command {
            ClientCommand::SendText(text) => {
                if text.is_empty() {
                    let _ = command_tx.send(ClientCommand::SendText(text)).await;
                    return false;
                }
                let text = if preserves_variable_templates(&text) {
                    text
                } else {
                    match self.variables.expand(&text) {
                        Ok(text) => text,
                        Err(error) => {
                            self.state
                                .push_output(error.to_string(), OutputCategory::Error);
                            return false;
                        }
                    }
                };
                if text.trim() == "/quit" {
                    return true;
                }
                if text.trim() == "/clear" {
                    self.state.clear_output();
                    self.mud_ansi_colors.reset();
                    return false;
                }
                self.handle_text_commands(&text, command_tx, &mut Vec::new())
                    .await;
            }
            command => {
                let _ = command_tx.send(command).await;
            }
        }
        false
    }

    async fn handle_text_commands(
        &mut self,
        text: &str,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) -> bool {
        let commands = match split_game_commands(text) {
            Ok(commands) => commands,
            Err(error) => {
                self.state.push_output(error, OutputCategory::Error);
                return true;
            }
        };
        for command in commands {
            if is_recognized_local_command(&command) {
                if !self.consume_command_budget(budgets) {
                    return false;
                }
                self.handle_local_command(&command, command_tx, budgets)
                    .await;
                continue;
            }
            match self.aliases.expand_actions(&command) {
                Ok(actions) => {
                    if let Some(lua) = actions.lua {
                        let context = LuaHookContext {
                            kind: "alias".to_string(),
                            input: Some(lua.input),
                            captures: lua.captures,
                            ..LuaHookContext::default()
                        };
                        self.run_lua_hook(&lua.function, context, command_tx, budgets)
                            .await;
                    }
                    for command in actions.commands {
                        let commands = match split_game_commands(&command) {
                            Ok(commands) => commands,
                            Err(error) => {
                                self.state.push_output(error, OutputCategory::Error);
                                return true;
                            }
                        };
                        for command in commands {
                            if !self.consume_command_budget(budgets) {
                                return false;
                            }
                            if is_recognized_local_command(&command) {
                                self.handle_local_command(&command, command_tx, budgets)
                                    .await;
                                continue;
                            }
                            self.send_mud_text_command(&command, command_tx).await;
                        }
                    }
                }
                Err(error) => self
                    .state
                    .push_output(error.to_string(), OutputCategory::Error),
            }
        }
        true
    }

    fn consume_command_budget(&mut self, budgets: &mut [CommandBudget]) -> bool {
        if let Some(budget) = budgets.iter().find(|budget| budget.remaining == 0) {
            self.state.push_output(
                format!(
                    "{} exceeded maximum command count of {} after command expansion",
                    budget.source, budget.limit
                ),
                OutputCategory::Error,
            );
            return false;
        }
        for budget in budgets {
            budget.remaining -= 1;
        }
        true
    }

    async fn handle_local_command(
        &mut self,
        text: &str,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) -> bool {
        let Some(command) = text.strip_prefix('/') else {
            return false;
        };
        let command = command.trim();
        let normalized;
        let command = if let Some(index) = command.find(char::is_whitespace) {
            normalized = format!("{} {}", &command[..index], command[index..].trim_start());
            normalized.as_str()
        } else {
            command
        };
        if command == "help" || command.starts_with("help ") {
            let topic = command.strip_prefix("help").unwrap_or_default().trim();
            if let Some(message) = help_text(topic) {
                push_output_lines(&mut self.state, message, OutputCategory::System);
                let first_line = self
                    .state
                    .output
                    .len()
                    .saturating_sub(message.lines().count());
                let output = self.resolved_layout(self.last_terminal_area).output;
                let (offset, rows) = crate::ui::output::position_for_line(
                    &self.state,
                    output.height.saturating_sub(2) as usize,
                    output.width.saturating_sub(2) as usize,
                    &self.theme,
                    first_line,
                );
                self.state.output_view.scroll_offset = offset;
                self.state.output_view.wrapped_row_offset = rows;
                self.state.output_view.follow_newest = (offset, rows) == (0, 0);
            } else {
                push_output_lines(
                    &mut self.state,
                    format!(
                        "Unknown help topic `{}`.\nUse /help for available topics.",
                        topic
                    ),
                    OutputCategory::Error,
                );
            }
            return true;
        }
        if command == "msdp" {
            let message = msdp_snapshot(&self.state);
            push_output_lines(&mut self.state, message, OutputCategory::System);
            return true;
        }
        if command == "lua" || command.starts_with("lua ") {
            let input = command.strip_prefix("lua").unwrap_or_default().trim();
            match self.handle_lua_command(input, command_tx, budgets).await {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "timer" || command.starts_with("timer ") {
            match self
                .handle_timer_command(command.strip_prefix("timer").unwrap_or_default())
                .await
            {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "macro" || command.starts_with("macro ") {
            match self.handle_macro_command(command.strip_prefix("macro").unwrap_or_default()) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "clear" {
            self.state.clear_output();
            self.mud_ansi_colors.reset();
            return true;
        }
        if command == "quit" {
            self.state.push_output(
                "Quit is only available from direct command input.",
                OutputCategory::Error,
            );
            return true;
        }
        if command == "echo" || command.starts_with("echo ") {
            match parse_echo(command.strip_prefix("echo").unwrap_or_default()) {
                Ok((message, style)) => {
                    self.capture_social_line(&message);
                    self.state.push_local_output_styled(
                        message,
                        OutputCategory::Normal,
                        Some(style),
                    );
                }
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "reload" {
            match self.reload_config() {
                Ok(message) => {
                    push_output_lines(&mut self.state, message, OutputCategory::System);
                    self.send_mud_window_size(command_tx).await;
                }
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "reconnect" {
            self.state
                .push_output("Reconnect requested.", OutputCategory::System);
            let _ = command_tx.send(ClientCommand::Reconnect).await;
            return true;
        }
        if command == "alias" || command.starts_with("alias ") {
            match self.handle_alias_command(command.strip_prefix("alias").unwrap_or_default()) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "trigger"
            || command.starts_with("trigger ")
            || command == "triggers"
            || command.starts_with("triggers ")
        {
            let input = command
                .strip_prefix("triggers")
                .or_else(|| command.strip_prefix("trigger"))
                .unwrap_or_default();
            match self.handle_trigger_command(input) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "highlight" || command.starts_with("highlight ") {
            let input = command.strip_prefix("highlight").unwrap_or_default();
            match self.handle_highlights_command(input) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "variable" || command.starts_with("variable ") {
            match self.handle_variable_command(command.strip_prefix("variable").unwrap_or_default())
            {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "handler" || command.starts_with("handler ") {
            match self.handle_handler_command(command.strip_prefix("handler").unwrap_or_default()) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "event" || command.starts_with("event ") {
            if self.dispatching_handler_commands {
                push_output_lines(
                    &mut self.state,
                    "event handlers must use `emit` for follow-up events",
                    OutputCategory::Error,
                );
                return true;
            }
            let input = command.strip_prefix("event").unwrap_or_default().trim();
            if input.is_empty() {
                let message = event_listing(&self.events, &self.state);
                push_output_lines(&mut self.state, message, OutputCategory::System);
            } else {
                if !self.events.is_enabled() {
                    push_output_lines(
                        &mut self.state,
                        "event dispatch is disabled in config",
                        OutputCategory::Error,
                    );
                    return true;
                }
                match parse_braced_fields(input, "event") {
                    Ok(fields) if fields.len() == 1 => {
                        if fields[0].trim().is_empty() {
                            push_output_lines(
                                &mut self.state,
                                "event name must not be empty",
                                OutputCategory::Error,
                            );
                        } else {
                            let event = ScriptEvent::parse(fields[0].trim());
                            Box::pin(
                                self.dispatch_script_event(event, "manual", command_tx, budgets),
                            )
                            .await;
                        }
                    }
                    Ok(_) => push_output_lines(
                        &mut self.state,
                        "usage: /event {name}",
                        OutputCategory::Error,
                    ),
                    Err(error) => {
                        push_output_lines(&mut self.state, error, OutputCategory::Error);
                    }
                }
            }
            return true;
        }
        if command == "toggle" || command.starts_with("toggle ") {
            match self.handle_toggle_command(command.strip_prefix("toggle").unwrap_or_default()) {
                Ok(message) => push_output_lines(&mut self.state, message, OutputCategory::System),
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "map" || command.starts_with("map ") {
            let map_command = command.strip_prefix("map").unwrap_or_default();
            match self.state.map.execute_with_base_path(
                map_command.trim(),
                self.config_path.as_deref().and_then(Path::parent),
            ) {
                Ok(result) => {
                    if let Some((name, value)) = result.variable {
                        let mut variable_failed = false;
                        match self.variables.with_runtime(&name, &value) {
                            Ok(variables) => {
                                if let Err(error) = self.apply_variables(variables) {
                                    push_output_lines(
                                        &mut self.state,
                                        error,
                                        OutputCategory::Error,
                                    );
                                    variable_failed = true;
                                }
                            }
                            Err(error) => {
                                push_output_lines(
                                    &mut self.state,
                                    error.to_string(),
                                    OutputCategory::Error,
                                );
                                variable_failed = true;
                            }
                        }
                        if variable_failed {
                            return true;
                        }
                    }
                    for command in result.commands {
                        if !self.consume_command_budget(budgets) {
                            break;
                        }
                        self.send_mud_text_command(&command, command_tx).await;
                    }
                    if result.show_map {
                        self.push_full_output_map();
                    } else {
                        push_output_lines(&mut self.state, result.message, OutputCategory::System);
                    }
                }
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        if command == "path" || command.starts_with("path ") {
            let path_command = command.strip_prefix("path").unwrap_or_default();
            let variables = self
                .variables
                .entries()
                .unwrap_or_default()
                .into_iter()
                .map(|entry| (entry.name, entry.value))
                .collect();
            match self.state.path.execute(path_command.trim(), &variables) {
                Ok(result) => {
                    if let Some((name, value)) = result.file {
                        let safe_name = name.replace(['/', '\\'], "_");
                        let base = self
                            .config_path
                            .as_deref()
                            .and_then(Path::parent)
                            .unwrap_or_else(|| Path::new("."));
                        let directory = base.join("bot_paths");
                        match fs::create_dir_all(&directory).and_then(|_| {
                            fs::write(directory.join(format!("{safe_name}.path")), value)
                        }) {
                            Ok(()) => push_output_lines(
                                &mut self.state,
                                format!(
                                    "Path mapping `{safe_name}` saved under {}.",
                                    directory.display()
                                ),
                                OutputCategory::System,
                            ),
                            Err(error) => push_output_lines(
                                &mut self.state,
                                format!("Failed to save path mapping `{safe_name}`: {error}"),
                                OutputCategory::Error,
                            ),
                        }
                    }
                    if let Some((name, value)) = result.variable {
                        match self.variables.with_runtime(&name, &value) {
                            Ok(updated) => {
                                if let Err(error) = self.apply_variables(updated) {
                                    push_output_lines(
                                        &mut self.state,
                                        error,
                                        OutputCategory::Error,
                                    );
                                    return true;
                                }
                            }
                            Err(error) => {
                                push_output_lines(
                                    &mut self.state,
                                    error.to_string(),
                                    OutputCategory::Error,
                                );
                                return true;
                            }
                        }
                    }
                    for command in result.commands {
                        self.send_mud_text_command_without_path_recording(&command, command_tx)
                            .await;
                    }
                    if result.show_map {
                        self.push_full_output_map();
                    } else if !result.message.is_empty() {
                        push_output_lines(&mut self.state, result.message, OutputCategory::System);
                    }
                }
                Err(error) => push_output_lines(&mut self.state, error, OutputCategory::Error),
            }
            return true;
        }
        false
    }

    fn reload_config(&mut self) -> std::result::Result<String, String> {
        let show_opponent = self.config.layout.show_opponent;
        let show_group = self.config.layout.show_group;
        let show_social = self.config.layout.show_social;
        let mut config =
            AppConfig::load_with_options(self.config_path.clone(), self.config_load_options)
                .map_err(|error| format!("Config reload failed: {error}"))?;
        config.layout.show_opponent = show_opponent;
        config.layout.show_group = show_group;
        config.layout.show_social = show_social;
        let variables = self
            .variables
            .with_config(&config.variables)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        let runtime_aliases = self.aliases.runtime_configs();
        let macros = self
            .macros
            .with_config(&config.macros)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        let runtime_triggers = self.triggers.runtime_configs();
        let runtime_handlers = self.events.runtime_configs();
        let runtime_highlights = self.highlights.runtime_configs();
        let mut aliases = AliasEngine::new_with_variables(&config.aliases, &variables)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        for alias in runtime_aliases {
            aliases
                .add_runtime_config(alias)
                .map_err(|error| format!("Config reload failed: {error}"))?;
        }
        let mut triggers = TriggerEngine::new_with_variables(&config.triggers, &variables)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        for trigger in runtime_triggers {
            triggers
                .add_runtime_config(trigger)
                .map_err(|error| format!("Config reload failed: {error}"))?;
        }
        let mut events = EventEngine::new_with_variables(&config.events, &variables)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        for handler in runtime_handlers {
            events
                .add_runtime_config(handler)
                .map_err(|error| format!("Config reload failed: {error}"))?;
        }
        let mut highlights = HighlightEngine::new(&config.highlights)
            .map_err(|error| format!("Config reload failed: {error}"))?;
        for highlight in runtime_highlights {
            highlights
                .add_runtime_highlight(highlight)
                .map_err(|error| format!("Config reload failed: {error}"))?;
        }
        self.theme = Theme::from_config(&config.colors);
        self.aliases = aliases;
        self.triggers = triggers;
        self.events = events;
        self.variables = variables;
        self.highlights = highlights;
        self.lua
            .reload(
                &config.lua,
                self.config_path.as_deref(),
                &self.state,
                &self.variables,
            )
            .map_err(|error| format!("Config reload failed: {error}"))?;
        self.animations.configure(&config.animation);
        self.state
            .set_scrollback_limit(config.layout.scrollback_lines);
        let excess_events = self
            .state
            .script_events
            .len()
            .saturating_sub(config.events.history_limit);
        if excess_events > 0 {
            self.state.script_events.drain(..excess_events);
        }
        self.config = config;
        self.macros = macros;
        self.full_hd_overrides = LayoutOverrides::default();
        self.ultrawide_overrides = LayoutOverrides::default();
        self.stacked_overrides = LayoutOverrides::default();
        Ok("# Config Reloaded\n\nConfiguration was reloaded from disk. Session panel toggles were preserved.".to_string())
    }

    fn handle_alias_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(alias_listing(&self.aliases));
        }
        if input.eq_ignore_ascii_case("clear") {
            let removed = self.aliases.clear_runtime_aliases();
            return Ok(format!(
                "# Aliases Cleared\n\nRemoved {removed} runtime aliases."
            ));
        }
        if let Some(input) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(input, "alias unset")?;
            if fields.len() != 1 {
                return Err("usage: /alias unset {pattern}".to_string());
            }
            let pattern = fields[0].trim();
            if self.aliases.remove_runtime_alias(pattern) {
                return Ok(format!(
                    "# Alias Removed\n\nRuntime alias `{}` was removed.",
                    markdown_inline(pattern)
                ));
            }
            return Err(format!("runtime alias `{}` is not defined", pattern));
        }
        let fields = parse_braced_fields(input, "alias")?;
        if fields.len() < 2 {
            return Err("usage: /alias {pattern} {command} [{command}...]".to_string());
        }
        let pattern = fields[0].trim();
        let commands = fields[1..]
            .iter()
            .map(|command| command.trim().to_string())
            .collect::<Vec<_>>();
        self.aliases
            .add_runtime_alias(pattern, commands.clone())
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "# Alias Added\n\n- `{}` -> {}",
            pattern,
            commands
                .iter()
                .map(|command| format!("`{command}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn handle_trigger_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(trigger_listing(&self.triggers));
        }
        if input.eq_ignore_ascii_case("clear") {
            let removed = self.triggers.clear_runtime_triggers();
            return Ok(format!(
                "# Triggers Cleared\n\nRemoved {removed} runtime triggers."
            ));
        }
        if let Some(input) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(input, "trigger unset")?;
            if fields.len() != 1 {
                return Err("usage: /triggers unset {pattern}".to_string());
            }
            let pattern = fields[0].trim();
            if self.triggers.remove_runtime_trigger(pattern) {
                return Ok(format!(
                    "# Trigger Removed\n\nRuntime trigger `{}` was removed.",
                    markdown_inline(pattern)
                ));
            }
            return Err(format!("runtime trigger `{}` is not defined", pattern));
        }

        let (input, match_type) = if let Some(input) = strip_subcommand(input, "plain") {
            (input, Some(MatchType::Plain))
        } else if let Some(input) = strip_subcommand(input, "regex") {
            (input, Some(MatchType::Regex))
        } else {
            (input, None)
        };
        let (input, foreground, background) = parse_trigger_color_options(input)?;
        let fields = parse_braced_fields(input, "trigger")?;
        if fields.len() < 2 {
            return Err(trigger_usage());
        }
        let pattern = fields[0].trim();
        let commands = fields[1..]
            .iter()
            .map(|command| command.trim().to_string())
            .collect::<Vec<_>>();
        self.triggers
            .add_runtime_trigger_with_options(
                pattern,
                commands.clone(),
                match_type,
                foreground,
                background,
            )
            .map_err(|error| error.to_string())?;
        let resolved_type = self
            .triggers
            .trigger_entries()
            .into_iter()
            .find(|(config, runtime)| *runtime && config.pattern == pattern)
            .map_or(MatchType::Plain, |(config, _)| config.match_type);
        Ok(format!(
            "# Trigger Added\n\n- `{}` ({resolved_type:?}) -> {}",
            markdown_inline(pattern),
            commands
                .iter()
                .map(|command| format!("`{}`", markdown_inline(command)))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn handle_highlights_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(highlight_listing(&self.highlights));
        }
        if input.eq_ignore_ascii_case("clear") {
            let removed = self.highlights.clear_runtime_highlights();
            return Ok(format!(
                "# Highlights Cleared\n\nRemoved {removed} runtime highlights."
            ));
        }
        if let Some(input) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(input, "highlight unset")?;
            if fields.len() != 1 {
                return Err("usage: /highlight unset {pattern}".to_string());
            }
            let pattern = fields[0].trim();
            if self.highlights.remove_runtime_highlight(pattern) {
                return Ok(format!(
                    "# Highlight Removed\n\nRuntime highlight `{}` was removed.",
                    markdown_inline(pattern)
                ));
            }
            return Err(format!("runtime highlight `{}` is not defined", pattern));
        }
        let (input, match_type) = if let Some(input) = strip_subcommand(input, "plain") {
            (input, MatchType::Plain)
        } else if let Some(input) = strip_subcommand(input, "regex") {
            (input, MatchType::Regex)
        } else {
            (input, MatchType::Plain)
        };
        let fields = parse_braced_fields(input, "highlight")?;
        if !(2..=4).contains(&fields.len()) {
            return Err(highlights_usage());
        }
        let pattern = fields[0].trim();
        if pattern.is_empty() {
            return Err("highlight pattern must not be empty".to_string());
        }
        let foreground = parse_optional_highlight_color(&fields[1], "foreground")?;
        let background = fields
            .get(2)
            .map(|value| parse_optional_highlight_color(value, "background"))
            .transpose()?
            .flatten();
        let mut config = HighlightRuleConfig {
            match_type,
            pattern: pattern.to_string(),
            foreground,
            background,
            categories: vec![OutputCategoryConfig::Normal],
            ..HighlightRuleConfig::default()
        };
        if let Some(styles) = fields.get(3) {
            apply_highlight_styles(&mut config, styles)?;
        }
        if config.foreground.is_none()
            && config.background.is_none()
            && !config.bold
            && !config.dim
            && !config.italic
            && !config.underline
            && !config.reverse
        {
            return Err("highlight must define at least one color or style".to_string());
        }
        self.highlights
            .add_runtime_highlight(config)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "# Highlight Added\n\n- `{}` ({match_type:?}, runtime)",
            markdown_inline(pattern)
        ))
    }

    fn handle_handler_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(handler_listing(&self.events));
        }
        if input.eq_ignore_ascii_case("clear") {
            let removed = self.events.clear_runtime_handlers();
            return Ok(format!(
                "# Event Handlers Cleared\n\nRemoved {removed} runtime handlers."
            ));
        }
        if let Some(input) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(input, "handler unset")?;
            if fields.len() != 1 {
                return Err("usage: /handler unset {event}".to_string());
            }
            let event = fields[0].trim();
            if self.events.remove_runtime_handler(event) {
                return Ok(format!(
                    "# Event Handler Removed\n\nRuntime handler `{}` was removed.",
                    markdown_inline(event)
                ));
            }
            return Err(format!("runtime event handler `{}` is not defined", event));
        }
        let (input, match_type) = if let Some(input) = strip_subcommand(input, "plain") {
            (input, Some(MatchType::Plain))
        } else if let Some(input) = strip_subcommand(input, "regex") {
            (input, Some(MatchType::Regex))
        } else {
            (input, None)
        };
        let fields = parse_braced_fields(input, "handler")?;
        if fields.len() < 2 {
            return Err(handler_usage());
        }
        let event = fields[0].trim();
        let commands = fields[1..]
            .iter()
            .map(|command| command.trim().to_string())
            .collect::<Vec<_>>();
        self.events
            .add_runtime_handler(event, commands.clone(), match_type)
            .map_err(|error| error.to_string())?;
        let resolved_type = self
            .events
            .handler_entries()
            .into_iter()
            .find(|(config, runtime)| *runtime && config.event == event)
            .map_or(MatchType::Plain, |(config, _)| config.match_type);
        Ok(format!(
            "# Event Handler Added\n\n- `{}` ({resolved_type:?}, runtime) -> {}",
            markdown_inline(event),
            commands
                .iter()
                .map(|command| format!("`{}`", markdown_inline(command)))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    async fn handle_lua_command(
        &mut self,
        input: &str,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() || input == "status" {
            return Ok(format!("# Lua Status\n\n{}", self.lua.status()));
        }
        if input == "reload" {
            return self
                .lua
                .reload(
                    &self.config.lua,
                    self.config_path.as_deref(),
                    &self.state,
                    &self.variables,
                )
                .map(|message| format!("# Lua Reload\n\n{message}"));
        }
        if let Some(rest) = input.strip_prefix("call ") {
            let function = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| "usage: /lua call <function>".to_string())?;
            let context = LuaHookContext {
                kind: "manual".to_string(),
                input: Some(rest.to_string()),
                ..LuaHookContext::default()
            };
            self.run_lua_hook(function, context, command_tx, budgets)
                .await;
            return Ok(format!(
                "# Lua Call\n\nCalled `{}`.",
                markdown_inline(function)
            ));
        }
        Err("usage: /lua [status|reload|call <function>]".to_string())
    }

    async fn run_lua_hook(
        &mut self,
        function: &str,
        context: LuaHookContext,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) {
        let result = self
            .lua
            .call_hook(function, context, &self.state, &self.variables);
        match result {
            Ok(result) => {
                Box::pin(self.apply_lua_actions(result.actions, command_tx, budgets)).await;
            }
            Err(error) => {
                if self.config.lua.runtime_errors_to_output {
                    self.state.push_output(error, OutputCategory::Error);
                } else {
                    tracing::error!(target: "mud_client::lua", error = %error);
                }
            }
        }
    }

    async fn apply_lua_actions(
        &mut self,
        actions: Vec<LuaAction>,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) {
        for action in actions {
            match action {
                LuaAction::Send(command) => {
                    if !self.consume_command_budget(budgets) {
                        break;
                    }
                    self.send_mud_text_command(&command, command_tx).await;
                }
                LuaAction::Echo(message, style) => {
                    self.state.push_local_output_styled(
                        message,
                        OutputCategory::Normal,
                        lua_output_style(style),
                    );
                }
                LuaAction::Notify(message, style) => {
                    self.state.push_local_output_styled(
                        message,
                        OutputCategory::Triggered,
                        lua_output_style(style),
                    );
                }
                LuaAction::Log(level, message) => match level {
                    LuaLogLevel::Debug => tracing::debug!(target: "mud_client::lua", "{}", message),
                    LuaLogLevel::Info => tracing::info!(target: "mud_client::lua", "{}", message),
                    LuaLogLevel::Warn => tracing::warn!(target: "mud_client::lua", "{}", message),
                    LuaLogLevel::Error => tracing::error!(target: "mud_client::lua", "{}", message),
                },
                LuaAction::SetVariable(name, value) => {
                    let variables = self.variables.with_runtime(&name, &value);
                    match variables {
                        Ok(variables) => {
                            if let Err(error) = self.apply_variables(variables) {
                                self.state.push_output(error, OutputCategory::Error);
                            }
                        }
                        Err(error) => self
                            .state
                            .push_output(error.to_string(), OutputCategory::Error),
                    }
                }
                LuaAction::UnsetVariable(name) => {
                    let variables = self.variables.without_runtime(&name);
                    match variables {
                        Ok(variables) => {
                            if let Err(error) = self.apply_variables(variables) {
                                self.state.push_output(error, OutputCategory::Error);
                            }
                        }
                        Err(error) => self
                            .state
                            .push_output(error.to_string(), OutputCategory::Error),
                    }
                }
                LuaAction::EmitEvent(name, source) => {
                    Box::pin(self.dispatch_script_event(
                        ScriptEvent::parse(name),
                        source.unwrap_or_else(|| "lua".to_string()),
                        command_tx,
                        budgets,
                    ))
                    .await;
                }
                LuaAction::LocalCommand(command) => {
                    Box::pin(self.handle_text_commands(&command, command_tx, budgets)).await;
                }
                LuaAction::SetTimer {
                    name,
                    interval_ms,
                    callback,
                    repeat,
                } => {
                    self.set_lua_timer(name, interval_ms, callback, repeat);
                }
                LuaAction::CancelTimer(name) => {
                    self.lua_timers.remove(&name);
                }
            }
        }
    }

    fn set_lua_timer(&mut self, name: String, interval_ms: u64, callback: String, repeat: bool) {
        let name = name.trim().to_string();
        let callback = callback.trim().to_string();
        if name.is_empty() || callback.is_empty() {
            self.state.push_output(
                "Timer name and callback are required.",
                OutputCategory::Error,
            );
            return;
        }
        self.lua_timers.insert(
            name,
            LuaTimer {
                interval: Duration::from_millis(interval_ms.max(1)),
                next_due: Instant::now() + Duration::from_millis(interval_ms.max(1)),
                callback,
                repeat,
                tick_count: 0,
            },
        );
    }

    async fn run_due_lua_timers(&mut self, now: Instant, command_tx: &mpsc::Sender<ClientCommand>) {
        let due = self
            .lua_timers
            .iter_mut()
            .filter_map(|(name, timer)| {
                if timer.next_due > now {
                    return None;
                }
                timer.tick_count += 1;
                let callback = timer.callback.clone();
                let tick_count = timer.tick_count;
                if timer.repeat {
                    timer.next_due = now + timer.interval;
                }
                Some((name.clone(), callback, tick_count, timer.repeat))
            })
            .collect::<Vec<_>>();
        for (name, callback, tick_count, repeat) in due {
            if !repeat {
                self.lua_timers.remove(&name);
            }
            self.run_lua_hook(
                &callback,
                LuaHookContext {
                    kind: "timer".to_string(),
                    timer: Some(name),
                    tick_count: Some(tick_count),
                    ..LuaHookContext::default()
                },
                command_tx,
                &mut Vec::new(),
            )
            .await;
        }
    }

    async fn handle_timer_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let fields = input.split_whitespace().collect::<Vec<_>>();
        match fields.as_slice() {
            [] | ["list"] => {
                if self.lua_timers.is_empty() { return Ok("# Timers\n\nNo timers are scheduled.".to_string()); }
                let mut output = String::from("# Timers\n\n");
                for (name, timer) in &self.lua_timers {
                    writeln!(output, "- `{name}` -> `{}` every {}ms ({})", timer.callback, timer.interval.as_millis(), if timer.repeat { "repeat" } else { "once" }).unwrap();
                }
                Ok(output)
            }
            ["set", name, interval, callback, ..] => {
                let interval_ms = interval.parse::<u64>().map_err(|_| "interval must be milliseconds".to_string())?;
                let repeat = fields.get(4).map(|value| *value != "once").unwrap_or(true);
                self.set_lua_timer((*name).to_string(), interval_ms, (*callback).to_string(), repeat);
                Ok(format!("Timer `{name}` scheduled."))
            }
            ["cancel", name] => { self.lua_timers.remove(*name); Ok(format!("Timer `{name}` cancelled.")) }
            ["clear"] => { self.lua_timers.clear(); Ok("All Lua timers cancelled.".to_string()) }
            _ => Err("usage: /timer [list|set <name> <interval_ms> <lua_function> [once|repeat]|cancel <name>|clear]".to_string()),
        }
    }

    fn handle_variable_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            return variable_listing(&self.variables);
        }
        if let Some(input) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(input, "variable unset")?;
            if fields.len() != 1 {
                return Err("usage: /variable unset {name}".to_string());
            }
            let name = fields[0].trim();
            let variables = self
                .variables
                .without_runtime(name)
                .map_err(|error| error.to_string())?;
            self.apply_variables(variables)?;
            return Ok(format!(
                "# Variable Removed\n\nRuntime variable `{}` was removed.",
                markdown_inline(name)
            ));
        }
        let fields = parse_braced_fields(input, "variable")?;
        if fields.len() != 2 {
            return Err("usage: /variable {name} {value}".to_string());
        }
        let name = fields[0].trim();
        let value = fields[1].as_str();
        let variables = self
            .variables
            .with_runtime(name, value)
            .map_err(|error| error.to_string())?;
        self.apply_variables(variables)?;
        Ok(format!(
            "# Variable Set\n\n- `{}` (runtime): `{}`",
            markdown_inline(name),
            markdown_inline(value)
        ))
    }

    fn apply_variables(&mut self, variables: VariableStore) -> std::result::Result<(), String> {
        let mut aliases = self.aliases.clone();
        let mut triggers = self.triggers.clone();
        let mut events = self.events.clone();
        aliases
            .set_variables(&variables)
            .map_err(|error| error.to_string())?;
        triggers
            .set_variables(&variables)
            .map_err(|error| error.to_string())?;
        events
            .set_variables(&variables)
            .map_err(|error| error.to_string())?;
        self.variables = variables;
        self.aliases = aliases;
        self.triggers = triggers;
        self.events = events;
        Ok(())
    }

    fn handle_toggle_command(&mut self, input: &str) -> std::result::Result<String, String> {
        let mut parts = input.split_whitespace();
        let Some(target) = parts.next() else {
            return Ok(format!(
                "# Optional Panels\n\n- `opponent`: {}\n- `group`: {}\n- `social`: {}",
                on_off(self.config.layout.show_opponent),
                on_off(self.config.layout.show_group),
                on_off(self.config.layout.show_social)
            ));
        };
        let requested = parts.next().map(parse_toggle_value).transpose()?;
        match target.to_ascii_lowercase().as_str() {
            "opponent" => {
                self.config.layout.show_opponent =
                    requested.unwrap_or(!self.config.layout.show_opponent);
                Ok(format!(
                    "Opponent panel {}.",
                    on_off(self.config.layout.show_opponent)
                ))
            }
            "group" => {
                self.config.layout.show_group = requested.unwrap_or(!self.config.layout.show_group);
                Ok(format!(
                    "Group panel {}.",
                    on_off(self.config.layout.show_group)
                ))
            }
            "social" => {
                self.config.layout.show_social =
                    requested.unwrap_or(!self.config.layout.show_social);
                Ok(format!(
                    "Social panel {}.",
                    on_off(self.config.layout.show_social)
                ))
            }
            _ => Err("usage: /toggle group|opponent|social [on|off]".to_string()),
        }
    }

    fn track_movement_command(&mut self, command: &str) {
        if self
            .state
            .has_authoritative_msdp_room(&self.config.msdp.mapping)
        {
            return;
        }
        if let Some(direction) = is_movement_command(command)
            && let Err(error) = self.state.map.move_direction(&direction)
        {
            self.state.push_output(error, OutputCategory::Error);
        }
    }

    async fn send_mud_text_command(
        &mut self,
        command: &str,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) {
        for command in self.state.map.mud_commands_for_movement(command) {
            self.state.path.record(&command);
            self.track_movement_command(&command);
            self.echo_mud_command(&command);
            let _ = command_tx.send(ClientCommand::SendText(command)).await;
        }
    }

    async fn send_mud_text_command_without_path_recording(
        &mut self,
        command: &str,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) {
        for command in self.state.map.mud_commands_for_movement(command) {
            self.track_movement_command(&command);
            self.echo_mud_command(&command);
            let _ = command_tx.send(ClientCommand::SendText(command)).await;
        }
    }

    fn push_full_output_map(&mut self) {
        let output = self.resolved_layout(self.last_terminal_area).output;
        let width = output.width.saturating_sub(2);
        let height = output.height.saturating_sub(2);
        if width == 0 || height == 0 {
            self.state.push_output(
                "MUD output pane is too small to display the map.",
                OutputCategory::Error,
            );
            return;
        }
        let lines = map_snapshot_lines(
            width,
            height,
            &self.state.map,
            &self.theme,
            &self.config.map,
        );
        self.state.push_output_snapshot(lines);
        self.state.follow_output();
    }

    fn echo_mud_command(&mut self, command: &str) {
        if self.config.terminal.echo_commands {
            self.state
                .push_output(command.to_string(), OutputCategory::System);
        }
    }

    async fn handle_network_event(
        &mut self,
        event: NetworkEvent,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) {
        match event {
            NetworkEvent::Connected => {
                self.state.connection = ConnectionStatus::Connected;
                self.state
                    .push_output_boundary("Connected to RoTS.", OutputCategory::System);
            }
            NetworkEvent::Disconnected => {
                self.state.connection = ConnectionStatus::Disconnected;
                self.mud_ansi_colors.reset();
                self.state
                    .clear_authoritative_msdp_room(&self.config.msdp.mapping);
                self.state
                    .push_output_boundary("Disconnected.", OutputCategory::System);
            }
            NetworkEvent::Text(text) => {
                for line in text.lines() {
                    self.push_mud_output_line(line, command_tx).await;
                }
            }
            NetworkEvent::Prompt(text) => {
                let colors = self.mud_ansi_colors.inspect(&text);
                let normalized = plain_text(&text);
                let style = self
                    .highlights
                    .style_for(&normalized, OutputCategory::Prompt);
                self.state.push_prompt_styled(text.clone(), style);
                self.run_trigger_actions_with_colors(
                    &text,
                    OutputCategory::Prompt,
                    &colors,
                    command_tx,
                )
                .await;
                self.mud_ansi_colors.reset();
            }
            NetworkEvent::Msdp(frames) => {
                self.state
                    .apply_msdp_frames(&frames, &self.config.msdp.mapping);
            }
            NetworkEvent::Error(message) => {
                self.state.connection = ConnectionStatus::Disconnected;
                self.mud_ansi_colors.reset();
                self.state.last_error = Some(message.clone());
                self.state.push_output(message, OutputCategory::Error);
            }
        }
    }

    async fn push_mud_output_line(&mut self, line: &str, command_tx: &mpsc::Sender<ClientCommand>) {
        let colors = self.mud_ansi_colors.inspect(line);
        let normalized = plain_text(line);
        self.capture_social_line(&normalized);
        let style = self
            .highlights
            .style_for(&normalized, OutputCategory::Normal);
        self.state
            .push_output_styled(line.to_string(), OutputCategory::Normal, style);
        self.run_trigger_actions_with_colors(line, OutputCategory::Normal, &colors, command_tx)
            .await;
        if is_casting_spinner_line(&normalized) {
            self.mud_ansi_colors.reset();
        }
    }

    async fn run_trigger_actions_with_colors(
        &mut self,
        line: &str,
        category: OutputCategory,
        colors: &AnsiColors,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) {
        let normalized = plain_text(line);
        let actions = self
            .triggers
            .evaluate_with_colors(&normalized, category, colors);
        let limit = self.triggers.max_commands_per_line();
        let mut budgets = vec![CommandBudget {
            remaining: limit,
            limit,
            source: "trigger execution",
        }];
        for event in actions.events {
            self.dispatch_script_event(
                ScriptEvent::parse(event.name),
                event.source,
                command_tx,
                &mut budgets,
            )
            .await;
        }
        for hook in actions.lua {
            let context = LuaHookContext {
                kind: "trigger".to_string(),
                line: Some(hook.line.clone()),
                raw_line: Some(line.to_string()),
                category: Some(hook.category),
                captures: hook.captures,
                colors: Some(hook.colors),
                ..LuaHookContext::default()
            };
            self.run_lua_hook(&hook.function, context, command_tx, &mut budgets)
                .await;
        }
        for command in actions.commands {
            if !self
                .handle_text_commands(&command, command_tx, &mut budgets)
                .await
            {
                break;
            }
        }
    }

    async fn dispatch_script_event(
        &mut self,
        event: ScriptEvent,
        source: impl Into<String>,
        command_tx: &mpsc::Sender<ClientCommand>,
        budgets: &mut Vec<CommandBudget>,
    ) {
        let dispatch = match self.events.dispatch(event, source) {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.state
                    .push_output(error.to_string(), OutputCategory::Error);
                return;
            }
        };
        let now = std::time::Instant::now();
        for record in &dispatch.records {
            self.animations.start_event_effect(&record.name, now);
        }
        self.state.script_events.extend(dispatch.records);
        let excess = self
            .state
            .script_events
            .len()
            .saturating_sub(self.config.events.history_limit);
        if excess > 0 {
            self.state.script_events.drain(..excess);
        }
        for notification in dispatch.notifications {
            self.state
                .push_output(notification, OutputCategory::Triggered);
        }
        let limit = self.config.events.max_commands_per_dispatch;
        budgets.push(CommandBudget {
            remaining: limit,
            limit,
            source: "event dispatch",
        });
        for hook in dispatch.lua {
            let context = LuaHookContext {
                kind: "event".to_string(),
                event: Some(hook.name),
                source: Some(hook.source),
                captures: hook.captures,
                ..LuaHookContext::default()
            };
            self.run_lua_hook(&hook.function, context, command_tx, budgets)
                .await;
        }
        for command in dispatch.commands {
            self.dispatching_handler_commands = true;
            let completed = self
                .handle_text_commands(&command, command_tx, budgets)
                .await;
            self.dispatching_handler_commands = false;
            if !completed {
                break;
            }
        }
        budgets.pop();
    }

    async fn handle_terminal_event(
        &mut self,
        event: TerminalEvent,
        command_tx: &mpsc::Sender<ClientCommand>,
    ) -> bool {
        match event {
            TerminalEvent::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Release {
                    return false;
                }
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && matches!(key.code, crossterm::event::KeyCode::Char('c' | 'C'))
                {
                    self.macros.printable_mode = false;
                }
                if let Some(action) = self
                    .macros
                    .action(key, self.state.output_view.search_active)
                {
                    let command = match action {
                        crate::macros::MacroAction::Execute(command) => command.to_string(),
                        crate::macros::MacroAction::SuppressRepeat => return false,
                    };
                    return self
                        .handle_command(ClientCommand::SendText(command), command_tx)
                        .await;
                }
                match handle_key(&mut self.state, key) {
                    InputAction::None => false,
                    InputAction::Command(command) => self.handle_command(command, command_tx).await,
                    InputAction::ClearOutput => {
                        self.state.clear_output();
                        self.mud_ansi_colors.reset();
                        false
                    }
                    InputAction::FollowOutput => {
                        self.state.follow_output();
                        false
                    }
                    InputAction::ScrollOutputDown(amount) => {
                        self.scroll_output_down(amount);
                        false
                    }
                    InputAction::ScrollOutputUp(amount) => {
                        self.scroll_output_up(amount);
                        false
                    }
                    InputAction::SearchNext => {
                        self.state.move_search_match(SearchDirection::Next);
                        false
                    }
                    InputAction::SearchPrevious => {
                        self.state.move_search_match(SearchDirection::Previous);
                        false
                    }
                    InputAction::ToggleOutputDisplayMode => {
                        self.state.toggle_output_display_mode();
                        false
                    }
                }
            }
            TerminalEvent::Mouse(mouse) => {
                if self.handle_mouse_event(mouse) {
                    self.send_mud_window_size(command_tx).await;
                }
                false
            }
            TerminalEvent::Resize { width, height } => {
                self.last_terminal_area = Rect::new(0, 0, width, height);
                self.send_mud_window_size(command_tx).await;
                false
            }
        }
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let layout = self.resolved_layout(self.last_terminal_area);
                self.active_divider = divider_at(mouse, &layout);
                if let Some(divider) = self.active_divider {
                    self.resize_pane_from_mouse(divider, mouse.column, mouse.row);
                    return true;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.active_divider.is_some() => {
                if let Some(divider) = self.active_divider {
                    self.resize_pane_from_mouse(divider, mouse.column, mouse.row);
                    return true;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.active_divider = None;
            }
            MouseEventKind::ScrollUp
                if self
                    .social_panel_area(&self.resolved_layout(self.last_terminal_area))
                    .is_some_and(|area| rect_contains(area, mouse.column, mouse.row)) =>
            {
                let layout = self.resolved_layout(self.last_terminal_area);
                if let Some(area) = self.social_panel_area(&layout) {
                    let max_offset =
                        social_scroll_max(&self.state, area.width, area.height, &self.theme);
                    self.state.social.scroll_up_to(3, max_offset);
                }
            }
            MouseEventKind::ScrollDown
                if self
                    .social_panel_area(&self.resolved_layout(self.last_terminal_area))
                    .is_some_and(|area| rect_contains(area, mouse.column, mouse.row)) =>
            {
                self.state.social.scroll_down(3);
            }
            MouseEventKind::ScrollUp
                if mouse_over_output(mouse, &self.resolved_layout(self.last_terminal_area)) =>
            {
                self.scroll_output_up(3);
            }
            MouseEventKind::ScrollDown
                if mouse_over_output(mouse, &self.resolved_layout(self.last_terminal_area)) =>
            {
                self.scroll_output_down(3);
            }
            _ => {}
        }
        false
    }

    fn resize_pane_from_mouse(&mut self, divider: Divider, column: u16, row: u16) {
        let layout = self.resolved_layout(self.last_terminal_area);
        if divider == Divider::MapOutput {
            let (UiMode::Mobile | UiMode::Tablet) = layout.mode else {
                return;
            };
            let available = self
                .last_terminal_area
                .height
                .saturating_sub(layout.input.height)
                .saturating_sub(layout.compact_status.map_or(0, |status| status.height));
            let maximum = available.saturating_sub(
                self.config
                    .panels
                    .output
                    .min_height
                    .min(available.saturating_sub(1)),
            );
            let minimum = self.config.panels.map.min_height.max(1);
            if maximum >= minimum {
                self.layout_overrides_mut(layout.mode).stacked_map_height = Some(
                    row.saturating_sub(self.last_terminal_area.y)
                        .saturating_add(1)
                        .clamp(minimum, maximum),
                );
            }
            return;
        }
        let opposite_width = match divider {
            Divider::Left => layout.right.map_or(0, |pane| pane.area.width),
            Divider::Right => layout.left.map_or(0, |pane| pane.area.width),
            Divider::MapOutput => return,
        };
        let overrides = match layout.mode {
            UiMode::FullHd => &mut self.full_hd_overrides,
            UiMode::Ultrawide => &mut self.ultrawide_overrides,
            UiMode::Mobile | UiMode::Tablet => return,
        };
        let max_width = self
            .last_terminal_area
            .width
            .saturating_sub(self.config.panels.output.min_width)
            .saturating_sub(opposite_width);
        let minimum_width = match divider {
            Divider::Left => layout.left.map_or(MIN_CENTER_WIDTH, |pane| {
                pane_min_width(pane.role, &self.config)
            }),
            Divider::Right => layout.right.map_or(MIN_CENTER_WIDTH, |pane| {
                pane_min_width(pane.role, &self.config)
            }),
            Divider::MapOutput => return,
        };
        if max_width < minimum_width {
            return;
        }
        match divider {
            Divider::Left => {
                overrides.right_width = Some(opposite_width);
                let width = column
                    .saturating_sub(self.last_terminal_area.x)
                    .saturating_add(1)
                    .clamp(minimum_width, max_width);
                overrides.left_width = Some(width);
            }
            Divider::Right => {
                overrides.left_width = Some(opposite_width);
                let width = self
                    .last_terminal_area
                    .right()
                    .saturating_sub(column)
                    .clamp(minimum_width, max_width);
                overrides.right_width = Some(width);
            }
            Divider::MapOutput => {}
        }
    }

    fn layout_overrides_mut(&mut self, mode: UiMode) -> &mut LayoutOverrides {
        match mode {
            UiMode::FullHd => &mut self.full_hd_overrides,
            UiMode::Ultrawide => &mut self.ultrawide_overrides,
            UiMode::Mobile | UiMode::Tablet => &mut self.stacked_overrides,
        }
    }

    fn capture_social_line(&mut self, line: &str) {
        let normalized = plain_text(line);
        if let Some(captured) = classify_social_line(&normalized) {
            self.state.social.push(
                captured.channel,
                Local::now().format("%H:%M").to_string(),
                captured.prefix,
                captured.text,
            );
        }
    }

    fn layout_overrides(&self, area: Rect) -> LayoutOverrides {
        match mode_for(area, &self.config.layout.breakpoints) {
            UiMode::FullHd => self.full_hd_overrides,
            UiMode::Ultrawide => self.ultrawide_overrides,
            UiMode::Mobile | UiMode::Tablet => self.stacked_overrides,
        }
    }

    fn resolved_layout(&self, area: Rect) -> ResolvedLayout {
        resolve_layout(
            area,
            &self.config.layout,
            layout_minimums(&self.config),
            self.layout_overrides(area),
        )
    }

    fn social_panel_area(&self, layout: &ResolvedLayout) -> Option<Rect> {
        let top = layout
            .top
            .filter(|pane| pane.role == PaneRole::StatusDashboard)?;
        status_dashboard_social_area(
            top.area,
            ResponsivePanelConfig::new(
                &self.config.gauges,
                &self.config.layout,
                &self.config.map,
                &self.config.panels,
                &self.config.weather,
                mode_for(self.last_terminal_area, &self.config.layout.breakpoints),
                false,
            ),
        )
    }

    fn scroll_output_up(&mut self, amount: usize) {
        self.scroll_output_rows(amount, true);
    }

    fn scroll_output_down(&mut self, amount: usize) {
        self.scroll_output_rows(amount, false);
    }

    fn scroll_output_rows(&mut self, amount: usize, up: bool) {
        let output = self.resolved_layout(self.last_terminal_area).output;
        let (offset, rows) = crate::ui::output::scroll_position(
            &self.state,
            output.height.saturating_sub(2) as usize,
            output.width.saturating_sub(2) as usize,
            &self.theme,
            amount,
            up,
        );
        self.state.output_view.scroll_offset = offset;
        self.state.output_view.wrapped_row_offset = rows;
        self.state.output_view.follow_newest = (offset, rows) == (0, 0);
    }

    #[cfg(test)]
    fn output_scroll_max(&self) -> usize {
        let output = self.resolved_layout(self.last_terminal_area).output;
        crate::ui::output::max_scroll_position(
            &self.state,
            output.height.saturating_sub(2) as usize,
            output.width.saturating_sub(2) as usize,
            &self.theme,
        )
        .0
    }

    fn mud_window_size(&self) -> Option<(u16, u16)> {
        let area = self.resolved_layout(self.last_terminal_area).output;
        (area.width > 0 && area.height > 0).then_some((
            area.width.saturating_sub(2).max(1),
            area.height.saturating_sub(2).max(1),
        ))
    }

    async fn send_mud_window_size(&self, command_tx: &mpsc::Sender<ClientCommand>) {
        if let Some((width, height)) = self.mud_window_size() {
            let _ = command_tx
                .send(ClientCommand::SetWindowSize { width, height })
                .await;
        }
    }

    fn render(&self, frame: &mut Frame) {
        render_with_overrides(
            frame,
            &self.config,
            &self.state,
            &self.theme,
            self.layout_overrides(frame.area()),
        );
    }
}

fn help_text(topic: &str) -> Option<&'static str> {
    match topic.trim().to_ascii_lowercase().as_str() {
        "macro" | "macros" => Some(include_str!("../docs/commands/macro.md")),
        "" | "commands" => Some(
            "# Mud Client Help\n\n## Local Commands\n- `/help [topic]` - show client help\n- `/clear` - clear output\n- `/quit` - quit the client\n- `/reload` - reload config from disk\n- `/reconnect` - request a network reconnect\n- `/msdp` - show stored MSDP values\n- `/lua [status|reload|call <function>]` - inspect and run Lua hooks\n- `/echo [--fg <color>] [--bg <color>] <text>` - write styled local output\n- `/variable` - list, set, or unset script variables\n- `/macro` - bind keys to commands; `/help macro` for details\n- `/alias` - list, add, unset, or clear runtime aliases\n- `/triggers` - list, add, unset, or clear runtime text/color triggers\n- `/highlight` - list, add, unset, or clear runtime highlights\n- `/handler` - list, add, unset, or clear runtime event handlers\n- `/event` - inspect or manually emit script events\n- `/toggle group|opponent|social [on|off]` - toggle optional panels for this session\n- `/map <command>` - mapper commands\n\n## Topics\n- `msdp` - stored MSDP values\n- `lua` - Lua scripting hooks and client API\n- `echo` - local styled output\n- `variable` - configured and runtime script variables\n- `map` - room mapping commands\n- `alias` - alias configuration\n- `trigger` - configured output reactions\n- `event` - script event dispatch and handlers\n- `highlight` - configured and runtime output styling\n- `animation` - animation timing and reduced motion\n- `diagnostics` - logging and troubleshooting\n- `toggle` - optional panel toggles\n- `social` - captured communication panel\n- `path` - path finding and path running\n- `output` - scrollback, search, triggers, and highlights\n- `input` - command input controls\n- `config` - runtime configuration notes",
        ),
        "msdp" => Some(
            "# MSDP Help\n\n## Usage\n- `/msdp`\n\n## Description\nShows every MSDP variable currently stored by the client. Values are sorted by variable name and reflect the latest MSDP frames received from the MUD.",
        ),
        "echo" => Some(
            "# Echo Help\n\n## Usage\n- `/echo <text>`\n- `/echo --fg <color> <text>`\n- `/echo --bg <color> <text>`\n- `/echo --fg=<color> --bg=<color> <text>`\n\n## Description\nWrites text to the MUD output pane without sending it to the server. Colors accept named terminal colors or `#RRGGBB`. Use `--` before text that begins with an option-like token.\n\n## Variables\nDirect input expands `${name}` before execution. Use `$${name}` to display a literal `${name}` reference.\n\n## Examples\n- `/echo Current Target: ${target}`\n- `/echo --fg yellow --bg #101010 Warning: ${target}`",
        ),
        "variable" | "variables" => Some(
            "# Variable Help\n\n## Commands\n- `/variable` - list all active variables\n- `/variable {name} {value}` - add or replace a runtime variable\n- `/variable unset {name}` - remove a runtime variable and reveal any configured value\n\n## Interpolation\nUse `${name}` in alias patterns/actions, trigger patterns/actions/events, and event handler patterns/actions. Use `$${name}` for literal `${name}` text. Variables expand before regular-expression compilation and before `{1}` capture substitution.\n\n## Configuration\n- `[variables]` controls `max_expansion_depth` and `max_expanded_bytes`\n- `[variables.values]` defines persistent variables\n\nRuntime variables override configured variables for the current session and survive `/reload`.",
        ),
        "lua" => Some(
            "# Lua Help\n\n## Commands\n- `/lua` or `/lua status` - show Lua status\n- `/lua reload` - reload the configured entrypoint\n- `/lua call <function>` - call a Lua function with manual context\n\n## Examples\n- `/lua status`\n- `/lua reload`\n- `/lua call smoke_test`\n\n## Config\n- `[lua] enabled` turns Lua hooks on or off\n- `script_dir` and `entrypoint` locate the startup script\n- `max_actions_per_hook` caps queued client actions\n- `runtime_errors_to_output` controls whether hook failures appear in output\n\n## Hooks\nAdd `lua = \"function_name\"` to an alias rule, trigger rule, or event handler. Lua hooks receive a context table and use the global `client` API to send commands, emit events, read MSDP, inspect output, update variables, run map commands, and toggle UI panels.\n\n## API\n`client.send`, `send_all`, `echo`, `notify`, `log.*`, `var.*`, `msdp.*`, `character.get`, `opponent.get`, `group.list`, `room.current`, `output.recent`, `output.search`, `event.emit`, `event.recent`, `map.*`, `ui.*`, and `time.now_ms` are available. Filesystem, OS, process, package, and debug globals are disabled.\n\n## Docs\nSee `docs/lua-api.md` and `docs/lua-scripting-examples.md`.",
        ),
        "map" | "mapping" => Some(
            "# Map Help\n\n## Display\n- `/map map` - show a full MUD-output-pane map centered on the current room\n- `/map get` / `/map info` - show current room details\n- `/map list [query]` - list mapped rooms\n\n## Rooms and Links\n- `/map create` - reset and create a map\n- `/map goto <vnum|name> [dig]` - set current map room\n- `/map move <direction>` - move only the mapper\n- `/map dig <direction> [new|vnum]` - create and link a room\n- `/map link <direction> <vnum> [both]` - link rooms\n- `/map unlink <direction>` - remove an exit link\n- `/map delete <direction|vnum>` - delete an exit or room\n- `/map undo` - undo the last mapper-created move\n- `/map return` - restore the previous mapper room\n- `/map leave` - leave the map while remembering the previous room\n\n## Metadata and Flags\n- `/map set <option> <value>` - set room metadata\n- `/map flag <name> [on|off]` - toggle mapper-wide flags\n- `/map roomflag [flag[;flag...] [on|off|get <variable>]]` - list or update current-room flags\n- `/map exitflag <direction> <flag> [on|off]` - toggle an exit flag\n- `/map door <direction> [state|none] [name]` - mark or clear an exit door\n\n## Paths and Files\n- `/map find <vnum|name>` - show optimized weighted path\n- `/map run <vnum|name>` - send each movement command in the optimized weighted path\n- `/map read <file>` - load map TOML\n- `/map write <file>` - save map TOML\n\nUse `/help map <command>` for focused help, such as `/help map set`, `/help map flag`, or `/help map door`.",
        ),
        "map map" => Some(
            "# /map map\n\n## Usage\n- `/map map`\n\n## Description\nRenders a map snapshot centered on the current room into every row and column of the MUD output content area. The snapshot is appended to scrollback and follows the newest output.",
        ),
        "map create" => Some(
            "# /map create\n\n## Usage\n- `/map create`\n\n## Description\nCreates a fresh map with room `1` as the current room. Existing in-memory map data is cleared.\n\n## Notes\nUse this when starting a new mapping session.",
        ),
        "map goto" => Some(
            "# /map goto\n\n## Usage\n- `/map goto <vnum|name> [dig]`\n\n## Description\nSets the mapper's current room to an existing room by vnum or name.\n\n## Notes\nAdd `dig` to create the target room if it does not already exist.",
        ),
        "map move" => Some(
            "# /map move\n\n## Usage\n- `/map move <direction>`\n\n## Description\nMoves only the local mapper in the given direction. This does not send movement to the MUD.\n\n## Notes\nNormal MUD movement commands are tracked separately when MSDP room data is not authoritative.",
        ),
        "map dig" => Some(
            "# /map dig\n\n## Usage\n- `/map dig <direction> [new|vnum]`\n\n## Description\nCreates or links a room from the current room in the given direction.\n\n## Notes\nUse `new` or omit the target to create the next generated room id.",
        ),
        "map link" => Some(
            "# /map link\n\n## Usage\n- `/map link <direction> <vnum> [both]`\n\n## Description\nLinks the current room's exit in `direction` to an existing or newly created target room.\n\n## Notes\nAdd `both` to also create the reverse link when the direction has a known reverse.",
        ),
        "map unlink" => Some(
            "# /map unlink\n\n## Usage\n- `/map unlink <direction>`\n\n## Description\nRemoves the current room's mapped exit for the given direction.",
        ),
        "map delete" => Some(
            "# /map delete\n\n## Usage\n- `/map delete <direction|vnum>`\n\n## Description\nDeletes an exit from the current room when given a direction, or deletes a room when given a room id.\n\n## Notes\nDeleting a room also removes exits from other rooms that targeted it.",
        ),
        "map undo" => Some(
            "# /map undo\n\n## Usage\n- `/map undo`\n\n## Description\nReturns to the previous mapper room and removes the last mapper-created room when applicable.",
        ),
        "map return" => Some(
            "# /map return\n\n## Usage\n- `/map return`\n\n## Description\nReturns the mapper to the previous room tracked by `goto`, movement, or `leave`.",
        ),
        "map leave" => Some(
            "# /map leave\n\n## Usage\n- `/map leave`\n\n## Description\nTemporarily leaves the map while remembering the current room for `/map return`.",
        ),
        "map set" => Some(
            "# /map set\n\n## Usage\n- `/map set <option> <value>`\n\n## Description\nUpdates metadata on the current room.\n\n## Options\n- `roomname` / `name` - room display name\n- `roomdesc` / `description` - room description\n- `roomarea` / `area` - area name\n- `roomnote` / `note` - mapper note\n- `roomterrain` / `terrain` - terrain type\n- `roomsymbol` / `symbol` - custom room symbol\n- `roomweight` / `weight` - pathing weight",
        ),
        "map get" | "map info" => Some(
            "# /map get\n\n## Usage\n- `/map get`\n- `/map info`\n\n## Description\nShows the current map room id, name, coordinates, area, terrain, weight, and mapped exits.",
        ),
        "map list" => Some(
            "# /map list\n\n## Usage\n- `/map list [query]`\n\n## Description\nLists mapped rooms, optionally filtered by room id, name, area, description, note, or terrain.",
        ),
        "map find" => Some(
            "# /map find\n\n## Usage\n- `/map find <vnum|name>`\n\n## Description\nFinds and displays the shortest known path from the current room to a target room.",
        ),
        "map run" => Some(
            "# /map run\n\n## Usage\n- `/map run <vnum|name>`\n\n## Description\nFinds the lowest-cost known path using room weights and sends each movement command to the MUD.",
        ),
        "map landmark" | "map landmarks" => Some(
            "# /map landmark\n\n## Usage\n- `/map landmark [query]`\n- `/map landmarks [query]`\n- `/map landmark <name> <vnum> [description] [size]`\n\n## Description\nLists or updates persisted TinTin++-style landmarks. Landmark names can be used with `/map goto`, `/map find`, and `/map run`.\n\nUse `/map unlandmark <name-or-pattern>` to remove landmarks.",
        ),
        "map unlandmark" => Some(
            "# /map unlandmark\n\n## Usage\n- `/map unlandmark <name-or-pattern>`\n\n## Description\nRemoves landmarks matching an exact name or `*`/`?` wildcard pattern.",
        ),
        "map flag" => Some(
            "# /map flag\n\n## Usage\n- `/map flag <name> [on|off]`\n\n## Description\nToggles mapper-wide behavior.\n\n## Flags\n- `static` - prevent automatic room creation\n- `nofollow` - stop movement commands from moving the mapper\n- `direction` - toggle direction arrow behavior\n- `unicode` - toggle unicode map rendering\n- `asciigraphics` - disable unicode map graphics\n- `asciivnums` - toggle vnum display",
        ),
        "map roomflag" => Some(
            "# /map roomflag\n\n## Usage\n- `/map roomflag`\n- `/map roomflag <flag>[;<flag>...] [on|off]`\n- `/map roomflag <flag> get <variable>`\n\n## Description\nLists or changes TinTin-style flags on the current room.\n\n## Flags\n`avoid`, `block`, `curved`, `fog`, `hide`, `invis`, `leave`, `noglobal`, `static`, `void`.\n\n## Examples\n- `/map roomflag`\n- `/map roomflag avoid on`\n- `/map roomflag avoid;fog off`\n- `/map roomflag block get is_blocked`",
        ),
        "map exitflag" => Some(
            "# /map exitflag\n\n## Usage\n- `/map exitflag <direction> <flag> [on|off]`\n\n## Description\nToggles a flag on an exit from the current room.\n\n## Flags\n`avoid`, `block`, `hide`, `invis`, `teleport`.",
        ),
        "map door" => Some(
            "# /map door\n\n## Usage\n- `/map door <direction> [state|none] [name]`\n\n## Description\nMarks a door state and optional directional door name on an exit from the current room. If the state is omitted, the door is marked `closed`. Door names are stored only on the current room's exit because the opposite side can use a different name. Door markers render centered on the map link and can appear anywhere in the world. Every state uses the same configured glyph, with color indicating the door state.\n\n## States\n- `trigger` - door uses a manual trigger; no automatic movement command\n- `unknown` - details are unknown; no automatic movement command\n- `open` - known open door; no automatic movement command\n- `closed` - sends `open <name> <direction>` before movement\n- `pickable` - sends `pick <name> <direction>` before movement\n- `locked` - sends `unlock <name> <direction>` and `open <name> <direction>` before movement\n- `none` - clears door state and name",
        ),
        "map read" => Some(
            "# /map read\n\n## Usage\n- `/map read <file>`\n\n## Description\nLoads map state from a TOML map file.",
        ),
        "map write" => Some(
            "# /map write\n\n## Usage\n- `/map write <file>`\n\n## Description\nWrites the current map state to a TOML map file, creating parent directories when needed.",
        ),
        "path" | "paths" | "pathing" => Some(
            "# Path Help\n\n## Commands\n- `/path create|destroy|start|stop` - manage movement recording\n- `/path insert <forward> [backward]` - add a path step\n- `/path delete` / `/path undo` - remove the last step\n- `/path describe` - show path length, position, and mapping state\n- `/path get <length|position>` - return path metadata\n- `/path goto <start|end|position>` - select a path position\n- `/path move [forward|backward] [number]` - move the path position without sending a command\n- `/path walk [forward|backward]` - send one path step\n- `/path run` - send the remaining path steps\n- `/path swap` - reverse the path and its directions\n- `/path zip` / `/path unzip <speedwalk>` - convert direction steps\n- `/path map` - show the current map\n- `/path save <forward|backward|both> <variable>` - save steps to a runtime variable\n- `/path load <variable>` - load steps from a runtime variable\n\n`/map find <vnum|name>` and `/map run <vnum|name>` remain the weighted destination-routing commands.",
        ),
        "alias" | "aliases" => Some(
            "# Alias Help\n\n## Commands\n- `/alias` - list aliases currently loaded in memory\n- `/alias {pattern} {command} [{command}...]` - add or replace an in-memory alias\n- `/alias unset {pattern}` - remove one runtime alias\n- `/alias clear` - remove all runtime aliases\n\n## Parameters\nUse `{1}` in an alias command to pass the text after the alias pattern.\n\n## Examples\n- `/alias {k} {kill {1}}`\n- `/alias {rr} {recall} {look}`\n- `/alias unset {k}`\n- `/alias clear`\n\nAliases from `config.toml` are loaded at startup. Runtime alias commands only remove session aliases; remove configured aliases from `config.toml` and run `/reload`.",
        ),
        "alias lua" | "aliases lua" => Some(
            "# Alias Lua Help\n\nAdd `lua = \"function_name\"` to an alias rule. The hook receives `ctx.kind`, `ctx.input`, and `ctx.captures`. Use `client.send`, `client.echo`, `client.var.*`, or any other safe client API from the hook. Alias `commands` and Lua hooks are additive.\n\n## Example\nConfig: `pattern = \"^sk\\\\s+(.+)$\"`, `lua = \"smart_kill\"`\nLua: `client.send(\"kill \" .. ctx.captures[1])`",
        ),
        "trigger" | "triggers" => Some(
            "# Trigger Help\n\n## Commands\n- `/triggers` - list triggers currently loaded in memory\n- `/triggers [plain|regex] [--fg color] [--bg color] {pattern} {command} [{command}...]` - add or replace an in-memory trigger\n- `/triggers unset {pattern}` - remove one runtime trigger\n- `/triggers clear` - remove all runtime triggers\n\n`/trigger` remains available as a singular alias.\n\n## Runtime Matching\nA runtime trigger uses plain substring matching by default. If any command contains a capture such as `{1}`, the pattern is inferred as a regular expression. Use `plain` or `regex` to select matching explicitly. Optional color filters require the named, `index:N`, or `#RRGGBB` ANSI color anywhere in the MUD line. Trigger actions run through the same local-command, alias, movement, and echo pipeline as typed commands.\n\n## Examples\n- `/triggers plain {You are hungry} {eat bread}`\n- `/triggers regex {^(.+) arrives\\.$} {target {1}} {consider {1}}`\n- `/triggers plain --fg lightred --bg index:17 {Danger} {flee}`\n- `/triggers unset {You are hungry}`\n- `/triggers clear`\n\n## Config\n- `[triggers]` controls configured triggers; explicitly added runtime triggers remain active\n- `max_commands_per_line` limits runaway trigger output\n- `[[triggers.rules]]` defines persistent triggers with optional `foreground` and `background`\n\nRuntime trigger removal only affects session triggers. Remove configured triggers from `config.toml` and run `/reload`. Configured trigger rules additionally support events, cooldowns, one-shot behavior, priorities, and category filters.",
        ),
        "trigger lua" | "triggers lua" => Some(
            "# Trigger Lua Help\n\nAdd `lua = \"function_name\"` to a trigger rule. The hook receives `ctx.kind`, `ctx.line`, `ctx.raw_line`, `ctx.category`, `ctx.captures`, and `ctx.colors.foregrounds/backgrounds`. Trigger commands, emitted events, and Lua hooks all count toward trigger action limits.\n\n## Example\nConfig: `pattern = \"^(.+) arrives\\\\.$\"`, `lua = \"enemy_arrives\"`\nLua: `client.event.emit(\"CombatStarted:\" .. ctx.captures[1], \"lua\")`",
        ),
        "event" | "events" => Some(
            "# Event Help\n\n## Commands\n- `/event` - show a handler summary, dispatch status, and recent event history\n- `/event {name}` - manually emit an event\n- `/handler` - show complete configured and runtime handler definitions\n- `/handler [plain|regex] {event} {command} [{command}...]` - add or replace a session handler\n- `/handler unset {event}` - remove one runtime handler\n- `/handler clear` - remove all runtime handlers\n\n## Dispatch\nEvents are matched against enabled configured and runtime handlers in priority order. Handlers can send commands, display notifications, and emit follow-up events. Commands use the same local-command, alias, movement, echo, and network pipeline as typed input. Runtime handlers survive `/reload` but are not written to configuration. The global `[events] enabled` setting still controls runtime handlers.\n\n## Configuration\n- `[events]` controls dispatch limits and retained history\n- `match_type` is `plain` for an exact event name or `regex` for capture matching\n- `event` is the event name or regex pattern\n- `commands` contains command actions\n- `emit` contains follow-up event templates\n- `notification` writes a Triggered-category output line\n\n## Examples\n- `/event {LowHealth}`\n- `/event {EnemyEntered:Orc}`\n- `/handler {LowHealth} {flee}`\n- `/handler regex {^EnemyEntered:(.+)$} {target {1}}`\n- `/handler unset {LowHealth}`\n- `/handler clear`\n\nRuntime handler removal only affects session handlers. Remove configured handlers from `config.toml` and run `/reload`. Bounded cascade depth, event count, and command count prevent recursive handlers from flooding the client.",
        ),
        "event lua" | "events lua" | "handler lua" => Some(
            "# Event Lua Help\n\nAdd `lua = \"function_name\"` to an event handler. The hook receives `ctx.kind`, `ctx.event`, `ctx.source`, and `ctx.captures`. Lua hooks run inside the existing event dispatch budget and can use `client.event.emit` for follow-up events.\n\n## Example\nConfig: `event = \"LowHealth\"`, `lua = \"low_health\"`\nLua: `client.send(\"flee\")`",
        ),
        "highlight" => Some(
            "# Highlight Help\n\n## Commands\n- `/highlight` - list configured and runtime highlights\n- `/highlight [plain|regex] {pattern} {foreground|none} [background|none] [styles]` - add or replace a runtime highlight\n- `/highlight unset {pattern}` - remove one runtime highlight\n- `/highlight clear` - remove all runtime highlights\n\nStyles are comma- or space-separated values chosen from `bold`, `dim`, `italic`, `underline`, and `reverse`.\n\n## Examples\n- `/highlight {You are hit} {red}`\n- `/highlight regex {You receive \\\\d+ gold} {yellow} {none} {bold}`\n- `/highlight plain {IMPORTANT} {white} {red} {bold underline}`\n- `/highlight unset {You are hit}`\n- `/highlight clear`\n\n## Config\n- `[highlights]` controls whether configured highlights are enabled\n- `[[highlights.rules]]` defines persistent highlights\n\n## Rule Fields\n- `name` - unique highlight name\n- `match_type` - `plain` or `regex`\n- `pattern` - text or regular expression to match\n- `foreground` / `background` - named color or hex color\n- `bold`, `dim`, `italic`, `underline`, `reverse` - style toggles\n- `categories` - optional output category filter\n\nRuntime highlight removal only affects session highlights. Remove configured highlights from `config.toml` and run `/reload`.",
        ),
        "animation" | "animations" => Some(
            "# Animation Help\n\n## Description\nAnimations are driven by a shared scheduler that uses elapsed time instead of render-count assumptions.\n\n## Config\n- `animation.enabled` - master animation flag for future effects\n- `animation.reduced_motion` - skip event-triggered motion effects\n- `animation.low_performance` - use cheaper event-effect defaults\n- `animation.map_fps` - map animation frame rate\n- `animation.weather_fps` - weather animation frame rate\n- `weather.show_info_marker` - show compact animated weather markers in the info pane",
        ),
        "toggle" | "toggles" => Some(
            "# Toggle Help\n\n## Commands\n- `/toggle` - show optional panel states\n- `/toggle opponent [on|off]` - toggle the Opponent panel for this session\n- `/toggle group [on|off]` - toggle the Group panel for this session\n- `/toggle social [on|off]` - toggle the Social panel for this session\n\n## Notes\nThese changes are in-memory only. Use `layout.show_opponent`, `layout.show_group`, and `layout.show_social` in config for startup defaults.",
        ),
        "social" => Some(
            "# Social Help\n\n## Description\nThe ultrawide Social panel copies RoTS tells, chats, says, narrates, group-says, yells, and sings out of the main output. It uses local machine `HH:MM` timestamps, extracts the text inside RoTS single quotes, wraps long messages, and scrolls independently with the mouse wheel over the panel.\n\n## Format\n- `[HH:MM](tell) from Name - Text`\n- `[HH:MM](tell) to Name - Text`\n- `[HH:MM](say) Name - Text`\n- `[HH:MM](say) Text` for your own message\n\nYour own chat, narrate, sing, yell, say, and group-say messages omit the name prefix.",
        ),
        "output" | "scrollback" | "search" => Some(
            "# Output Help\n\n## Scrollback\n- Mouse wheel over MUD output - scroll output\n- Drag a large-layout side divider - resize that pane\n- Drag the Map/MUD Output boundary in mobile or tablet layouts - resize their heights\n- `PageUp` / `PageDown` - scroll output\n- `Ctrl-Up` / `Ctrl-Down` - scroll one line\n- `Ctrl-E` - follow newest output\n- `/clear` - clear output\n\n## Search and Modes\n- `Ctrl-F` - search output\n- `Ctrl-N` / `Ctrl-P` - next or previous search match\n- `F2` - cycle styled, plain, and debug views; the mode indicator appears briefly in the output title\n\n## Automation\n- `/help trigger` - configured output reactions\n- `/help highlight` - configured output styling",
        ),
        "input" | "keys" => Some(
            "# Input Help\n\n## Command Editing\n- `Enter` - send command, or send a blank line when input is empty\n- `;` - separate multiple MUD commands in one input\n- `#<count> {command}` - repeat one command or braced command group\n- `Enter` on highlighted last command - resend it\n- Typing while last command is highlighted - replace it\n- `Tab` - complete the current word from recent MUD output\n- `Shift-Tab` - cycle to the previous completion\n- `Up` / `Down` - command history; typed text filters history by prefix\n- `Left` / `Right` / `Home` / `End` - edit input\n- `Ctrl-C` - clear the input line; use `/quit` to exit\n\n## Examples\n- `#10 {kill orc}` sends `kill orc` ten times\n- `#2 {look;score};rest` sends `look`, `score`, `look`, `score`, then `rest` once",
        ),
        "config" => Some(
            "# Config Help\n\nRuntime config is loaded from the platform config path when present.\n\n## Commands\n- `/reload` - reload config from disk and report validation errors in the output pane\n- `/reconnect` - request a network reconnect\n\n## Notes\n- The repository `config.toml` is the parse-tested default example.\n- The default endpoint is `rotsmud.org:3791`.\n- `--local` forces `localhost:3791` even when config points elsewhere.\n- `layout.breakpoints` selects display profiles from terminal-cell dimensions.\n- `layout.mobile` and `layout.tablet` configure stacked map/status behavior.\n- `layout.full_hd` configures the classic sidebar position and width.\n- `layout.ultrawide` configures top, left, and right pane roles and dimensions.\n- Map, MUD output, and command input are required in every display profile.\n- `map.persistence` can load a map file at startup and save it on graceful exit.\n- `panels.*` controls optional panel title, enabled state, minimum size, priority, and responsive visibility.\n- `social.scrollback_lines` controls retained Social panel messages.\n- `variables`, `aliases`, `triggers`, `events`, and `highlights` are loaded from config at startup and reload.\n- `logging.level` controls tracing filters; `logging.raw_protocol` is reserved for protocol diagnostics and should stay off unless debugging.\n- `layout.show_group`, `layout.show_opponent`, and `layout.show_social` control optional panels at startup.\n- `/toggle group|opponent|social [on|off]` changes those optional panels in memory for the current session.",
        ),
        "diagnostic" | "diagnostics" | "logging" => Some(
            "# Diagnostics Help\n\n## Commands\n- `/msdp` - inspect stored MSDP values\n- `/reload` - reload config and display validation errors\n- `/reconnect` - request a network reconnect\n\n## Config\n- `logging.level` - tracing filter, for example `mud_client=debug`\n- `logging.raw_protocol` - reserved raw protocol diagnostics flag\n\n## Notes\nRaw protocol logging can be noisy and may expose game text. Leave it disabled unless actively troubleshooting.",
        ),
        _ => None,
    }
}

fn msdp_snapshot(state: &AppState) -> String {
    if state.raw_msdp.is_empty() {
        return "# MSDP Values\n\nNo MSDP values have been received yet.".to_string();
    }

    let mut variables = state.raw_msdp.iter().collect::<Vec<_>>();
    variables.sort_by_key(|(variable, _)| *variable);

    let mut message = "# MSDP Values\n\n## Stored Values".to_string();
    for (variable, value) in variables {
        let _ = write!(
            message,
            "\n- `{}`: {}",
            markdown_inline(variable),
            format_msdp_value(value)
        );
    }
    message
}

fn variable_listing(variables: &VariableStore) -> std::result::Result<String, String> {
    let entries = variables.entries().map_err(|error| error.to_string())?;
    if entries.is_empty() {
        return Ok("# Variables\n\nNo variables are defined.".to_string());
    }
    let mut message = "# Variables\n\n## Active Variables".to_string();
    for entry in entries {
        let source = match entry.source {
            VariableSource::Config => "config",
            VariableSource::Runtime => "runtime",
        };
        let _ = write!(
            message,
            "\n- `{}` ({source}): `{}`",
            markdown_inline(&entry.name),
            markdown_inline(&entry.value)
        );
    }
    Ok(message)
}

fn alias_listing(aliases: &AliasEngine) -> String {
    let configs = aliases.alias_configs();
    if configs.is_empty() {
        return "# Aliases\n\nNo aliases are loaded.".to_string();
    }
    let mut message = "# Aliases\n\n## Loaded Aliases".to_string();
    for config in configs {
        let commands = config
            .commands
            .iter()
            .map(|command| format!("`{}`", markdown_inline(command)))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            message,
            "\n- `{}` ({:?}) -> {}",
            markdown_inline(&config.pattern),
            config.match_type,
            commands
        );
    }
    message
}

fn trigger_listing(triggers: &TriggerEngine) -> String {
    let entries = triggers.trigger_entries();
    if entries.is_empty() {
        return "# Triggers\n\nNo triggers are loaded.".to_string();
    }
    let mut message = "# Triggers\n\n## Loaded Triggers".to_string();
    for (config, runtime) in entries {
        let commands = if config.commands.is_empty() {
            "no commands".to_string()
        } else {
            config
                .commands
                .iter()
                .map(|command| format!("`{}`", markdown_inline(command)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = write!(
            message,
            "\n- `{}` ({}, {:?}, {}, fg={}, bg={}) -> {}",
            markdown_inline(&config.pattern),
            if runtime { "runtime" } else { "config" },
            config.match_type,
            if config.enabled {
                "enabled"
            } else {
                "disabled"
            },
            config.foreground.as_deref().unwrap_or("any"),
            config.background.as_deref().unwrap_or("any"),
            commands
        );
    }
    message
}

fn parse_trigger_color_options(
    mut input: &str,
) -> std::result::Result<(&str, Option<String>, Option<String>), String> {
    let mut foreground = None;
    let mut background = None;
    while let Some((option, remainder)) = split_first_token(input) {
        let target = match option {
            "--fg" => &mut foreground,
            "--bg" => &mut background,
            _ => break,
        };
        let Some((value, remainder)) = split_first_token(remainder) else {
            return Err(format!("{option} requires a color\n{}", trigger_usage()));
        };
        if parse_ansi_color(value).is_none() {
            return Err(format!(
                "invalid trigger color `{value}`; use a named color, index:N, or #RRGGBB"
            ));
        }
        *target = Some(value.to_string());
        input = remainder.trim_start();
    }
    Ok((input, foreground, background))
}

fn trigger_usage() -> String {
    "usage: /triggers [plain|regex] [--fg color] [--bg color] {pattern} {command} [{command}...] | /triggers unset {pattern} | /triggers clear"
        .to_string()
}

fn event_listing(events: &EventEngine, state: &AppState) -> String {
    let handlers = events.handler_entries();
    let mut message = format!(
        "# Events\n\nDispatch is {}.",
        if events.is_enabled() {
            "enabled"
        } else {
            "disabled"
        }
    );
    if handlers.is_empty() {
        message.push_str("\n\nNo event handlers are loaded.");
    } else {
        message.push_str("\n\n## Handlers");
        for (handler, runtime) in handlers {
            let _ = write!(
                message,
                "\n- `{}` ({}, {:?}, {}) matches `{}`",
                markdown_inline(&handler.name),
                if runtime { "runtime" } else { "config" },
                handler.match_type,
                if handler.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                markdown_inline(&handler.event)
            );
        }
    }
    if state.script_events.is_empty() {
        message.push_str("\n\n## Recent Events\n\nNo events have been dispatched.");
    } else {
        message.push_str("\n\n## Recent Events");
        for event in state.script_events.iter().rev().take(10) {
            let _ = write!(
                message,
                "\n- `{}` from `{}`",
                markdown_inline(&event.name),
                markdown_inline(&event.source)
            );
        }
    }
    message
}

fn handler_listing(events: &EventEngine) -> String {
    let entries = events.handler_entries();
    if entries.is_empty() {
        return "# Event Handlers\n\nNo event handlers are loaded.".to_string();
    }
    let mut message = "# Event Handlers\n\n## Loaded Handlers".to_string();
    for (handler, runtime) in entries {
        let commands = markdown_values(&handler.commands);
        let emitted = markdown_values(&handler.emit);
        let notification = handler
            .notification
            .as_deref()
            .map(markdown_inline)
            .map_or_else(|| "none".to_string(), |value| format!("`{value}`"));
        let _ = write!(
            message,
            "\n\n### `{}`\n- Source: {}\n- Match: {:?} `{}`\n- State: {}\n- Priority: {}\n- Commands: {}\n- Notification: {}\n- Emits: {}",
            markdown_inline(&handler.name),
            if runtime { "runtime" } else { "config" },
            handler.match_type,
            markdown_inline(&handler.event),
            if handler.enabled {
                "enabled"
            } else {
                "disabled"
            },
            handler.priority,
            commands,
            notification,
            emitted
        );
    }
    message
}

fn markdown_values(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|value| format!("`{}`", markdown_inline(value)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn handler_usage() -> String {
    "usage: /handler [plain|regex] {event} {command} [{command}...] | /handler unset {event} | /handler clear".to_string()
}

fn highlight_listing(highlights: &HighlightEngine) -> String {
    let entries = highlights.highlight_entries();
    if entries.is_empty() {
        return "# Highlights\n\nNo highlights are loaded.".to_string();
    }
    let mut message = "# Highlights\n\n## Loaded Highlights".to_string();
    for (config, runtime) in entries {
        let foreground = config.foreground.as_deref().unwrap_or("none");
        let background = config.background.as_deref().unwrap_or("none");
        let mut styles = Vec::new();
        if config.bold {
            styles.push("bold");
        }
        if config.dim {
            styles.push("dim");
        }
        if config.italic {
            styles.push("italic");
        }
        if config.underline {
            styles.push("underline");
        }
        if config.reverse {
            styles.push("reverse");
        }
        let styles = if styles.is_empty() {
            "none".to_string()
        } else {
            styles.join(",")
        };
        let _ = write!(
            message,
            "\n- `{}` / `{}` ({}, {:?}, {}, priority {}) fg=`{}`, bg=`{}`, styles=`{}`, categories=`{:?}`",
            markdown_inline(&config.name),
            markdown_inline(&config.pattern),
            if runtime { "runtime" } else { "config" },
            config.match_type,
            if config.enabled {
                "enabled"
            } else {
                "disabled"
            },
            config.priority,
            markdown_inline(foreground),
            markdown_inline(background),
            styles,
            config.categories
        );
    }
    message
}

fn parse_optional_highlight_color(
    value: &str,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if parse_color(value).is_none() {
        return Err(format!(
            "invalid highlight {field} color `{value}`; use a named color, #RRGGBB, or none"
        ));
    }
    Ok(Some(value.to_string()))
}

fn apply_highlight_styles(
    config: &mut HighlightRuleConfig,
    styles: &str,
) -> std::result::Result<(), String> {
    for style in styles
        .split([',', ' '])
        .map(str::trim)
        .filter(|style| !style.is_empty())
    {
        match style.to_ascii_lowercase().as_str() {
            "bold" => config.bold = true,
            "dim" => config.dim = true,
            "italic" => config.italic = true,
            "underline" => config.underline = true,
            "reverse" => config.reverse = true,
            _ => {
                return Err(format!(
                    "unknown highlight style `{style}`; use bold, dim, italic, underline, or reverse"
                ));
            }
        }
    }
    Ok(())
}

fn highlights_usage() -> String {
    "usage: /highlight [plain|regex] {pattern} {foreground|none} [background|none] [styles] | /highlight unset {pattern} | /highlight clear"
        .to_string()
}

fn parse_braced_fields(
    input: &str,
    command_name: &str,
) -> std::result::Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((index, value)) = chars.next() {
        if value.is_whitespace() {
            continue;
        }
        if value != '{' {
            return Err(format!(
                "{command_name} parameters must use braces near `{}`",
                &input[index..]
            ));
        }
        let start = index + value.len_utf8();
        let mut depth = 1usize;
        let mut end = None;
        let mut escaped = false;
        for (inner_index, inner_value) in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            match inner_value {
                '\\' => escaped = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = Some(inner_index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return Err(format!("{command_name} parameter is missing closing `}}`"));
        };
        fields.push(input[start..end].replace("\\\\", "\\"));
    }
    Ok(fields)
}

fn strip_subcommand<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let remainder = input.strip_prefix(name)?;
    remainder
        .chars()
        .next()
        .filter(|character| character.is_whitespace())?;
    Some(remainder.trim_start())
}

fn lua_output_style(options: Option<LuaOutputOptions>) -> Option<OutputStyle> {
    options.map(|options| OutputStyle {
        foreground: options.foreground,
        background: options.background,
        ..OutputStyle::default()
    })
}

fn parse_toggle_value(value: &str) -> std::result::Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err("toggle value must be on or off".to_string()),
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    use tokio::sync::mpsc;

    use crate::{
        config::{
            AliasMatchType, AliasRuleConfig, EventHandlerConfig, HighlightRuleConfig,
            TriggerRuleConfig,
        },
        network::msdp::MsdpValue,
        state::SocialChannel,
    };

    use super::*;

    #[test]
    fn every_display_profile_renders_map_output_and_command() {
        let config = AppConfig::default();
        let state = AppState::new(&config);
        let theme = Theme::from_config(&config.colors);

        for (width, height) in [(70, 23), (100, 29), (160, 40), (220, 50)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
            terminal
                .draw(|frame| render(frame, &config, &state, &theme))
                .expect("profile should render");

            let buffer = terminal.backend().buffer();
            let rendered = (0..height)
                .map(|y| {
                    (0..width)
                        .filter_map(|x| buffer.cell((x, y)))
                        .map(|cell| cell.symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rendered.contains("Map"), "{width}x{height} omitted Map");
            assert!(
                rendered.contains("MUD Output"),
                "{width}x{height} omitted MUD Output"
            );
            assert!(
                rendered.contains("Command"),
                "{width}x{height} omitted Command"
            );
        }
    }

    #[tokio::test]
    async fn resize_sends_mud_output_pane_as_window_size() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Resize {
                width: 160,
                height: 32,
            },
            &tx,
        )
        .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SetWindowSize {
                width: 124,
                height: 27,
            })
        );
    }

    #[tokio::test]
    async fn send_text_commands_expand_aliases_before_network_send() {
        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "recall".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "rr".to_string(),
            commands: vec!["recall".to_string(), "look".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("rr".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("recall".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "recall")
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "look")
        );
    }

    #[tokio::test]
    async fn send_text_commands_echo_before_network_send() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("look".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            app.state.output.back().map(|line| line.normalized.as_str()),
            Some("look")
        );
        assert_eq!(
            app.state.output.back().map(|line| &line.category),
            Some(&OutputCategory::System)
        );
    }

    #[tokio::test]
    async fn send_text_commands_do_not_echo_when_disabled() {
        let mut config = AppConfig::default();
        config.terminal.echo_commands = false;
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("look".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert!(app.state.output.is_empty());
    }

    #[tokio::test]
    async fn blank_text_command_sends_line_ending_without_echo() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText(String::new()), &tx)
            .await;

        assert!(!should_quit);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText(String::new()))
        );
        assert!(app.state.output.is_empty());
    }

    #[tokio::test]
    async fn echo_command_can_seed_social_capture_for_testing() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/echo You narrate 'testing social'".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        let message = app
            .state
            .social
            .messages
            .back()
            .expect("echoed social line should be captured");
        assert_eq!(message.channel, SocialChannel::Narrates);
        assert_eq!(message.prefix, "");
        assert_eq!(message.text, "testing social");
    }

    #[tokio::test]
    async fn network_text_captures_timestamped_social_messages() {
        let mut app = App::new(AppConfig::default());
        let (tx, _rx) = mpsc::channel(4);

        app.handle_network_event(
            NetworkEvent::Text("Gimli tells you 'hello'".to_string()),
            &tx,
        )
        .await;

        let message = app
            .state
            .social
            .messages
            .back()
            .expect("social message should be captured");
        assert_eq!(message.channel, SocialChannel::Tells);
        assert_eq!(message.prefix, "from Gimli - ");
        assert_eq!(message.text, "hello");
        assert_eq!(message.timestamp.len(), 5);
        assert_eq!(message.timestamp.chars().nth(2), Some(':'));
        assert!(app.state.output.iter().any(|line| {
            line.normalized == "Gimli tells you 'hello'" && line.category == OutputCategory::Normal
        }));
    }

    #[tokio::test]
    async fn mouse_scrolls_social_panel_independently() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 220, 50);
        for index in 0..10 {
            app.state.social.push(
                SocialChannel::Chats,
                "12:34",
                "",
                format!("message {index}"),
            );
        }
        let layout = app.resolved_layout(app.last_terminal_area);
        let area = app.social_panel_area(&layout).expect("social panel area");

        let changed = app.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: area.x.saturating_add(1),
            row: area.y.saturating_add(1),
            modifiers: KeyModifiers::empty(),
        });

        assert!(!changed);
        assert_eq!(app.state.social.scroll_offset, 3);
        assert_eq!(app.state.output_view.scroll_offset, 0);
    }

    #[tokio::test]
    async fn semicolon_splits_mud_commands() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("look;score".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "look")
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "score")
        );
    }

    #[tokio::test]
    async fn repeat_command_sends_braced_command_requested_count() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("#3 {look}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn repeat_command_only_repeats_braced_group_before_semicolon() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(8);

        app.handle_command(
            ClientCommand::SendText("#2 {look;score};rest".to_string()),
            &tx,
        )
        .await;

        for expected in ["look", "score", "look", "score", "rest"] {
            assert_eq!(
                rx.recv().await,
                Some(ClientCommand::SendText(expected.to_string()))
            );
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn repeat_command_expands_aliases_inside_braced_group() {
        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "scan".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "scan".to_string(),
            commands: vec!["look".to_string(), "score".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(8);

        app.handle_command(ClientCommand::SendText("#2 {scan}".to_string()), &tx)
            .await;

        for expected in ["look", "score", "look", "score"] {
            assert_eq!(
                rx.recv().await,
                Some(ClientCommand::SendText(expected.to_string()))
            );
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn repeat_command_errors_without_braced_body() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(1);

        app.handle_command(ClientCommand::SendText("#2 look".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized == "usage: #<count> {command[;command...]}")
        );
    }

    #[tokio::test]
    async fn alias_expansion_can_emit_semicolon_separated_commands() {
        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "scan".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "scan".to_string(),
            commands: vec!["look;score".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("scan".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
    }

    #[tokio::test]
    async fn mouse_wheel_scrolls_mud_output_when_over_output_pane() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        for index in 0..80 {
            app.state
                .push_output(format!("line {index}"), OutputCategory::Normal);
        }
        let (tx, _rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 50,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        assert_eq!(app.state.output_view.scroll_offset, 3);
    }

    #[tokio::test]
    async fn mouse_wheel_scroll_up_clamps_to_oldest_full_output_page() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        for index in 0..80 {
            app.state
                .push_output(format!("line {index}"), OutputCategory::Normal);
        }
        let (tx, _rx) = mpsc::channel(4);

        for _ in 0..100 {
            app.handle_terminal_event(
                TerminalEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollUp,
                    column: 50,
                    row: 10,
                    modifiers: KeyModifiers::NONE,
                }),
                &tx,
            )
            .await;
        }
        let max_offset = app.output_scroll_max();
        assert_eq!(app.state.output_view.scroll_offset, max_offset);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 50,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        assert_eq!(
            app.state.output_view.scroll_offset,
            max_offset.saturating_sub(3)
        );
    }

    #[tokio::test]
    async fn mouse_wheel_ignores_sidebar_scrolls() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        for index in 0..20 {
            app.state
                .push_output(format!("line {index}"), OutputCategory::Normal);
        }
        let (tx, _rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 150,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        assert_eq!(app.state.output_view.scroll_offset, 0);
    }

    #[tokio::test]
    async fn mouse_drag_resizes_right_sidebar() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 126,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;
        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 110,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        assert_eq!(app.full_hd_overrides.right_width, Some(50));
    }

    #[tokio::test]
    async fn mouse_drag_resizes_left_sidebar() {
        let mut config = AppConfig::default();
        config.layout.full_hd.sidebar_position = crate::config::SidebarPosition::Left;
        let mut app = App::new(config);
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 33,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;
        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 49,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        assert_eq!(app.full_hd_overrides.left_width, Some(50));
    }

    #[tokio::test]
    async fn mouse_drag_resizes_stacked_mud_output_boundary() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 70, 24);
        let (tx, _rx) = mpsc::channel(4);
        let initial = app.resolved_layout(app.last_terminal_area);
        let divider_row = initial
            .map_output_divider
            .expect("stacked layout should expose map/output divider")
            .y;

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 20,
                row: divider_row,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;
        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 20,
                row: divider_row.saturating_add(3),
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        let resized = app.resolved_layout(app.last_terminal_area);
        assert!(app.stacked_overrides.stacked_map_height.is_some());
        assert!(resized.map.height > initial.map.height);
        assert!(resized.output.height >= crate::ui::layout::MIN_OUTPUT_HEIGHT);
        assert_eq!(resized.input.height, crate::ui::layout::COMMAND_HEIGHT);
    }

    #[tokio::test]
    async fn dragging_full_hd_divider_preserves_minimum_output_width() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 126,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;
        app.handle_terminal_event(
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
            &tx,
        )
        .await;

        let layout = app.resolved_layout(app.last_terminal_area);
        assert!(layout.left.is_none());
        assert_eq!(layout.right.expect("right pane").area.width, 140);
        assert_eq!(layout.output.width, MIN_CENTER_WIDTH);
    }

    #[test]
    fn stale_full_hd_override_does_not_block_dragging_after_terminal_shrink() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 120, 40);
        app.full_hd_overrides.right_width = Some(151);

        app.resize_pane_from_mouse(Divider::Right, 94, 10);

        assert_eq!(app.full_hd_overrides.right_width, Some(26));
        let layout = app.resolved_layout(app.last_terminal_area);
        assert!(layout.left.is_none());
        assert_eq!(layout.right.expect("right pane").area.width, 26);
        assert_eq!(layout.output.width, 94);
    }

    #[tokio::test]
    async fn alias_expansion_errors_surface_as_output_errors() {
        let mut config = AppConfig::default();
        config.aliases.max_expansion_depth = 1;
        config.aliases.rules.push(AliasRuleConfig {
            name: "loop".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "x".to_string(),
            commands: vec!["x".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("x".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state.output.back().map(|line| &line.category),
            Some(&OutputCategory::Error)
        );
    }

    #[tokio::test]
    async fn plural_highlights_command_is_sent_to_mud() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        let handled = app
            .handle_command(ClientCommand::SendText("/highlights".to_string()), &tx)
            .await;

        assert!(!handled);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("/highlights".to_string()))
        );
    }

    #[tokio::test]
    async fn alias_command_lists_loaded_aliases() {
        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "recall".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "rr".to_string(),
            commands: vec!["recall".to_string(), "look".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/alias".to_string()), &tx)
            .await;

        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("`rr`"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("`recall`"))
        );
    }

    #[tokio::test]
    async fn alias_command_adds_runtime_prefix_alias_with_braced_argument() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/alias {k} {kill {1}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("k bear".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill bear".to_string()))
        );
    }

    #[tokio::test]
    async fn alias_command_adds_runtime_exact_alias_with_multiple_commands() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/alias {rr} {recall} {look}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("rr".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("recall".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
    }

    #[tokio::test]
    async fn alias_unset_and_clear_remove_runtime_aliases() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/alias {k} {kill {1}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/alias unset {k}".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("k bear".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("k bear".to_string()))
        );
        assert!(app.aliases.runtime_configs().is_empty());

        app.handle_command(
            ClientCommand::SendText("/alias {rr} {recall}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/alias clear".to_string()), &tx)
            .await;

        assert!(app.aliases.runtime_configs().is_empty());
    }

    #[tokio::test]
    async fn runtime_clear_preserves_configured_scripting_rules() {
        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "kill".to_string(),
            match_type: AliasMatchType::Prefix,
            pattern: "k".to_string(),
            commands: vec!["kill {1}".to_string()],
            ..AliasRuleConfig::default()
        });
        config.triggers.rules.push(TriggerRuleConfig {
            name: "hungry".to_string(),
            pattern: "You are hungry".to_string(),
            commands: vec!["eat bread".to_string()],
            ..TriggerRuleConfig::default()
        });
        config.highlights.rules.push(HighlightRuleConfig {
            name: "orc".to_string(),
            pattern: "orc".to_string(),
            foreground: Some("blue".to_string()),
            ..HighlightRuleConfig::default()
        });
        config.events.handlers.push(EventHandlerConfig {
            name: "ready".to_string(),
            event: "Ready".to_string(),
            commands: vec!["score".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(8);

        for command in [
            "/alias {rr} {recall}",
            "/trigger {runtime hungry} {drink water}",
            "/highlight {runtime orc} {red}",
            "/handler {RuntimeReady} {look}",
            "/alias clear",
            "/trigger clear",
            "/highlight clear",
            "/handler clear",
        ] {
            app.handle_command(ClientCommand::SendText(command.to_string()), &tx)
                .await;
        }

        app.handle_command(ClientCommand::SendText("k bear".to_string()), &tx)
            .await;
        app.push_mud_output_line("You are hungry.", &tx).await;
        app.push_mud_output_line("An orc arrives.", &tx).await;
        app.handle_command(ClientCommand::SendText("/event {Ready}".to_string()), &tx)
            .await;

        for expected in ["kill bear", "eat bread", "score"] {
            assert_eq!(
                rx.recv().await,
                Some(ClientCommand::SendText(expected.to_string()))
            );
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state
                .output
                .iter()
                .rev()
                .find(|line| line.normalized == "An orc arrives.")
                .and_then(|line| line.style.as_ref())
                .and_then(|style| style.foreground.as_deref()),
            Some("blue")
        );
    }

    #[tokio::test]
    async fn trigger_command_lists_configured_and_runtime_triggers() {
        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "wounded".to_string(),
            pattern: "You are badly wounded".to_string(),
            commands: vec!["flee".to_string()],
            ..TriggerRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/trigger {You are hungry} {eat bread}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/trigger".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(output.contains("# Triggers"));
        assert!(output.contains("You are badly wounded"));
        assert!(output.contains("You are hungry"));
    }

    #[tokio::test]
    async fn trigger_command_adds_runtime_plain_trigger() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/trigger {You are hungry} {eat bread}".to_string()),
            &tx,
        )
        .await;
        app.handle_network_event(NetworkEvent::Text("You are hungry.".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("eat bread".to_string()))
        );
    }

    #[tokio::test]
    async fn trigger_unset_and_clear_remove_runtime_triggers() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/trigger {You are hungry} {eat bread}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/trigger unset {You are hungry}".to_string()),
            &tx,
        )
        .await;
        app.push_mud_output_line("You are hungry.", &tx).await;

        assert!(rx.try_recv().is_err());
        assert!(app.triggers.runtime_configs().is_empty());

        app.handle_command(
            ClientCommand::SendText("/trigger {You are thirsty} {drink water}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/trigger clear".to_string()), &tx)
            .await;

        assert!(app.triggers.runtime_configs().is_empty());
    }

    #[tokio::test]
    async fn trigger_fires_on_prompt_events() {
        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "login".to_string(),
            pattern: "Account email".to_string(),
            commands: vec!["account@example.com".to_string()],
            ..TriggerRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(NetworkEvent::Prompt("Account email: ".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("account@example.com".to_string()))
        );
    }

    #[tokio::test]
    async fn trigger_matches_normalized_ansi_text() {
        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "arrival".to_string(),
            match_type: crate::config::MatchType::Regex,
            pattern: "^Orc arrives\\.$".to_string(),
            commands: vec!["consider orc".to_string()],
            ..TriggerRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(
            NetworkEvent::Text("\u{1b}[31mOrc arrives.\u{1b}[0m".to_string()),
            &tx,
        )
        .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("consider orc".to_string()))
        );
    }

    #[tokio::test]
    async fn triggers_command_filters_on_inherited_ansi_colors() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText(
                "/triggers plain --fg lightred --bg index:17 {Danger} {flee}".to_string(),
            ),
            &tx,
        )
        .await;
        app.handle_network_event(NetworkEvent::Text("Danger".to_string()), &tx)
            .await;
        assert!(rx.try_recv().is_err());

        app.handle_network_event(
            NetworkEvent::Text("\u{1b}[91;48;5;17mWarning".to_string()),
            &tx,
        )
        .await;
        app.handle_network_event(NetworkEvent::Text("Danger\u{1b}[0m".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("flee".to_string()))
        );
        app.handle_command(ClientCommand::SendText("/triggers".to_string()), &tx)
            .await;
        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(output.contains("fg=lightred"));
        assert!(output.contains("bg=index:17"));
    }

    #[tokio::test]
    async fn spinner_and_network_error_reset_inherited_trigger_colors() {
        let mut app = App::new(AppConfig::default());
        app.state.connection = ConnectionStatus::Connected;
        app.handle_trigger_command("plain --fg red {Danger} {flee}")
            .expect("trigger should be added");
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(
            NetworkEvent::Text("\u{1b}[31mWarning\n-\nDanger".to_string()),
            &tx,
        )
        .await;
        assert!(rx.try_recv().is_err());

        app.handle_network_event(NetworkEvent::Text("\u{1b}[31mWarning".to_string()), &tx)
            .await;
        app.handle_network_event(NetworkEvent::Error("lost connection".to_string()), &tx)
            .await;
        app.handle_network_event(NetworkEvent::Text("Danger".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(app.state.connection, ConnectionStatus::Disconnected);
    }

    #[tokio::test]
    async fn local_system_output_preserves_inherited_trigger_color() {
        let mut app = App::new(AppConfig::default());
        app.handle_trigger_command("plain --fg red {Danger} {flee}")
            .expect("trigger should be added");
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(NetworkEvent::Text("\u{1b}[31mWarning".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("look".to_string()), &tx)
            .await;
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        app.handle_network_event(NetworkEvent::Text("Danger".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("flee".to_string()))
        );
    }

    #[tokio::test]
    async fn triggers_command_rejects_invalid_color() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/triggers --fg ultraviolet {Danger} {flee}".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("invalid trigger color")
        }));
        assert!(app.triggers.runtime_configs().is_empty());
    }

    #[test]
    fn reload_preserves_runtime_trigger_color_filters() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-color-trigger-reload.toml",
            std::process::id()
        ));
        fs::write(&path, "[triggers]\nenabled = true\n")
            .expect("trigger reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );

        app.handle_trigger_command("plain --fg red {Danger} {flee}")
            .expect("runtime trigger should be added");
        app.reload_config().expect("config should reload");

        let _ = fs::remove_file(path);
        let colors = AnsiColors {
            foregrounds: vec![crate::color::AnsiColor::Red],
            backgrounds: Vec::new(),
        };
        assert_eq!(
            app.triggers
                .evaluate_with_colors("Danger", OutputCategory::Normal, &colors)
                .commands,
            ["flee"]
        );
        assert_eq!(
            app.triggers.runtime_configs()[0].foreground.as_deref(),
            Some("red")
        );
    }

    #[tokio::test]
    async fn trigger_command_adds_runtime_regex_trigger_with_multiple_commands() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText(
                "/trigger {^(.+) arrives\\.$} {target {1}} {consider {1}}".to_string(),
            ),
            &tx,
        )
        .await;
        app.handle_network_event(NetworkEvent::Text("Orc arrives.".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("target Orc".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("consider Orc".to_string()))
        );
    }

    #[tokio::test]
    async fn trigger_actions_use_the_local_command_pipeline() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText(
                "/trigger plain {You enter a room} {/toggle group off}".to_string(),
            ),
            &tx,
        )
        .await;
        app.handle_network_event(NetworkEvent::Text("You enter a room".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(!app.config.layout.show_group);
    }

    #[tokio::test]
    async fn trigger_budget_applies_after_command_and_alias_expansion() {
        let mut config = AppConfig::default();
        config.triggers.max_commands_per_line = 2;
        config.triggers.rules.push(TriggerRuleConfig {
            name: "burst".to_string(),
            pattern: "burst".to_string(),
            commands: vec!["combo".to_string()],
            ..TriggerRuleConfig::default()
        });
        config.aliases.rules.push(AliasRuleConfig {
            name: "combo".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "combo".to_string(),
            commands: vec!["one;two;three".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.push_mud_output_line("burst", &tx).await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("one".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("two".to_string()))
        );
        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("trigger execution exceeded")
        }));
    }

    #[tokio::test]
    async fn trigger_budget_also_limits_emitted_event_commands() {
        let mut config = AppConfig::default();
        config.triggers.max_commands_per_line = 1;
        config.triggers.rules.push(TriggerRuleConfig {
            name: "event".to_string(),
            pattern: "danger".to_string(),
            event: Some("Danger".to_string()),
            ..TriggerRuleConfig::default()
        });
        config.events.handlers.push(EventHandlerConfig {
            name: "responses".to_string(),
            event: "Danger".to_string(),
            commands: vec!["flee;stand".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.push_mud_output_line("danger", &tx).await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("flee".to_string()))
        );
        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("trigger execution exceeded")
        }));
    }

    #[tokio::test]
    async fn trigger_events_dispatch_handler_actions_and_notifications() {
        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "wounded".to_string(),
            pattern: "You are badly wounded".to_string(),
            event: Some("LowHealth".to_string()),
            ..TriggerRuleConfig::default()
        });
        config.events.handlers.push(EventHandlerConfig {
            name: "flee".to_string(),
            event: "LowHealth".to_string(),
            commands: vec!["flee".to_string()],
            notification: Some("Low health event fired".to_string()),
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(
            NetworkEvent::Text("You are badly wounded.".to_string()),
            &tx,
        )
        .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("flee".to_string()))
        );
        assert_eq!(app.state.script_events[0].name, "LowHealth");
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Triggered
                && line.normalized == "Low health event fired"
        }));
    }

    #[tokio::test]
    async fn manual_event_routes_local_handler_commands() {
        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "hide-group".to_string(),
            event: "CombatStarted".to_string(),
            commands: vec!["/toggle group off".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/event {CombatStarted}".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert!(!app.config.layout.show_group);
        assert_eq!(app.state.script_events[0].name, "CombatStarted");
        assert_eq!(app.state.script_events[0].source, "manual");
    }

    #[tokio::test]
    async fn event_command_lists_dispatch_status_and_recent_history() {
        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "notice".to_string(),
            event: "RoomChanged".to_string(),
            notification: Some("Room changed".to_string()),
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/event {RoomChanged}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/event".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(output.contains("Dispatch is enabled"));
        assert!(output.contains("notice"));
        assert!(output.contains("config"));
        assert!(output.contains("RoomChanged"));
        assert!(output.contains("manual"));
    }

    #[tokio::test]
    async fn handler_command_lists_complete_handler_details() {
        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "enemy".to_string(),
            enabled: false,
            priority: 75,
            match_type: MatchType::Regex,
            event: "^Enemy:(.+)$".to_string(),
            commands: vec!["target {1}".to_string()],
            notification: Some("Targeting {1}".to_string()),
            emit: vec!["CombatStarted:{1}".to_string()],
            lua: None,
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/handler".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "Source: config",
            "Match: Regex",
            "State: disabled",
            "Priority: 75",
            "target {1}",
            "Targeting {1}",
            "CombatStarted:{1}",
        ] {
            assert!(output.contains(expected), "missing `{expected}`");
        }
    }

    #[tokio::test]
    async fn handler_command_adds_and_replaces_runtime_handlers() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(8);

        app.handle_command(
            ClientCommand::SendText("/handler regex {^Enemy:(.+)$} {target {1}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/handler regex {^Enemy:(.+)$} {consider {1}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/event {Enemy:Orc}".to_string()),
            &tx,
        )
        .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("consider Orc".to_string()))
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(app.events.runtime_configs().len(), 1);
    }

    #[tokio::test]
    async fn handler_unset_and_clear_remove_runtime_handlers() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/handler {Ready} {look}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/handler unset {Ready}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/event {Ready}".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(app.events.runtime_configs().is_empty());

        app.handle_command(
            ClientCommand::SendText("/handler {Ready} {score}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/handler clear".to_string()), &tx)
            .await;

        assert!(app.events.runtime_configs().is_empty());
    }

    #[tokio::test]
    async fn handler_command_preserves_variable_templates() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("action".to_string(), "look".to_string());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/handler {Ready} {${action}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/variable {action} {score}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/event {Ready}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
    }

    #[tokio::test]
    async fn app_event_script_variant_dispatches_handlers() {
        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "mana".to_string(),
            event: "ManaLow".to_string(),
            commands: vec!["drink flask".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_app_event(AppEvent::Script(ScriptEvent::ManaLow), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("drink flask".to_string()))
        );
    }

    #[tokio::test]
    async fn handler_commands_cannot_bypass_event_cascade_limits() {
        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "loop".to_string(),
            event: "Loop".to_string(),
            commands: vec!["/event {Loop}".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/event {Loop}".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error && line.normalized.contains("must use `emit`")
        }));
        assert_eq!(app.state.script_events.len(), 1);
    }

    #[tokio::test]
    async fn manual_event_reports_disabled_dispatch() {
        let mut config = AppConfig::default();
        config.events.enabled = false;
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/event {LowHealth}".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert!(app.state.script_events.is_empty());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error && line.normalized.contains("disabled")
        }));
    }

    #[tokio::test]
    async fn expanded_handler_commands_respect_dispatch_budget() {
        let mut config = AppConfig::default();
        config.events.max_commands_per_dispatch = 2;
        config.events.handlers.push(EventHandlerConfig {
            name: "many".to_string(),
            event: "Many".to_string(),
            commands: vec!["combo".to_string()],
            ..EventHandlerConfig::default()
        });
        config.aliases.rules.push(AliasRuleConfig {
            name: "combo".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "combo".to_string(),
            commands: vec!["one;two;three".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/event {Many}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("one".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("two".to_string()))
        );
        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("after command expansion")
        }));
    }

    #[tokio::test]
    async fn map_run_handler_commands_respect_dispatch_budget() {
        let mut config = AppConfig::default();
        config.events.max_commands_per_dispatch = 2;
        config.events.handlers.push(EventHandlerConfig {
            name: "route".to_string(),
            event: "RunRoute".to_string(),
            commands: vec!["/map run 3".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        app.state.map.create();
        app.state
            .map
            .move_direction("n")
            .expect("first room should be created");
        app.state
            .map
            .move_direction("e")
            .expect("second room should be created");
        app.state
            .map
            .execute("goto 1")
            .expect("start room should exist");
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/event {RunRoute}".to_string()),
            &tx,
        )
        .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("n".to_string()))
        );
        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("after command expansion")
        }));
    }

    #[tokio::test]
    async fn unrecognized_slash_handler_command_consumes_one_budget_unit() {
        let mut config = AppConfig::default();
        config.events.max_commands_per_dispatch = 1;
        config.events.handlers.push(EventHandlerConfig {
            name: "slash".to_string(),
            event: "Slash".to_string(),
            commands: vec!["/unknown".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/event {Slash}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("/unknown".to_string()))
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn alias_trigger_and_event_actions_can_set_runtime_variables() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "orc".to_string());
        config.aliases.rules.extend([
            AliasRuleConfig {
                name: "set-target".to_string(),
                match_type: AliasMatchType::Exact,
                pattern: "set-target".to_string(),
                commands: vec!["/variable {target} {troll}".to_string()],
                ..AliasRuleConfig::default()
            },
            AliasRuleConfig {
                name: "attack".to_string(),
                match_type: AliasMatchType::Exact,
                pattern: "attack".to_string(),
                commands: vec!["kill ${target}".to_string()],
                ..AliasRuleConfig::default()
            },
        ]);
        config.triggers.rules.push(TriggerRuleConfig {
            name: "set-stance".to_string(),
            pattern: "You become enraged".to_string(),
            commands: vec!["/variable {stance} {aggressive}".to_string()],
            ..TriggerRuleConfig::default()
        });
        config.events.handlers.push(EventHandlerConfig {
            name: "set-mode".to_string(),
            event: "CombatStarted".to_string(),
            commands: vec!["/variable {mode} {combat}".to_string()],
            ..EventHandlerConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(8);

        app.handle_command(ClientCommand::SendText("set-target".to_string()), &tx)
            .await;
        app.push_mud_output_line("You become enraged.", &tx).await;
        app.handle_command(
            ClientCommand::SendText("/event {CombatStarted}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("attack".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill troll".to_string()))
        );
        let entries = app.variables.entries().unwrap();
        assert!(entries.iter().any(|entry| {
            entry.name == "target"
                && entry.value == "troll"
                && entry.source == VariableSource::Runtime
        }));
        assert!(
            entries
                .iter()
                .any(|entry| entry.name == "stance" && entry.value == "aggressive")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.name == "mode" && entry.value == "combat")
        );
    }

    #[tokio::test]
    async fn echo_writes_variable_expanded_styled_output_without_network_send() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "dragon".to_string());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText(
                "/echo --bg #101010 --fg=yellow Current Target: ${target}".to_string(),
            ),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        let line = app.state.output.back().expect("echo should add output");
        assert_eq!(line.normalized, "Current Target: dragon");
        assert_eq!(line.category, OutputCategory::Normal);
        let style = line.style.as_ref().expect("echo should retain style");
        assert_eq!(style.foreground.as_deref(), Some("yellow"));
        assert_eq!(style.background.as_deref(), Some("#101010"));
    }

    #[tokio::test]
    async fn echo_supports_plain_text_option_delimiter_and_reports_invalid_input() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/echo -- --starts-with-option".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/echo --fg ultraviolet nope".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/echo".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Normal && line.normalized == "--starts-with-option"
        }));
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line.normalized.contains("invalid echo foreground color")
        }));
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error && line.normalized.contains("usage: /echo")
        }));
    }

    #[tokio::test]
    async fn echo_spinner_glyph_is_retained_as_normal_output() {
        let mut app = App::new(AppConfig::default());
        app.state.push_prompt("Prompt>");
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/echo -".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        let line = app.state.output.back().expect("echo should add output");
        assert_eq!(line.normalized, "-");
        assert_eq!(line.category, OutputCategory::Normal);

        app.state.push_output("|", OutputCategory::Normal);

        assert_eq!(app.state.output.len(), 2);
        assert_eq!(app.state.output[0].normalized, "-");
        assert_eq!(app.state.output[0].category, OutputCategory::Normal);
        assert_eq!(app.state.output[1].normalized, "|");
        assert_eq!(app.state.output[1].category, OutputCategory::Prompt);
    }

    #[tokio::test]
    async fn direct_command_input_expands_and_escapes_variables() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "dragon".to_string());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("kill ${target}".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("say $${target}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill dragon".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("say ${target}".to_string()))
        );
    }

    #[tokio::test]
    async fn direct_input_preserves_runtime_scripting_variable_templates() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "orc".to_string());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/alias {attack} {kill ${target}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/variable {target} {dragon}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("attack".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill dragon".to_string()))
        );
    }

    #[tokio::test]
    async fn direct_input_preserves_trigger_and_nested_variable_templates() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "orc".to_string());
        config
            .variables
            .values
            .insert("arrival".to_string(), "An orc arrives".to_string());
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/trigger plain {${arrival}} {consider ${target}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/variable {action} {kill ${target}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/variable {target} {dragon}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/variable {arrival} {A dragon arrives}".to_string()),
            &tx,
        )
        .await;
        app.push_mud_output_line("A dragon arrives", &tx).await;
        app.handle_command(ClientCommand::SendText("${action}".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("consider dragon".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill dragon".to_string()))
        );
    }

    #[tokio::test]
    async fn expanded_clear_and_quit_remain_local_commands() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("clear_command".to_string(), "/clear".to_string());
        config
            .variables
            .values
            .insert("quit_command".to_string(), "/quit".to_string());
        let mut app = App::new(config);
        app.state.push_output("existing", OutputCategory::Normal);
        let (tx, mut rx) = mpsc::channel(4);

        let quit = app
            .handle_command(ClientCommand::SendText("${clear_command}".to_string()), &tx)
            .await;
        assert!(!quit);
        assert!(app.state.output.is_empty());

        let quit = app
            .handle_command(ClientCommand::SendText("${quit_command}".to_string()), &tx)
            .await;
        assert!(quit);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn undefined_direct_input_variable_is_reported_without_network_send() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("kill ${missing}".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error
                && line
                    .normalized
                    .contains("variable `missing` is not defined")
        }));
    }

    #[tokio::test]
    async fn variable_unset_reveals_configured_value_and_recompiles_aliases() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("target".to_string(), "orc".to_string());
        config.aliases.rules.push(AliasRuleConfig {
            name: "attack".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "attack".to_string(),
            commands: vec!["kill ${target}".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/variable {target} {troll}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("attack".to_string()), &tx)
            .await;
        app.handle_command(
            ClientCommand::SendText("/variable unset {target}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("attack".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill troll".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill orc".to_string()))
        );
    }

    #[tokio::test]
    async fn variable_command_accepts_tab_separator() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/variable\t{name} {value}".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(app.variables.expand("${name}").unwrap(), "value");

        app.handle_command(
            ClientCommand::SendText("/variable unset\t{name}".to_string()),
            &tx,
        )
        .await;

        assert!(app.variables.expand("${name}").is_err());
    }

    #[tokio::test]
    async fn braced_commands_accept_a_doubled_trailing_backslash() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/variable {path} {C:\\\\}".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(app.variables.expand("${path}").unwrap(), "C:\\");
    }

    #[tokio::test]
    async fn invalid_variable_update_rolls_back_all_script_engines() {
        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("pattern".to_string(), "^x$".to_string());
        config.aliases.rules.push(AliasRuleConfig {
            name: "regex".to_string(),
            match_type: AliasMatchType::Regex,
            pattern: "${pattern}".to_string(),
            commands: vec!["look".to_string()],
            ..AliasRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/variable {pattern} {[}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("x".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(app.variables.expand("${pattern}").unwrap(), "^x$");
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error && line.normalized.contains("invalid regex")
        }));
    }

    #[tokio::test]
    async fn reload_preserves_runtime_variable_overrides() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-variable-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[variables.values]
target = "configured-after"

[aliases]
enabled = false

[[aliases.rules]]
name = "attack"
match_type = "exact"
pattern = "attack"
commands = ["kill ${target}"]
"#,
        )
        .expect("variable reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/variable {target} {runtime}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/alias {runtime-alias} {say ${target}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/trigger plain {runtime trigger} {eat ${target}}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/reload".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("attack".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("runtime-alias".to_string()), &tx)
            .await;
        app.push_mud_output_line("runtime trigger", &tx).await;

        let _ = fs::remove_file(path);
        assert!(matches!(
            rx.recv().await,
            Some(ClientCommand::SetWindowSize { .. })
        ));
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("kill runtime".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("say runtime".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("eat runtime".to_string()))
        );
        assert!(app.variables.entries().unwrap().iter().any(|entry| {
            entry.name == "target"
                && entry.value == "runtime"
                && entry.source == VariableSource::Runtime
        }));
    }

    #[tokio::test]
    async fn toggle_command_updates_optional_panels_in_memory() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/toggle group off".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/toggle opponent".to_string()), &tx)
            .await;
        app.handle_command(
            ClientCommand::SendText("/toggle social off".to_string()),
            &tx,
        )
        .await;

        assert!(rx.try_recv().is_err());
        assert!(!app.config.layout.show_group);
        assert!(!app.config.layout.show_opponent);
        assert!(!app.config.layout.show_social);
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Group panel off."))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Opponent panel off."))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Social panel off."))
        );
    }

    #[tokio::test]
    async fn disconnect_clears_authoritative_msdp_room_data() {
        let mut app = App::new(AppConfig::default());
        let mapping = app.config.msdp.mapping.clone();
        let (tx, _rx) = mpsc::channel(4);

        app.state.raw_msdp.insert(
            mapping.room.clone(),
            MsdpValue::String("old room".to_string()),
        );
        app.state.raw_msdp.insert(
            mapping.room_vnum.clone(),
            MsdpValue::String("100".to_string()),
        );
        app.state.raw_msdp.insert(
            mapping.room_exits.clone(),
            MsdpValue::String("n".to_string()),
        );

        app.handle_network_event(NetworkEvent::Disconnected, &tx)
            .await;

        assert!(!app.state.has_authoritative_msdp_room(&mapping));
        assert!(!app.state.raw_msdp.contains_key(&mapping.room_exits));
    }

    #[tokio::test]
    async fn network_text_runs_triggers_and_records_events() {
        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "arrives".to_string(),
            match_type: crate::config::MatchType::Regex,
            pattern: "^(.+) arrives\\.$".to_string(),
            commands: vec!["target {1}".to_string()],
            event: Some("Enemy:{1}".to_string()),
            ..TriggerRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_network_event(NetworkEvent::Text("Orc arrives.".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("target Orc".to_string()))
        );
        assert_eq!(app.state.script_events[0].name, "Enemy:Orc");
    }

    #[tokio::test]
    async fn network_text_applies_highlight_style() {
        let mut config = AppConfig::default();
        config.highlights.rules.push(HighlightRuleConfig {
            name: "danger".to_string(),
            pattern: "bleeding".to_string(),
            foreground: Some("#f7768e".to_string()),
            bold: true,
            ..HighlightRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_network_event(NetworkEvent::Text("You are bleeding.".to_string()), &tx)
            .await;

        let style = app.state.output.back().and_then(|line| line.style.as_ref());
        assert_eq!(
            style.and_then(|style| style.foreground.as_deref()),
            Some("#f7768e")
        );
        assert!(style.is_some_and(|style| style.bold));
    }

    #[tokio::test]
    async fn highlight_command_lists_and_applies_runtime_rules() {
        let mut config = AppConfig::default();
        config.highlights.rules.push(HighlightRuleConfig {
            name: "configured".to_string(),
            pattern: "configured text".to_string(),
            foreground: Some("blue".to_string()),
            ..HighlightRuleConfig::default()
        });
        let mut app = App::new(config);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText(
                "/highlight regex {^Danger: .+$} {yellow} {#101010} {bold underline}".to_string(),
            ),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/highlight".to_string()), &tx)
            .await;
        app.push_mud_output_line("Danger: dragon", &tx).await;

        assert!(rx.try_recv().is_err());
        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(output.contains("configured text"));
        assert!(output.contains("^Danger: .+$"));
        assert!(output.contains("runtime"));
        let line = app.state.output.back().expect("MUD line should be present");
        let style = line.style.as_ref().expect("highlight should apply");
        assert_eq!(style.foreground.as_deref(), Some("yellow"));
        assert_eq!(style.background.as_deref(), Some("#101010"));
        assert!(style.bold);
        assert!(style.underline);
    }

    #[tokio::test]
    async fn highlights_match_visible_ansi_text_and_configured_prompts() {
        let mut config = AppConfig::default();
        config.highlights.rules.extend([
            HighlightRuleConfig {
                name: "ansi".to_string(),
                match_type: MatchType::Regex,
                pattern: "^Danger$".to_string(),
                foreground: Some("red".to_string()),
                ..HighlightRuleConfig::default()
            },
            HighlightRuleConfig {
                name: "prompt".to_string(),
                pattern: "Password:".to_string(),
                foreground: Some("yellow".to_string()),
                categories: vec![OutputCategoryConfig::Prompt],
                ..HighlightRuleConfig::default()
            },
        ]);
        let mut app = App::new(config);
        let (tx, _rx) = mpsc::channel(4);

        app.handle_network_event(
            NetworkEvent::Text("\u{1b}[31mDanger\u{1b}[0m".to_string()),
            &tx,
        )
        .await;
        assert_eq!(
            app.state
                .output
                .back()
                .and_then(|line| line.style.as_ref())
                .and_then(|style| style.foreground.as_deref()),
            Some("red")
        );

        app.handle_network_event(NetworkEvent::Prompt("Password: ".to_string()), &tx)
            .await;
        assert_eq!(
            app.state
                .output
                .back()
                .and_then(|line| line.style.as_ref())
                .and_then(|style| style.foreground.as_deref()),
            Some("yellow")
        );
    }

    #[tokio::test]
    async fn highlight_command_replaces_runtime_rule_and_validates_fields() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        for command in [
            "/highlight {orc} {red}",
            "/highlight plain {orc} {green} {none} {italic}",
            "/highlight regex {[} {red}",
            "/highlight {bad color} {ultraviolet}",
            "/highlight {no style} {none}",
            "/highlight {bad style} {red} {none} {blink}",
        ] {
            app.handle_command(ClientCommand::SendText(command.to_string()), &tx)
                .await;
        }
        app.push_mud_output_line("An orc arrives.", &tx).await;

        assert!(rx.try_recv().is_err());
        assert_eq!(app.highlights.runtime_configs().len(), 1);
        let line = app.state.output.back().expect("MUD line should be present");
        let style = line.style.as_ref().expect("highlight should apply");
        assert_eq!(style.foreground.as_deref(), Some("green"));
        assert!(style.italic);
        let errors = app
            .state
            .output
            .iter()
            .filter(|line| line.category == OutputCategory::Error)
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(errors.contains("invalid regex"));
        assert!(errors.contains("invalid highlight foreground color"));
        assert!(errors.contains("at least one color or style"));
        assert!(errors.contains("unknown highlight style"));
    }

    #[tokio::test]
    async fn highlight_unset_and_clear_remove_runtime_highlights() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/highlight {orc} {red}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(
            ClientCommand::SendText("/highlight unset {orc}".to_string()),
            &tx,
        )
        .await;
        app.push_mud_output_line("An orc arrives.", &tx).await;

        assert!(rx.try_recv().is_err());
        assert!(app.highlights.runtime_configs().is_empty());
        assert!(
            app.state
                .output
                .back()
                .is_some_and(|line| line.style.is_none())
        );

        app.handle_command(
            ClientCommand::SendText("/highlight {orc} {red}".to_string()),
            &tx,
        )
        .await;
        app.handle_command(ClientCommand::SendText("/highlight clear".to_string()), &tx)
            .await;

        assert!(app.highlights.runtime_configs().is_empty());
    }

    #[tokio::test]
    async fn highlight_command_accepts_escaped_literal_braces_in_regex() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/highlight regex {^\\{$} {red}".to_string()),
            &tx,
        )
        .await;
        app.push_mud_output_line("{", &tx).await;

        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state
                .output
                .back()
                .and_then(|line| line.style.as_ref())
                .and_then(|style| style.foreground.as_deref()),
            Some("red")
        );
    }

    #[test]
    fn reload_preserves_runtime_highlights() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-highlight-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[highlights]
enabled = true
"#,
        )
        .expect("highlight reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );

        app.handle_highlights_command("{dragon} {magenta}")
            .expect("runtime highlight should be added");
        app.reload_config().expect("config should reload");

        let _ = fs::remove_file(path);
        let style = app
            .highlights
            .style_for("A dragon arrives.", OutputCategory::Normal)
            .expect("runtime highlight should survive reload");
        assert_eq!(style.foreground.as_deref(), Some("magenta"));
        assert_eq!(app.highlights.runtime_configs().len(), 1);
    }

    #[tokio::test]
    async fn map_commands_are_handled_locally() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("/map create".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert!(rx.try_recv().is_err());
        assert_eq!(app.state.map.current_room.as_deref(), Some("1"));
        assert_eq!(
            app.state.output.back().map(|line| &line.category),
            Some(&OutputCategory::System)
        );
    }

    #[test]
    fn map_persistence_loads_relative_to_config_path_on_startup() {
        let root = std::env::temp_dir().join(format!(
            "mud-client-test-{}-startup-map",
            std::process::id()
        ));
        let map_dir = root.join("maps");
        fs::create_dir_all(&map_dir).expect("map test directory should be created");
        let map_path = map_dir.join("rots.toml");
        let config_path = root.join("config.toml");
        let mut map = crate::map::MapState::default();
        map.create();
        map.rooms.get_mut("1").unwrap().name = "Loaded Room".to_string();
        map.write_to_path(&map_path)
            .expect("test map should be written");

        let mut config = AppConfig::default();
        config.map.persistence.load_on_startup = true;
        config.map.persistence.path = "maps/rots.toml".to_string();
        let app =
            App::new_with_config_source(config, Some(config_path), ConfigLoadOptions::default());

        assert_eq!(app.state.map.rooms["1"].name.as_str(), "Loaded Room");
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::System && line.normalized.contains("Read map from")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn map_persistence_startup_error_is_visible_without_aborting() {
        let root = std::env::temp_dir().join(format!(
            "mud-client-test-{}-missing-startup-map",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("map test directory should be created");
        let config_path = root.join("config.toml");
        let mut config = AppConfig::default();
        config.map.persistence.load_on_startup = true;
        config.map.persistence.path = "maps/missing.toml".to_string();

        let app =
            App::new_with_config_source(config, Some(config_path), ConfigLoadOptions::default());

        assert!(app.state.map.rooms.is_empty());
        assert!(app.state.output.iter().any(|line| {
            line.category == OutputCategory::Error && line.normalized.contains("Failed to read map")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn map_persistence_save_on_exit_writes_relative_to_config_path() {
        let root = std::env::temp_dir().join(format!(
            "mud-client-test-{}-save-exit-map",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("map test directory should be created");
        let config_path = root.join("config.toml");
        let map_path = root.join("maps/rots.toml");
        let mut config = AppConfig::default();
        config.map.persistence.save_on_exit = true;
        config.map.persistence.path = "maps/rots.toml".to_string();
        let mut app =
            App::new_with_config_source(config, Some(config_path), ConfigLoadOptions::default());
        app.state.map.create();
        app.state.map.rooms.get_mut("1").unwrap().name = "Saved Room".to_string();

        app.save_exit_map();

        let mut loaded = crate::map::MapState::default();
        loaded
            .read_from_path(&map_path)
            .expect("saved map should be readable");
        assert_eq!(loaded.rooms["1"].name.as_str(), "Saved Room");
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn map_link_command_is_handled_locally() {
        let mut app = App::new(AppConfig::default());
        app.state.map.create();
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(
                ClientCommand::SendText("/map link e 12 both".to_string()),
                &tx,
            )
            .await;

        assert!(!should_quit);
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state.map.rooms["1"].exits["e"].to.as_deref(),
            Some("12")
        );
        assert_eq!(
            app.state.map.rooms["12"].exits["w"].to.as_deref(),
            Some("1")
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Linked e to 12."))
        );
    }

    #[tokio::test]
    async fn first_macro_help_opens_at_its_heading_and_can_resume_following() {
        let mut app = App::new(AppConfig::default());
        app.last_terminal_area = Rect::new(0, 0, 100, 30);
        let (tx, mut rx) = mpsc::channel(4);
        for _ in 0..2 {
            app.handle_command(ClientCommand::SendText("/help macro".into()), &tx)
                .await;
            let area = app.resolved_layout(app.last_terminal_area).output;
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            crate::ui::output::render_output(area, &mut buffer, &app.state, &app.theme, "Output");
            let first_row = (area.x + 1..area.right() - 1)
                .map(|x| buffer[(x, area.y + 1)].symbol())
                .collect::<String>();
            assert!(first_row.starts_with("Macro Help"), "{first_row:?}");
            assert!(!app.state.output_view.follow_newest);
            app.scroll_output_down(usize::MAX);
            assert!(app.state.output_view.follow_newest);
            assert_eq!(app.state.output_view.wrapped_row_offset, 0);
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn help_command_is_handled_locally() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("/help".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Mud Client Help"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("## Topics"))
        );
    }

    #[tokio::test]
    async fn default_help_lists_all_base_commands() {
        let mut app = App::new(AppConfig::default());
        let (tx, _rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/help".to_string()), &tx)
            .await;

        let output = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for command in [
            "/help",
            "/clear",
            "/quit",
            "/reload",
            "/reconnect",
            "/msdp",
            "/alias",
            "/trigger",
            "/toggle",
            "/map",
        ] {
            assert!(output.contains(command), "{command} should be listed");
        }
        for topic in ["trigger", "highlight", "animation", "diagnostics"] {
            assert!(output.contains(topic), "{topic} should be listed");
        }
    }

    #[tokio::test]
    async fn reconnect_command_routes_network_reconnect() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/reconnect".to_string()), &tx)
            .await;

        assert_eq!(rx.recv().await, Some(ClientCommand::Reconnect));
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Reconnect requested."))
        );
    }

    #[tokio::test]
    async fn reload_command_applies_valid_config_and_preserves_panel_toggles() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r##"
[connection]
host = "localhost"
port = 3791

[panels.output]
title = "Game"
"##,
        )
        .expect("reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        app.config.layout.show_group = false;
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/reload".to_string()), &tx)
            .await;

        let _ = fs::remove_file(path);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SetWindowSize {
                width: 124,
                height: 27,
            })
        );
        assert_eq!(app.config.panels.output.title, "Game");
        assert!(!app.config.layout.show_group);
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Config Reloaded"))
        );
    }

    #[tokio::test]
    async fn reload_command_rebuilds_event_handlers() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-event-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r##"
[events]
history_limit = 1

[[events.handlers]]
name = "reloaded"
event = "ReloadedEvent"
commands = ["score"]
"##,
        )
        .expect("event reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        app.state.script_events.extend([
            crate::state::ScriptEventRecord {
                name: "OldOne".to_string(),
                source: "test".to_string(),
            },
            crate::state::ScriptEventRecord {
                name: "OldTwo".to_string(),
                source: "test".to_string(),
            },
        ]);
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/reload".to_string()), &tx)
            .await;
        app.handle_command(
            ClientCommand::SendText("/event {ReloadedEvent}".to_string()),
            &tx,
        )
        .await;

        let _ = fs::remove_file(path);
        assert!(matches!(
            rx.recv().await,
            Some(ClientCommand::SetWindowSize { .. })
        ));
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
        assert_eq!(app.state.script_events.len(), 1);
        assert_eq!(app.state.script_events[0].name, "ReloadedEvent");
    }

    #[tokio::test]
    async fn reload_preserves_runtime_event_handlers() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-runtime-event-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r##"
[connection]
host = "localhost"
port = 3791
"##,
        )
        .expect("reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(
            ClientCommand::SendText("/handler {Ready} {score}".to_string()),
            &tx,
        )
        .await;
        app.reload_config().expect("config should reload");
        app.handle_command(ClientCommand::SendText("/event {Ready}".to_string()), &tx)
            .await;

        let _ = fs::remove_file(path);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
        assert_eq!(app.events.runtime_configs().len(), 1);
    }

    #[tokio::test]
    async fn reload_rejects_runtime_handler_over_new_command_limit() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-runtime-event-limit.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r##"
[events]
max_commands_per_dispatch = 1
"##,
        )
        .expect("reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        let (tx, mut rx) = mpsc::channel(4);
        app.handle_command(
            ClientCommand::SendText("/handler {Ready} {look} {score}".to_string()),
            &tx,
        )
        .await;

        let error = app
            .reload_config()
            .expect_err("tighter limit should reject runtime handler");
        app.handle_command(ClientCommand::SendText("/event {Ready}".to_string()), &tx)
            .await;

        let _ = fs::remove_file(path);
        assert!(error.contains("max_commands_per_dispatch"));
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("look".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("score".to_string()))
        );
    }

    #[tokio::test]
    async fn reload_command_surfaces_validation_errors() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-bad-reload.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r##"
[connection]
host = ""
port = 3791
"##,
        )
        .expect("reload config should be written");
        let mut app = App::new_with_config_source(
            AppConfig::default(),
            Some(path.clone()),
            ConfigLoadOptions::default(),
        );
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/reload".to_string()), &tx)
            .await;

        let _ = fs::remove_file(path);
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state.output.back().map(|line| &line.category),
            Some(&OutputCategory::Error)
        );
        assert!(
            app.state
                .output
                .back()
                .is_some_and(|line| line.normalized.contains("Config reload failed"))
        );
    }

    #[tokio::test]
    async fn help_topics_render_specific_content() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/help path".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("/map run <vnum|name>"))
        );

        app.handle_command(ClientCommand::SendText("/help toggle".to_string()), &tx)
            .await;

        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Toggle Help"))
        );
        app.handle_command(ClientCommand::SendText("/help trigger".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("/help highlight".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("/help animation".to_string()), &tx)
            .await;
        app.handle_command(
            ClientCommand::SendText("/help diagnostics".to_string()),
            &tx,
        )
        .await;

        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Trigger Help"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Highlight Help"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Animation Help"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# Diagnostics Help"))
        );
    }

    #[tokio::test]
    async fn msdp_command_outputs_current_stored_values() {
        let mut app = App::new(AppConfig::default());
        app.state
            .raw_msdp
            .insert("HEALTH".to_string(), MsdpValue::String("465".to_string()));
        app.state.raw_msdp.insert(
            "ROOM".to_string(),
            MsdpValue::Table(
                [
                    (
                        "NAME".to_string(),
                        MsdpValue::String("A Shadowy Forest".to_string()),
                    ),
                    ("VNUM".to_string(), MsdpValue::String("6945".to_string())),
                ]
                .into(),
            ),
        );
        app.state.raw_msdp.insert(
            "REPORTABLE_VARIABLES".to_string(),
            MsdpValue::Array(vec![
                MsdpValue::String("HEALTH".to_string()),
                MsdpValue::String("ROOM".to_string()),
            ]),
        );
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/msdp".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        let lines = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("# MSDP Values")));
        assert!(lines.iter().any(|line| line.contains("- `HEALTH`: `465`")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("- `REPORTABLE_VARIABLES`: [`HEALTH`, `ROOM`]"))
        );
        assert!(lines.iter().any(|line| {
            line.contains("- `ROOM`: {")
                && line.contains("`NAME`: `A Shadowy Forest`")
                && line.contains("`VNUM`: `6945`")
        }));
    }

    #[tokio::test]
    async fn msdp_command_outputs_empty_state_message() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/msdp".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("No MSDP values"))
        );
    }

    #[tokio::test]
    async fn help_map_command_topics_render_focused_content() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/help map door".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("/help map create".to_string()), &tx)
            .await;
        app.handle_command(ClientCommand::SendText("/help map goto".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# /map door"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("Door markers render centered"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# /map create"))
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| line.normalized.contains("# /map goto"))
        );
    }

    #[tokio::test]
    async fn unknown_help_topic_outputs_error() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/help nope".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state.output.back().map(|line| &line.category),
            Some(&OutputCategory::Error)
        );
    }

    #[tokio::test]
    async fn map_run_sends_path_commands() {
        let mut app = App::new(AppConfig::default());
        app.state.map.create();
        app.state.map.move_direction("n").unwrap();
        app.state.map.move_direction("e").unwrap();
        app.state.map.execute("goto 1").unwrap();
        let (tx, mut rx) = mpsc::channel(4);

        let should_quit = app
            .handle_command(ClientCommand::SendText("/map run 3".to_string()), &tx)
            .await;

        assert!(!should_quit);
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("n".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("e".to_string()))
        );
    }

    #[tokio::test]
    async fn map_map_fills_the_mud_output_content_area() {
        let mut config = AppConfig::default();
        config.layout.scrollback_lines = 1;
        let mut app = App::new(config);
        app.last_terminal_area = Rect::new(0, 0, 160, 32);
        app.state.map.create();
        app.state.map.unicode = false;
        app.state
            .map
            .move_direction("n")
            .expect("north room should be created");
        app.state
            .map
            .move_direction("n")
            .expect("second north room should be created");
        app.state
            .map
            .execute("goto 1")
            .expect("start room should exist");
        let layout = app.resolved_layout(app.last_terminal_area);
        let expected_width = layout.output.width.saturating_sub(2) as usize;
        let expected_height = layout.output.height.saturating_sub(2) as usize;
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/map map".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(app.state.output.len(), expected_height);
        assert!(app.state.output.iter().all(|line| {
            unicode_width::UnicodeWidthStr::width(plain_text(&line.raw).as_str()) == expected_width
        }));
        assert_eq!(
            app.state
                .output
                .get(expected_height / 2)
                .and_then(|line| plain_text(&line.raw).chars().nth(expected_width / 2)),
            Some('X')
        );
        assert!(
            app.state
                .output
                .iter()
                .any(|line| plain_text(&line.raw).trim() == "|")
        );
    }

    #[tokio::test]
    async fn bare_map_command_outputs_markdown_help() {
        let mut app = App::new(AppConfig::default());
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/map".to_string()), &tx)
            .await;

        assert!(rx.try_recv().is_err());
        assert_eq!(
            app.state
                .output
                .front()
                .map(|line| line.normalized.as_str()),
            Some("# Map Commands")
        );
        assert!(
            app.state
                .output
                .iter()
                .all(|line| { line.category == OutputCategory::System })
        );
        assert!(app.state.output.iter().any(|line| {
            line.normalized
                == "- `/map map` - show a full MUD-output-pane map centered on the current room"
        }));
    }

    #[tokio::test]
    async fn movement_commands_track_locally_before_msdp_room_data() {
        let mut app = App::new(AppConfig::default());
        app.state.map.create();
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("n".to_string()), &tx)
            .await;

        assert_eq!(app.state.map.current_room.as_deref(), Some("2"));
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("n".to_string()))
        );
    }

    #[tokio::test]
    async fn movement_commands_open_closed_named_doors_first() {
        let mut app = App::new(AppConfig::default());
        app.state.map.create();
        app.state.map.execute("dig w").unwrap();
        app.state.map.execute("door w closed stone door").unwrap();
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("west".to_string()), &tx)
            .await;

        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("open stone door w".to_string()))
        );
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("w".to_string()))
        );
    }

    #[tokio::test]
    async fn movement_commands_do_not_speculatively_move_after_msdp_room_data() {
        let config = AppConfig::default();
        let mut app = App::new(config.clone());
        app.state.apply_msdp_frames(
            &[crate::network::msdp::MsdpFrame {
                variable: config.msdp.mapping.room.clone(),
                value: crate::network::msdp::MsdpValue::Table(
                    [(
                        "VNUM".to_string(),
                        crate::network::msdp::MsdpValue::String("100".to_string()),
                    )]
                    .into(),
                ),
            }],
            &config.msdp.mapping,
        );
        let (tx, mut rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("n".to_string()), &tx)
            .await;

        assert_eq!(app.state.map.current_room.as_deref(), Some("100"));
        assert_eq!(
            rx.recv().await,
            Some(ClientCommand::SendText("n".to_string()))
        );
    }

    #[tokio::test]
    async fn map_multiline_output_renders_as_separate_lines() {
        let mut app = App::new(AppConfig::default());
        app.state.map.create();
        let (tx, _rx) = mpsc::channel(4);

        app.handle_command(ClientCommand::SendText("/map get".to_string()), &tx)
            .await;

        let lines = app
            .state
            .output
            .iter()
            .map(|line| line.normalized.as_str())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.starts_with("Room 1:")));
        assert!(lines.iter().any(|line| line.starts_with("Coords:")));
        assert!(lines.iter().any(|line| line.starts_with("Terrain:")));
        assert!(lines.iter().any(|line| line.starts_with("Weight:")));
        assert!(lines.iter().any(|line| line.starts_with("Exits:")));
    }
}
