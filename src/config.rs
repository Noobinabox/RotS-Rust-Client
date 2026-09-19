use std::{collections::BTreeMap, fs, io, path::PathBuf, time::Duration};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing_subscriber::{EnvFilter, fmt};
use unicode_width::UnicodeWidthChar;

use crate::{
    color::{parse_ansi_color, parse_color},
    error::{MudClientError, Result},
    scripting::templates::validate_capture_references,
    scripting::variables::VariableStore,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub connection: ConnectionConfig,
    pub terminal: TerminalConfig,
    pub layout: LayoutConfig,
    pub colors: ThemeConfig,
    pub gauges: GaugeConfig,
    pub animation: AnimationConfig,
    pub weather: WeatherConfig,
    pub social: SocialConfig,
    pub panels: PanelConfig,
    pub map: MapRenderConfig,
    pub msdp: MsdpConfig,
    pub aliases: AliasConfig,
    pub triggers: TriggerConfig,
    pub events: EventConfig,
    pub lua: LuaConfig,
    pub variables: VariableConfig,
    pub highlights: HighlightConfig,
    pub logging: LoggingConfig,
}

impl AppConfig {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        Self::load_with_options(path, ConfigLoadOptions::default())
    }

    pub fn load_with_options(path: Option<PathBuf>, options: ConfigLoadOptions) -> Result<Self> {
        let Some(path) = path.or_else(default_config_path) else {
            let mut config = Self::default();
            if options.local_test_endpoint {
                config.use_local_test_endpoint();
            }
            config.normalize();
            config.validate()?;
            return Ok(config);
        };

        if !path.exists() {
            let mut config = Self::default();
            if options.local_test_endpoint {
                config.use_local_test_endpoint();
            }
            config.normalize();
            config.validate()?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&path)?;
        let mut config: Self =
            toml::from_str(&raw).map_err(|source| MudClientError::ConfigParse { path, source })?;
        if options.local_test_endpoint {
            config.use_local_test_endpoint();
        }
        config.normalize();
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.connection.host.trim().is_empty() {
            return Err(MudClientError::ConfigValidation(
                "connection.host must not be empty".to_string(),
            ));
        }
        if self.connection.port == 0 {
            return Err(MudClientError::ConfigValidation(
                "connection.port must be greater than zero".to_string(),
            ));
        }
        if self.terminal.tick_rate_ms == 0 {
            return Err(MudClientError::ConfigValidation(
                "terminal.tick_rate_ms must be greater than zero".to_string(),
            ));
        }
        if self.terminal.animation_fps == 0 {
            return Err(MudClientError::ConfigValidation(
                "terminal.animation_fps must be greater than zero".to_string(),
            ));
        }
        let breakpoints = &self.layout.breakpoints;
        if breakpoints.mobile_width == 0
            || breakpoints.mobile_height == 0
            || breakpoints.mobile_width >= breakpoints.tablet_width
            || breakpoints.mobile_height >= breakpoints.tablet_height
            || breakpoints.tablet_width >= breakpoints.ultrawide_width
            || breakpoints.tablet_height >= breakpoints.ultrawide_height
        {
            return Err(MudClientError::ConfigValidation(
                "layout.breakpoints must increase from mobile to tablet to ultrawide".to_string(),
            ));
        }
        for (name, layout) in [
            ("mobile", &self.layout.mobile),
            ("tablet", &self.layout.tablet),
        ] {
            if layout.map_height < 3 {
                return Err(MudClientError::ConfigValidation(format!(
                    "layout.{name}.map_height must be at least 3"
                )));
            }
        }
        if self.layout.full_hd.sidebar_width < 20 {
            return Err(MudClientError::ConfigValidation(
                "layout.full_hd.sidebar_width must be at least 20".to_string(),
            ));
        }
        let layout = &self.layout.ultrawide;
        if [layout.top, layout.left, layout.right].contains(&PaneRole::ClassicSidebar) {
            return Err(MudClientError::ConfigValidation(
                "layout.ultrawide does not support the classic_sidebar role".to_string(),
            ));
        }
        if layout.top != PaneRole::None && layout.top_height < 3 {
            return Err(MudClientError::ConfigValidation(
                "layout.ultrawide.top_height must be at least 3 when the top pane is enabled"
                    .to_string(),
            ));
        }
        if layout.left != PaneRole::None && layout.left_width < 20 {
            return Err(MudClientError::ConfigValidation(
                "layout.ultrawide.left_width must be at least 20 when the left pane is enabled"
                    .to_string(),
            ));
        }
        if layout.right != PaneRole::None && layout.right_width < 20 {
            return Err(MudClientError::ConfigValidation(
                "layout.ultrawide.right_width must be at least 20 when the right pane is enabled"
                    .to_string(),
            ));
        }
        let map_slots = [layout.top, layout.left, layout.right]
            .into_iter()
            .filter(|role| *role == PaneRole::Map)
            .count();
        if map_slots != 1 {
            return Err(MudClientError::ConfigValidation(
                "layout.ultrawide must assign the map to exactly one of top, left, or right"
                    .to_string(),
            ));
        }
        if self.layout.scrollback_lines == 0 {
            return Err(MudClientError::ConfigValidation(
                "layout.scrollback_lines must be greater than zero".to_string(),
            ));
        }
        if self.social.scrollback_lines == 0 {
            return Err(MudClientError::ConfigValidation(
                "social.scrollback_lines must be greater than zero".to_string(),
            ));
        }
        if self.animation.map_fps == 0 || self.animation.weather_fps == 0 {
            return Err(MudClientError::ConfigValidation(
                "animation frame rates must be greater than zero".to_string(),
            ));
        }
        for (name, panel) in self.panels.iter() {
            if panel.title.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "panels.{name}.title must not be empty"
                )));
            }
            if panel.min_width == 0 {
                return Err(MudClientError::ConfigValidation(format!(
                    "panels.{name}.min_width must be greater than zero"
                )));
            }
            if panel.min_height == 0 {
                return Err(MudClientError::ConfigValidation(format!(
                    "panels.{name}.min_height must be greater than zero"
                )));
            }
        }
        for (name, panel) in [
            ("map", &self.panels.map),
            ("output", &self.panels.output),
            ("input", &self.panels.input),
        ] {
            if !panel.enabled || !panel.visible_modes.is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "panels.{name} is required and must be enabled with visible_modes = []"
                )));
            }
        }
        if !self.layout.show_output {
            return Err(MudClientError::ConfigValidation(
                "layout.show_output must be true because MUD output is required".to_string(),
            ));
        }
        if self.map.room_spacing_columns <= 0 {
            return Err(MudClientError::ConfigValidation(
                "map.room_spacing_columns must be greater than zero".to_string(),
            ));
        }
        if self.map.room_spacing_rows <= 0 {
            return Err(MudClientError::ConfigValidation(
                "map.room_spacing_rows must be greater than zero".to_string(),
            ));
        }
        if !is_fixed_width_map_symbol(&self.map.current_room_symbol, 1) {
            return Err(MudClientError::ConfigValidation(
                "map.current_room_symbol must be exactly one terminal cell".to_string(),
            ));
        }
        if !is_fixed_width_map_symbol(&self.map.stub_symbol, 1) {
            return Err(MudClientError::ConfigValidation(
                "map.stub_symbol must be exactly one terminal cell".to_string(),
            ));
        }
        if (self.map.persistence.load_on_startup || self.map.persistence.save_on_exit)
            && self.map.persistence.path.trim().is_empty()
        {
            return Err(MudClientError::ConfigValidation(
                "map.persistence.path must not be empty when map persistence is enabled"
                    .to_string(),
            ));
        }
        if self.map.doors.show && !is_fixed_width_map_symbol(&self.map.doors.glyph, 1) {
            return Err(MudClientError::ConfigValidation(
                "map.doors.glyph must be exactly one terminal cell when doors are shown"
                    .to_string(),
            ));
        }
        if self.map.teleport.show && !is_fixed_width_map_symbol(&self.map.teleport.glyph, 1) {
            return Err(MudClientError::ConfigValidation(
                "map.teleport.glyph must be exactly one terminal cell when teleport markers are shown"
                    .to_string(),
            ));
        }
        for (terrain, style) in &self.map.terrain {
            if terrain.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "map terrain names must not be empty".to_string(),
                ));
            }
            let expected_width = if style.double { 2 } else { 1 };
            if !is_fixed_width_map_symbol(&style.symbol, expected_width) {
                return Err(MudClientError::ConfigValidation(format!(
                    "map terrain `{terrain}` symbol must contain exactly {expected_width} one-cell character(s)"
                )));
            }
            if !is_allowed_map_option(&style.density, &["", "dense", "sparse", "scant"]) {
                return Err(MudClientError::ConfigValidation(format!(
                    "map terrain `{}` density must be dense, sparse, scant, or empty",
                    terrain
                )));
            }

            if !is_allowed_map_option(&style.spread, &["", "narrow", "wide", "vast"]) {
                return Err(MudClientError::ConfigValidation(format!(
                    "map terrain `{}` spread must be narrow, wide, vast, or empty",
                    terrain
                )));
            }
            if !is_allowed_map_option(&style.fade, &["", "fadein", "fadeout"]) {
                return Err(MudClientError::ConfigValidation(format!(
                    "map terrain `{}` fade must be fadein, fadeout, or empty",
                    terrain
                )));
            }
        }
        if self.msdp.report_variables.is_empty() {
            return Err(MudClientError::ConfigValidation(
                "msdp.report_variables must include at least one variable".to_string(),
            ));
        }
        if self.aliases.max_expansion_depth == 0 {
            return Err(MudClientError::ConfigValidation(
                "aliases.max_expansion_depth must be greater than zero".to_string(),
            ));
        }
        if self.aliases.max_expanded_commands == 0 {
            return Err(MudClientError::ConfigValidation(
                "aliases.max_expanded_commands must be greater than zero".to_string(),
            ));
        }
        if self.triggers.max_commands_per_line == 0 {
            return Err(MudClientError::ConfigValidation(
                "triggers.max_commands_per_line must be greater than zero".to_string(),
            ));
        }
        if self.events.max_dispatch_depth == 0
            || self.events.max_events_per_dispatch == 0
            || self.events.max_commands_per_dispatch == 0
            || self.events.history_limit == 0
        {
            return Err(MudClientError::ConfigValidation(
                "event dispatch limits and history_limit must be greater than zero".to_string(),
            ));
        }
        if self.lua.enabled {
            if self.lua.script_dir.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "lua.script_dir must not be empty when Lua is enabled".to_string(),
                ));
            }
            if self.lua.entrypoint.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "lua.entrypoint must not be empty when Lua is enabled".to_string(),
                ));
            }
            if self.lua.instruction_budget == 0 || self.lua.max_actions_per_hook == 0 {
                return Err(MudClientError::ConfigValidation(
                    "lua.instruction_budget and lua.max_actions_per_hook must be greater than zero"
                        .to_string(),
                ));
            }
        }
        let variables = VariableStore::new(&self.variables)?;
        for alias in &self.aliases.rules {
            if alias.name.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "alias name must not be empty".to_string(),
                ));
            }
            if alias.pattern.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` pattern must not be empty",
                    alias.name
                )));
            }
            if alias.commands.is_empty() && alias.lua.is_none() {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` must include at least one command or lua hook",
                    alias.name
                )));
            }
            if alias.lua.as_ref().is_some_and(|lua| lua.trim().is_empty()) {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` lua hook must not be empty",
                    alias.name
                )));
            }
            if alias.commands.len() > self.aliases.max_expanded_commands {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` has more commands than aliases.max_expanded_commands",
                    alias.name
                )));
            }
            if alias
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` commands must not be empty",
                    alias.name
                )));
            }
            let pattern = variables.expand(&alias.pattern)?;
            let commands = alias
                .commands
                .iter()
                .map(|command| variables.expand(command))
                .collect::<Result<Vec<_>>>()?;
            if pattern.trim().is_empty() || commands.iter().any(|command| command.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "alias `{}` pattern and commands must not expand to empty values",
                    alias.name
                )));
            }
            let available_captures = match alias.match_type {
                AliasMatchType::Exact => 0,
                AliasMatchType::Prefix => 1,
                AliasMatchType::Regex => regex::Regex::new(&pattern)
                    .map_err(|error| {
                        MudClientError::ConfigValidation(format!(
                            "alias `{}` has invalid regex pattern: {error}",
                            alias.name
                        ))
                    })?
                    .captures_len()
                    .saturating_sub(1),
            };
            for command in &commands {
                validate_capture_references(
                    command,
                    available_captures,
                    &format!("alias `{}`", alias.name),
                )?;
            }
        }
        for trigger in &self.triggers.rules {
            if trigger.name.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "trigger name must not be empty".to_string(),
                ));
            }
            if trigger.pattern.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` pattern must not be empty",
                    trigger.name
                )));
            }
            if trigger
                .event
                .as_ref()
                .is_some_and(|event| event.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` event must not be empty",
                    trigger.name
                )));
            }
            if trigger
                .lua
                .as_ref()
                .is_some_and(|lua| lua.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` lua hook must not be empty",
                    trigger.name
                )));
            }
            if trigger
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` commands must not be empty",
                    trigger.name
                )));
            }
            let action_count = trigger.commands.len()
                + usize::from(trigger.event.is_some())
                + usize::from(trigger.lua.is_some());
            if action_count > self.triggers.max_commands_per_line {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` has {action_count} actions but triggers.max_commands_per_line is {}",
                    trigger.name, self.triggers.max_commands_per_line
                )));
            }
            for (field, value) in [
                ("foreground", trigger.foreground.as_deref()),
                ("background", trigger.background.as_deref()),
            ] {
                if let Some(value) = value
                    && parse_ansi_color(value).is_none()
                {
                    return Err(MudClientError::ConfigValidation(format!(
                        "trigger `{}` has invalid {field} color `{value}`",
                        trigger.name
                    )));
                }
            }
            let pattern = variables.expand(&trigger.pattern)?;
            let templates = trigger
                .commands
                .iter()
                .chain(trigger.event.iter())
                .map(|template| variables.expand(template))
                .collect::<Result<Vec<_>>>()?;
            if pattern.trim().is_empty()
                || templates.iter().any(|template| template.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "trigger `{}` pattern and actions must not expand to empty values",
                    trigger.name
                )));
            }
            let available_captures = if trigger.match_type == MatchType::Regex {
                regex::Regex::new(&pattern)
                    .map_err(|error| {
                        MudClientError::ConfigValidation(format!(
                            "trigger `{}` has invalid regex pattern: {error}",
                            trigger.name
                        ))
                    })?
                    .captures_len()
                    .saturating_sub(1)
            } else {
                0
            };
            for template in &templates {
                validate_capture_references(
                    template,
                    available_captures,
                    &format!("trigger `{}`", trigger.name),
                )?;
            }
        }
        for handler in &self.events.handlers {
            if handler.name.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "event handler name must not be empty".to_string(),
                ));
            }
            if handler.event.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` event pattern must not be empty",
                    handler.name
                )));
            }
            if handler.commands.is_empty()
                && handler.emit.is_empty()
                && handler.notification.is_none()
                && handler.lua.is_none()
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` must define a command, emitted event, notification, or lua hook",
                    handler.name
                )));
            }
            if handler
                .lua
                .as_ref()
                .is_some_and(|lua| lua.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` lua hook must not be empty",
                    handler.name
                )));
            }
            if handler
                .commands
                .iter()
                .chain(&handler.emit)
                .any(|action| action.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` actions must not be empty",
                    handler.name
                )));
            }
            if handler
                .notification
                .as_ref()
                .is_some_and(|notification| notification.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` notification must not be empty",
                    handler.name
                )));
            }
            if handler.commands.len() > self.events.max_commands_per_dispatch {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` has more commands than events.max_commands_per_dispatch",
                    handler.name
                )));
            }
            let event = variables.expand(&handler.event)?;
            let templates = handler
                .commands
                .iter()
                .chain(&handler.emit)
                .chain(handler.notification.iter())
                .map(|template| variables.expand(template))
                .collect::<Result<Vec<_>>>()?;
            if event.trim().is_empty()
                || templates.iter().any(|template| template.trim().is_empty())
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "event handler `{}` pattern and actions must not expand to empty values",
                    handler.name
                )));
            }
            let available_captures = if handler.match_type == MatchType::Regex {
                regex::Regex::new(&event)
                    .map_err(|error| {
                        MudClientError::ConfigValidation(format!(
                            "event handler `{}` has invalid regex pattern: {error}",
                            handler.name
                        ))
                    })?
                    .captures_len()
                    .saturating_sub(1)
            } else {
                0
            };
            for template in &templates {
                validate_capture_references(
                    template,
                    available_captures,
                    &format!("event handler `{}`", handler.name),
                )?;
            }
        }
        for highlight in &self.highlights.rules {
            if highlight.name.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(
                    "highlight name must not be empty".to_string(),
                ));
            }
            if highlight.pattern.trim().is_empty() {
                return Err(MudClientError::ConfigValidation(format!(
                    "highlight `{}` pattern must not be empty",
                    highlight.name
                )));
            }
            if highlight.match_type == MatchType::Regex
                && let Err(error) = regex::Regex::new(&highlight.pattern)
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "highlight `{}` has invalid regex pattern: {error}",
                    highlight.name
                )));
            }
            for (field, value) in [
                ("foreground", highlight.foreground.as_deref()),
                ("background", highlight.background.as_deref()),
            ] {
                if let Some(value) = value
                    && parse_color(value).is_none()
                {
                    return Err(MudClientError::ConfigValidation(format!(
                        "highlight `{}` has invalid {field} color `{value}`",
                        highlight.name
                    )));
                }
            }
            if highlight.foreground.is_none()
                && highlight.background.is_none()
                && !highlight.bold
                && !highlight.dim
                && !highlight.italic
                && !highlight.underline
                && !highlight.reverse
            {
                return Err(MudClientError::ConfigValidation(format!(
                    "highlight `{}` must define at least one color or style",
                    highlight.name
                )));
            }
        }
        Ok(())
    }

    pub fn use_local_test_endpoint(&mut self) {
        self.connection.host = "localhost".to_string();
        self.connection.port = 3791;
    }

    fn normalize(&mut self) {
        self.msdp.ensure_required_room_reports();
    }

    pub fn init_logging(&self) {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(self.logging.level.clone()));
        let _ = fmt()
            .with_env_filter(filter)
            .with_writer(io::sink)
            .try_init();
    }
}

fn is_fixed_width_map_symbol(value: &str, expected_width: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() == expected_width
        && value.chars().all(|character| character.width() == Some(1))
}

fn is_allowed_map_option(value: &str, allowed: &[&str]) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    allowed.iter().any(|option| normalized == *option)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigLoadOptions {
    pub local_test_endpoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub auto_reconnect: bool,
    pub line_ending: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 3791,
            username: String::new(),
            password: String::new(),
            auto_reconnect: true,
            line_ending: "\r\n".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TerminalConfig {
    pub tick_rate_ms: u64,
    pub animation_fps: u64,
    pub mouse: bool,
    pub true_color: bool,
    pub echo_commands: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: 100,
            animation_fps: 15,
            mouse: true,
            true_color: true,
            echo_commands: true,
        }
    }
}

impl TerminalConfig {
    pub fn tick_rate(&self) -> Duration {
        Duration::from_millis(self.tick_rate_ms)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LayoutConfig {
    pub breakpoints: DisplayBreakpointsConfig,
    pub mobile: StackedLayoutConfig,
    pub tablet: StackedLayoutConfig,
    pub full_hd: FullHdLayoutConfig,
    pub ultrawide: LargeLayoutConfig,
    pub show_character: bool,
    pub show_opponent: bool,
    pub show_group: bool,
    pub show_social: bool,
    pub show_output: bool,
    pub scrollback_lines: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            breakpoints: DisplayBreakpointsConfig::default(),
            mobile: StackedLayoutConfig {
                map_height: 5,
                show_status: false,
            },
            tablet: StackedLayoutConfig {
                map_height: 8,
                show_status: true,
            },
            full_hd: FullHdLayoutConfig {
                sidebar_position: SidebarPosition::Right,
                sidebar_width: 34,
            },
            ultrawide: LargeLayoutConfig {
                top_height: 9,
                left_width: 42,
                right_width: 46,
                left: PaneRole::Details,
                right: PaneRole::Map,
                ..LargeLayoutConfig::default()
            },
            show_character: true,
            show_opponent: true,
            show_group: false,
            show_social: true,
            show_output: true,
            scrollback_lines: 2_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SocialConfig {
    pub scrollback_lines: usize,
}

impl Default for SocialConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DisplayBreakpointsConfig {
    pub mobile_width: u16,
    pub mobile_height: u16,
    pub tablet_width: u16,
    pub tablet_height: u16,
    pub ultrawide_width: u16,
    pub ultrawide_height: u16,
}

impl Default for DisplayBreakpointsConfig {
    fn default() -> Self {
        Self {
            mobile_width: 80,
            mobile_height: 24,
            tablet_width: 120,
            tablet_height: 30,
            ultrawide_width: 200,
            ultrawide_height: 45,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StackedLayoutConfig {
    pub map_height: u16,
    pub show_status: bool,
}

impl Default for StackedLayoutConfig {
    fn default() -> Self {
        Self {
            map_height: 5,
            show_status: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FullHdLayoutConfig {
    pub sidebar_position: SidebarPosition,
    pub sidebar_width: u16,
}

impl Default for FullHdLayoutConfig {
    fn default() -> Self {
        Self {
            sidebar_position: SidebarPosition::Right,
            sidebar_width: 34,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SidebarPosition {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LargeLayoutConfig {
    pub top_height: u16,
    pub left_width: u16,
    pub right_width: u16,
    pub top: PaneRole,
    pub left: PaneRole,
    pub right: PaneRole,
}

impl Default for LargeLayoutConfig {
    fn default() -> Self {
        Self {
            top_height: 7,
            left_width: 28,
            right_width: 34,
            top: PaneRole::StatusDashboard,
            left: PaneRole::Details,
            right: PaneRole::Map,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaneRole {
    Map,
    StatusDashboard,
    Details,
    Info,
    ClassicSidebar,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PanelConfig {
    pub info: PanelOptions,
    pub social: PanelOptions,
    pub map: PanelOptions,
    pub opponent: PanelOptions,
    pub group: PanelOptions,
    pub character: PanelOptions,
    pub output: PanelOptions,
    pub input: PanelOptions,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            info: PanelOptions::new("World", 20, 5, 100),
            social: PanelOptions::new("Social", 30, 5, 95),
            map: PanelOptions::new("Map", 20, 5, 90),
            opponent: PanelOptions::new("Opponent", 20, 4, 80),
            group: PanelOptions::new("Group", 20, 3, 70),
            character: PanelOptions::new("Character", 20, 6, 60),
            output: PanelOptions::new("MUD Output", 20, 5, 100),
            input: PanelOptions::new("Command", 20, 3, 100),
        }
    }
}

impl PanelConfig {
    fn iter(&self) -> [(&str, &PanelOptions); 8] {
        [
            ("info", &self.info),
            ("social", &self.social),
            ("map", &self.map),
            ("opponent", &self.opponent),
            ("group", &self.group),
            ("character", &self.character),
            ("output", &self.output),
            ("input", &self.input),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PanelOptions {
    pub enabled: bool,
    pub title: String,
    pub min_width: u16,
    pub min_height: u16,
    pub priority: i32,
    pub visible_modes: Vec<PanelMode>,
}

impl PanelOptions {
    fn new(title: &str, min_width: u16, min_height: u16, priority: i32) -> Self {
        Self {
            enabled: true,
            title: title.to_string(),
            min_width,
            min_height,
            priority,
            visible_modes: Vec::new(),
        }
    }
}

impl Default for PanelOptions {
    fn default() -> Self {
        Self::new("", 1, 1, 0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PanelMode {
    #[serde(rename = "mobile", alias = "tiny")]
    Mobile,
    #[serde(rename = "tablet", alias = "compact")]
    Tablet,
    #[serde(rename = "1080p", alias = "standard")]
    FullHd,
    #[serde(rename = "ultrawide", alias = "wide")]
    Ultrawide,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    pub background: String,
    pub foreground: String,
    pub border: String,
    pub title: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub muted: String,
    pub player: String,
    pub enemy: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: "#101010".to_string(),
            foreground: "#d0c8b0".to_string(),
            border: "#806f4a".to_string(),
            title: "#d8b365".to_string(),
            accent: "#c49a50".to_string(),
            success: "#65b875".to_string(),
            warning: "#d6ad55".to_string(),
            danger: "#d45c5c".to_string(),
            muted: "#777777".to_string(),
            player: "#e5c07b".to_string(),
            enemy: "#e06c75".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GaugeConfig {
    pub unicode: bool,
    pub width: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub reduced_motion: bool,
    pub low_performance: bool,
    pub map_fps: u64,
    pub weather_fps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub show_info_marker: bool,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_info_marker: true,
        }
    }
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            reduced_motion: false,
            low_performance: false,
            map_fps: 12,
            weather_fps: 10,
        }
    }
}

impl Default for GaugeConfig {
    fn default() -> Self {
        Self {
            unicode: true,
            width: 14,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapRenderConfig {
    pub show_links: bool,
    pub room_spacing_columns: i32,
    pub room_spacing_rows: i32,
    pub current_room_symbol: String,
    pub current_room_color: String,
    pub stub_symbol: String,
    pub link_color: String,
    pub avoid_color: String,
    pub block_color: String,
    pub hide_color: String,
    pub invis_color: String,
    pub fog_color: String,
    pub void_color: String,
    pub persistence: MapPersistenceConfig,
    pub doors: MapDoorConfig,
    pub teleport: MapTeleportConfig,
    pub terrain: BTreeMap<String, MapTerrainConfig>,
}

impl Default for MapRenderConfig {
    fn default() -> Self {
        Self {
            show_links: true,
            room_spacing_columns: 3,
            room_spacing_rows: 2,
            current_room_symbol: "X".to_string(),
            current_room_color: "#cc0000".to_string(),
            stub_symbol: "∘".to_string(),
            link_color: "#505050".to_string(),
            avoid_color: "#e0af68".to_string(),
            block_color: "#f7768e".to_string(),
            hide_color: "#565f89".to_string(),
            invis_color: "#7aa2f7".to_string(),
            fog_color: "#89ddff".to_string(),
            void_color: "#414868".to_string(),
            persistence: MapPersistenceConfig::default(),
            doors: MapDoorConfig::default(),
            teleport: MapTeleportConfig::default(),
            terrain: default_map_terrain(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapPersistenceConfig {
    pub load_on_startup: bool,
    pub path: String,
    pub save_on_exit: bool,
}

impl Default for MapPersistenceConfig {
    fn default() -> Self {
        Self {
            load_on_startup: false,
            path: "maps/rots.toml".to_string(),
            save_on_exit: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapDoorConfig {
    pub show: bool,
    pub glyph: String,
    pub open_color: String,
    pub closed_color: String,
    pub pickable_color: String,
    pub locked_color: String,
    pub trigger_color: String,
    pub unknown_color: String,
}

impl Default for MapDoorConfig {
    fn default() -> Self {
        Self {
            show: true,
            glyph: "╬".to_string(),
            open_color: "#65b875".to_string(),
            closed_color: "#d6ad55".to_string(),
            pickable_color: "#61afef".to_string(),
            locked_color: "#d45c5c".to_string(),
            trigger_color: "#c678dd".to_string(),
            unknown_color: "#777777".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapTeleportConfig {
    pub show: bool,
    pub glyph: String,
    pub color: String,
}

impl Default for MapTeleportConfig {
    fn default() -> Self {
        Self {
            show: true,
            glyph: "◇".to_string(),
            color: "#61afef".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapTerrainConfig {
    pub symbol: String,
    pub color: String,
    pub density: String,
    pub spread: String,
    pub fade: String,
    pub double: bool,
}

impl Default for MapTerrainConfig {
    fn default() -> Self {
        Self {
            symbol: "∘".to_string(),
            color: "#777777".to_string(),
            density: String::new(),
            spread: String::new(),
            fade: String::new(),
            double: false,
        }
    }
}

fn default_map_terrain() -> BTreeMap<String, MapTerrainConfig> {
    [
        ("City", "⌂", "#ffffff"),
        ("Road", "═", "#ffffff"),
        ("Floor", "∙", "#404040"),
        ("Field", "″", "#8a8a1e"),
        ("Forest", "♣", "#2e7d32"),
        ("Dense_forest", "♠", "#1b5e20"),
        ("Hills", "∩", "#8d6e63"),
        ("Mountain", "▲", "#5d4037"),
        ("Water", "≈", "#1565c0"),
        ("Water_noswim", "≋", "#0d47a1"),
        ("Underwater", "≋", "#0d47a1"),
        ("Swamp", "∼", "#6d7a2e"),
        ("Crack", "✕", "#5d4037"),
    ]
    .into_iter()
    .map(|(terrain, symbol, color)| {
        (
            terrain.to_string(),
            MapTerrainConfig {
                symbol: symbol.to_string(),
                color: color.to_string(),
                ..MapTerrainConfig::default()
            },
        )
    })
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MsdpConfig {
    pub client_id: String,
    pub client_version: String,
    pub ansi_colors: bool,
    pub xterm_256_colors: bool,
    pub utf_8: bool,
    pub report_variables: Vec<String>,
    pub mapping: MsdpMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AliasConfig {
    pub enabled: bool,
    pub max_expansion_depth: usize,
    pub max_expanded_commands: usize,
    pub rules: Vec<AliasRuleConfig>,
}

impl Default for AliasConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_expansion_depth: 8,
            max_expanded_commands: 256,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AliasRuleConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub match_type: AliasMatchType,
    pub pattern: String,
    pub commands: Vec<String>,
    pub lua: Option<String>,
}

impl Default for AliasRuleConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            priority: 0,
            match_type: AliasMatchType::Regex,
            pattern: String::new(),
            commands: Vec::new(),
            lua: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AliasMatchType {
    Exact,
    Prefix,
    #[default]
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TriggerConfig {
    pub enabled: bool,
    pub max_commands_per_line: usize,
    pub rules: Vec<TriggerRuleConfig>,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_commands_per_line: 8,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TriggerRuleConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub match_type: MatchType,
    pub pattern: String,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub commands: Vec<String>,
    pub event: Option<String>,
    pub lua: Option<String>,
    pub cooldown_ms: u64,
    pub one_shot: bool,
    pub categories: Vec<OutputCategoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct EventConfig {
    pub enabled: bool,
    pub max_dispatch_depth: usize,
    pub max_events_per_dispatch: usize,
    pub max_commands_per_dispatch: usize,
    pub history_limit: usize,
    pub handlers: Vec<EventHandlerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LuaConfig {
    pub enabled: bool,
    pub script_dir: String,
    pub entrypoint: String,
    pub instruction_budget: u32,
    pub max_actions_per_hook: usize,
    pub runtime_errors_to_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VariableConfig {
    pub max_expansion_depth: usize,
    pub max_expanded_bytes: usize,
    pub values: BTreeMap<String, String>,
}

impl Default for VariableConfig {
    fn default() -> Self {
        Self {
            max_expansion_depth: 8,
            max_expanded_bytes: 65_536,
            values: BTreeMap::new(),
        }
    }
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_dispatch_depth: 8,
            max_events_per_dispatch: 32,
            max_commands_per_dispatch: 16,
            history_limit: 100,
            handlers: Vec::new(),
        }
    }
}

impl Default for LuaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            script_dir: "scripts".to_string(),
            entrypoint: "init.lua".to_string(),
            instruction_budget: 100_000,
            max_actions_per_hook: 32,
            runtime_errors_to_output: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct EventHandlerConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub match_type: MatchType,
    pub event: String,
    pub commands: Vec<String>,
    pub emit: Vec<String>,
    pub notification: Option<String>,
    pub lua: Option<String>,
}

impl Default for EventHandlerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            priority: 0,
            match_type: MatchType::Plain,
            event: String::new(),
            commands: Vec::new(),
            emit: Vec::new(),
            notification: None,
            lua: None,
        }
    }
}

impl Default for TriggerRuleConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            priority: 0,
            match_type: MatchType::Plain,
            pattern: String::new(),
            foreground: None,
            background: None,
            commands: Vec::new(),
            event: None,
            lua: None,
            cooldown_ms: 0,
            one_shot: false,
            categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HighlightConfig {
    pub enabled: bool,
    pub rules: Vec<HighlightRuleConfig>,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HighlightRuleConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub match_type: MatchType,
    pub pattern: String,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub categories: Vec<OutputCategoryConfig>,
}

impl Default for HighlightRuleConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            priority: 0,
            match_type: MatchType::Plain,
            pattern: String::new(),
            foreground: None,
            background: None,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            reverse: false,
            categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    #[default]
    Plain,
    Regex,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputCategoryConfig {
    Normal,
    Combat,
    Communication,
    System,
    Error,
    Prompt,
    Triggered,
}

impl Default for MsdpConfig {
    fn default() -> Self {
        Self {
            client_id: "mud-client".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            ansi_colors: true,
            xterm_256_colors: true,
            utf_8: true,
            report_variables: [
                "CHARACTER_NAME",
                "RACE",
                "LEVEL",
                "HEALTH",
                "HEALTH_MAX",
                "MANA",
                "MANA_MAX",
                "MOVEMENT",
                "MOVEMENT_MAX",
                "EXPERIENCE",
                "EXPERIENCE_MAX",
                "OFFENSIVE_BONUS",
                "DODGE",
                "PARRY",
                "ATTACK_SPEED",
                "STR",
                "INT",
                "WILL",
                "DEX",
                "CON",
                "LEA",
                "WILLPOWER",
                "SPELL_SAVE",
                "SPIRIT",
                "SPELL_POWER",
                "SPELL_PEN",
                "OPPONENT_NAME",
                "OPPONENT_LEVEL",
                "OPPONENT_HEALTH",
                "OPPONENT_HEALTH_MAX",
                "ROOM",
                "ROOM_NAME",
                "ROOM_VNUM",
                "ROOM_EXITS",
                "WORLD_TIME",
                "WEATHER",
                "GROUP",
            ]
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
            mapping: MsdpMapping::default(),
        }
    }
}

impl MsdpConfig {
    fn ensure_required_room_reports(&mut self) {
        let required = [
            self.mapping.character_name.as_str(),
            self.mapping.race.as_str(),
            self.mapping.level.as_str(),
            self.mapping.health.as_str(),
            self.mapping.health_max.as_str(),
            self.mapping.mana.as_str(),
            self.mapping.mana_max.as_str(),
            self.mapping.movement.as_str(),
            self.mapping.movement_max.as_str(),
            self.mapping.experience.as_str(),
            self.mapping.experience_max.as_str(),
            self.mapping.offensive_bonus.as_str(),
            self.mapping.dodge.as_str(),
            self.mapping.parry.as_str(),
            self.mapping.attack_speed.as_str(),
            self.mapping.strength.as_str(),
            self.mapping.intelligence.as_str(),
            self.mapping.will.as_str(),
            self.mapping.dexterity.as_str(),
            self.mapping.constitution.as_str(),
            self.mapping.learning.as_str(),
            self.mapping.willpower.as_str(),
            self.mapping.spell_save.as_str(),
            self.mapping.spirit.as_str(),
            self.mapping.spell_power.as_str(),
            self.mapping.spell_pen.as_str(),
            self.mapping.warrior_level.as_str(),
            self.mapping.ranger_level.as_str(),
            self.mapping.mystic_level.as_str(),
            self.mapping.mage_level.as_str(),
            self.mapping.health_regeneration.as_str(),
            self.mapping.stamina_regeneration.as_str(),
            self.mapping.movement_regeneration.as_str(),
            self.mapping.opponent_name.as_str(),
            self.mapping.opponent_level.as_str(),
            self.mapping.opponent_health.as_str(),
            self.mapping.opponent_health_max.as_str(),
            self.mapping.room.as_str(),
            self.mapping.room_name.as_str(),
            self.mapping.room_vnum.as_str(),
            self.mapping.room_exits.as_str(),
            self.mapping.world_time.as_str(),
            self.mapping.weather.as_str(),
            self.mapping.group.as_str(),
        ];
        for variable in required {
            if !variable.trim().is_empty()
                && !self
                    .report_variables
                    .iter()
                    .any(|reported| reported == variable)
            {
                self.report_variables.push(variable.to_string());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MsdpMapping {
    pub character_name: String,
    pub race: String,
    pub level: String,
    pub health: String,
    pub health_max: String,
    pub mana: String,
    pub mana_max: String,
    pub movement: String,
    pub movement_max: String,
    pub experience: String,
    pub experience_max: String,
    pub offensive_bonus: String,
    pub dodge: String,
    pub parry: String,
    pub attack_speed: String,
    pub strength: String,
    pub intelligence: String,
    pub will: String,
    pub dexterity: String,
    pub constitution: String,
    pub learning: String,
    pub willpower: String,
    pub spell_save: String,
    pub spirit: String,
    pub spell_power: String,
    pub spell_pen: String,
    pub warrior_level: String,
    pub ranger_level: String,
    pub mystic_level: String,
    pub mage_level: String,
    pub health_regeneration: String,
    pub stamina_regeneration: String,
    pub movement_regeneration: String,
    pub opponent_name: String,
    pub opponent_health: String,
    pub opponent_health_max: String,
    pub opponent_level: String,
    pub room: String,
    pub room_name: String,
    pub room_vnum: String,
    pub room_exits: String,
    pub world_time: String,
    pub weather: String,
    pub group: String,
    pub group_members: String,
    pub group_member_name: String,
    pub group_member_health: String,
    pub group_member_mana: String,
    pub group_member_movement: String,
}

impl Default for MsdpMapping {
    fn default() -> Self {
        Self {
            character_name: "CHARACTER_NAME".to_string(),
            race: "RACE".to_string(),
            level: "LEVEL".to_string(),
            health: "HEALTH".to_string(),
            health_max: "HEALTH_MAX".to_string(),
            mana: "MANA".to_string(),
            mana_max: "MANA_MAX".to_string(),
            movement: "MOVEMENT".to_string(),
            movement_max: "MOVEMENT_MAX".to_string(),
            experience: "EXPERIENCE".to_string(),
            experience_max: "EXPERIENCE_MAX".to_string(),
            offensive_bonus: "OFFENSIVE_BONUS".to_string(),
            dodge: "DODGE".to_string(),
            parry: "PARRY".to_string(),
            attack_speed: "ATTACK_SPEED".to_string(),
            strength: "STR".to_string(),
            intelligence: "INT".to_string(),
            will: "WILL".to_string(),
            dexterity: "DEX".to_string(),
            constitution: "CON".to_string(),
            learning: "LEA".to_string(),
            willpower: "WILLPOWER".to_string(),
            spell_save: "SPELL_SAVE".to_string(),
            spirit: "SPIRIT".to_string(),
            spell_power: "SPELL_POWER".to_string(),
            spell_pen: "SPELL_PEN".to_string(),
            warrior_level: "WARRIOR_LEVEL".to_string(),
            ranger_level: "RANGER_LEVEL".to_string(),
            mystic_level: "MYSTIC_LEVEL".to_string(),
            mage_level: "MAGE_LEVEL".to_string(),
            health_regeneration: "HEALTH_REGENERATION".to_string(),
            stamina_regeneration: "STAMINA_REGENERATION".to_string(),
            movement_regeneration: "MOVEMENT_REGENERATION".to_string(),
            opponent_name: "OPPONENT_NAME".to_string(),
            opponent_health: "OPPONENT_HEALTH".to_string(),
            opponent_health_max: "OPPONENT_HEALTH_MAX".to_string(),
            opponent_level: "OPPONENT_LEVEL".to_string(),
            room: "ROOM".to_string(),
            room_name: "ROOM_NAME".to_string(),
            room_vnum: "ROOM_VNUM".to_string(),
            room_exits: "ROOM_EXITS".to_string(),
            world_time: "WORLD_TIME".to_string(),
            weather: "WEATHER".to_string(),
            group: "GROUP".to_string(),
            group_members: "MEMBERS".to_string(),
            group_member_name: "NAME".to_string(),
            group_member_health: "HEALTH".to_string(),
            group_member_mana: "MANA".to_string(),
            group_member_movement: "MOVEMENT".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub raw_protocol: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "mud_client=info".to_string(),
            raw_protocol: false,
        }
    }
}

fn default_config_path() -> Option<PathBuf> {
    ProjectDirs::from("org", "seth", "mud-client").map(|dirs| dirs.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_target_local_rots() {
        let config = AppConfig::default();
        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.connection.port, 3791);
        assert!(config.msdp.report_variables.contains(&"ROOM".to_string()));
        assert!(config.msdp.report_variables.contains(&"GROUP".to_string()));
        assert!(config.msdp.report_variables.contains(&"RACE".to_string()));
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"OFFENSIVE_BONUS".to_string())
        );
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"EXPERIENCE_MAX".to_string())
        );
        assert_eq!(config.msdp.mapping.movement, "MOVEMENT");
        assert_eq!(config.msdp.mapping.race, "RACE");
        assert_eq!(config.msdp.mapping.offensive_bonus, "OFFENSIVE_BONUS");
        assert_eq!(config.msdp.mapping.spell_power, "SPELL_POWER");
        assert_eq!(config.map.current_room_symbol, "X");
        assert_eq!(config.map.stub_symbol, "∘");
        assert_eq!(config.map.teleport.glyph, "◇");
        assert_eq!(
            config
                .map
                .terrain
                .get("Forest")
                .map(|style| style.symbol.as_str()),
            Some("♣")
        );
    }

    #[test]
    fn local_test_endpoint_targets_localhost() {
        let mut config = AppConfig::default();
        config.use_local_test_endpoint();

        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.connection.port, 3791);
    }

    #[test]
    fn local_test_endpoint_overrides_invalid_configured_endpoint_before_validation() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-config.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[connection]
host = ""
port = 0
"#,
        )
        .expect("test config should be written");

        let config = AppConfig::load_with_options(
            Some(path.clone()),
            ConfigLoadOptions {
                local_test_endpoint: true,
            },
        )
        .expect("local endpoint should override invalid configured endpoint");

        let _ = fs::remove_file(path);
        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.connection.port, 3791);
    }

    #[test]
    fn loading_older_config_adds_required_msdp_reports() {
        let path = std::env::temp_dir().join(format!(
            "mud-client-test-{}-older-msdp-config.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[msdp]
report_variables = ["ROOM_NAME", "ROOM_VNUM", "ROOM_EXITS"]
"#,
        )
        .expect("test config should be written");

        let config = AppConfig::load_with_options(Some(path.clone()), ConfigLoadOptions::default())
            .expect("older config should load");

        let _ = fs::remove_file(path);
        assert!(config.msdp.report_variables.contains(&"ROOM".to_string()));
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"ROOM_EXITS".to_string())
        );
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"OPPONENT_HEALTH".to_string())
        );
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"OPPONENT_NAME".to_string())
        );
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"WORLD_TIME".to_string())
        );
        assert!(
            config
                .msdp
                .report_variables
                .contains(&"WEATHER".to_string())
        );
        assert!(config.msdp.report_variables.contains(&"GROUP".to_string()));
        assert!(config.msdp.report_variables.contains(&"RACE".to_string()));
    }

    #[test]
    fn missing_door_config_fields_use_defaults() {
        let config: AppConfig = toml::from_str(
            r##"
[map.doors]
glyph = "#"
"##,
        )
        .expect("partial door config should parse");

        assert_eq!(config.map.doors.glyph, "#");
        assert_eq!(config.map.doors.trigger_color, "#c678dd");
        assert_eq!(config.map.doors.locked_color, "#d45c5c");
    }

    #[test]
    fn legacy_panel_modes_deserialize_as_display_profiles() {
        let config: AppConfig = toml::from_str(
            r#"
[panels.info]
visible_modes = ["tiny", "compact", "standard", "wide"]
"#,
        )
        .expect("legacy mode names should parse");

        assert_eq!(
            config.panels.info.visible_modes,
            vec![
                PanelMode::Mobile,
                PanelMode::Tablet,
                PanelMode::FullHd,
                PanelMode::Ultrawide,
            ]
        );
    }

    #[test]
    fn validation_rejects_invalid_values() {
        let mut config = AppConfig::default();
        config.connection.host.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.terminal.tick_rate_ms = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.msdp.report_variables.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.social.scrollback_lines = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.aliases.max_expansion_depth = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.aliases.max_expanded_commands = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.variables.max_expansion_depth = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.variables.max_expanded_bytes = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "missing-variable".to_string(),
            match_type: AliasMatchType::Exact,
            pattern: "${missing}".to_string(),
            commands: vec!["look".to_string()],
            ..AliasRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("one".to_string(), "${two}".to_string());
        config
            .variables
            .values
            .insert("two".to_string(), "${one}".to_string());
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config
            .variables
            .values
            .insert("empty".to_string(), "   ".to_string());
        config.triggers.rules.push(TriggerRuleConfig {
            name: "empty-pattern".to_string(),
            pattern: "${empty}".to_string(),
            commands: vec!["look".to_string()],
            ..TriggerRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.room_spacing_columns = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.current_room_symbol.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.current_room_symbol = "界".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.doors.glyph = "##".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.teleport.glyph = "##".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.persistence.load_on_startup = true;
        config.map.persistence.path.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.persistence.save_on_exit = true;
        config.map.persistence.path = "   ".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().symbol.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().symbol = "界".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().double = true;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().density = "packed".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().spread = "huge".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.map.terrain.get_mut("Forest").unwrap().fade = "middle".to_string();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.panels.output.title.clear();
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.panels.map.min_height = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.panels.map.enabled = false;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.layout.ultrawide.left = PaneRole::Map;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.layout.ultrawide.right = PaneRole::Details;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.layout.ultrawide.right = PaneRole::ClassicSidebar;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.layout.full_hd.sidebar_width = 19;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.layout.breakpoints.tablet_width = 70;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.animation.map_fps = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "bad".to_string(),
            pattern: "[".to_string(),
            commands: vec!["look".to_string()],
            ..AliasRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.aliases.rules.push(AliasRuleConfig {
            name: "empty-command".to_string(),
            pattern: "x".to_string(),
            commands: vec!["".to_string()],
            ..AliasRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.triggers.max_commands_per_line = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.triggers.max_commands_per_line = 1;
        config.triggers.rules.push(TriggerRuleConfig {
            name: "too-many".to_string(),
            pattern: "danger".to_string(),
            commands: vec!["flee".to_string()],
            event: Some("Danger".to_string()),
            ..TriggerRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.events.max_dispatch_depth = 0;
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "empty".to_string(),
            event: "LowHealth".to_string(),
            ..EventHandlerConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.events.handlers.push(EventHandlerConfig {
            name: "bad-regex".to_string(),
            match_type: MatchType::Regex,
            event: "[".to_string(),
            commands: vec!["flee".to_string()],
            ..EventHandlerConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "bad-event-capture".to_string(),
            match_type: MatchType::Regex,
            pattern: "^You are wounded$".to_string(),
            event: Some("LowHealth:{1}".to_string()),
            ..TriggerRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.triggers.rules.push(TriggerRuleConfig {
            name: "bad-trigger".to_string(),
            match_type: MatchType::Regex,
            pattern: "[".to_string(),
            ..TriggerRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.highlights.rules.push(HighlightRuleConfig {
            name: "bad-highlight".to_string(),
            match_type: MatchType::Regex,
            pattern: "[".to_string(),
            ..HighlightRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.highlights.rules.push(HighlightRuleConfig {
            name: "bad-color".to_string(),
            pattern: "danger".to_string(),
            foreground: Some("ultraviolet".to_string()),
            ..HighlightRuleConfig::default()
        });
        assert!(config.validate().is_err());

        let mut config = AppConfig::default();
        config.highlights.rules.push(HighlightRuleConfig {
            name: "no-style".to_string(),
            pattern: "danger".to_string(),
            ..HighlightRuleConfig::default()
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn root_default_config_parses_and_validates() {
        let config: AppConfig =
            toml::from_str(include_str!("../config.toml")).expect("root config should parse");

        config.validate().expect("root config should validate");
    }

    #[test]
    fn default_route_terrain_uses_white_map_color() {
        let config = AppConfig::default();

        assert_eq!(config.map.terrain["City"].color, "#ffffff");
        assert_eq!(config.map.terrain["Road"].color, "#ffffff");
    }

    #[test]
    fn default_map_flag_colors_use_tokyo_night_palette() {
        let config = AppConfig::default();

        assert_eq!(config.map.avoid_color, "#e0af68");
        assert_eq!(config.map.block_color, "#f7768e");
        assert_eq!(config.map.hide_color, "#565f89");
        assert_eq!(config.map.invis_color, "#7aa2f7");
        assert_eq!(config.map.fog_color, "#89ddff");
        assert_eq!(config.map.void_color, "#414868");
    }
}
