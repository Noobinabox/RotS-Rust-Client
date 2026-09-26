use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use mlua::{Error as LuaError, Function, HookTriggers, Lua, Table, Value, VmState};

use crate::{
    color::{AnsiColor, AnsiColors},
    config::LuaConfig,
    map::{DoorState, Room},
    network::msdp::MsdpValue,
    scripting::variables::{VariableEntry, VariableStore},
    state::{AppState, OutputCategory},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaAction {
    Send(String),
    Execute(String),
    Echo(String, Option<LuaOutputOptions>),
    Notify(String, Option<LuaOutputOptions>),
    Log(LuaLogLevel, String),
    SetVariable(String, String),
    UnsetVariable(String),
    EmitEvent(String, Option<String>),
    LocalCommand(String),
    SetTimer {
        name: String,
        interval_ms: u64,
        callback: String,
        repeat: bool,
    },
    CancelTimer(String),
    ReplaceLine(String),
    GagLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LuaOutputOptions {
    pub foreground: Option<String>,
    pub background: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LuaHookContext {
    pub kind: String,
    pub input: Option<String>,
    pub line: Option<String>,
    pub raw_line: Option<String>,
    pub event: Option<String>,
    pub source: Option<String>,
    pub category: Option<OutputCategory>,
    pub captures: Vec<String>,
    pub colors: Option<AnsiColors>,
    pub timer: Option<String>,
    pub tick_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaCallResult {
    pub actions: Vec<LuaAction>,
}

#[derive(Debug, Clone, Default)]
struct LuaSnapshot {
    variables: BTreeMap<String, String>,
    msdp: BTreeMap<String, MsdpValue>,
    character: BTreeMap<String, String>,
    opponent: BTreeMap<String, String>,
    group: Vec<BTreeMap<String, String>>,
    room: Option<LuaRoomSnapshot>,
    output: Vec<String>,
    events: Vec<(String, String)>,
    world: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct LuaRoomSnapshot {
    id: String,
    name: String,
    terrain: String,
    weight: String,
    exits: Vec<String>,
    x: i32,
    y: i32,
    z: i32,
}

pub struct LuaEngine {
    enabled: bool,
    lua: Lua,
    actions: Rc<LuaActionQueue>,
    snapshot: Rc<RefCell<LuaSnapshot>>,
    instruction_budget: u32,
    loaded_paths: Vec<PathBuf>,
    last_error: Option<String>,
}

impl LuaEngine {
    pub fn new(
        config: &LuaConfig,
        config_path: Option<&Path>,
        state: &AppState,
        variables: &VariableStore,
    ) -> Self {
        let mut engine = Self::empty(config, state, variables);
        if config.enabled
            && let Err(error) = engine.load_entrypoint(config, config_path)
        {
            engine.enabled = false;
            engine.actions.clear();
            engine.last_error = Some(error);
        }
        engine
    }

    pub fn disabled() -> Self {
        let config = LuaConfig {
            enabled: false,
            ..LuaConfig::default()
        };
        Self::empty(
            &config,
            &AppState::default_for_lua(),
            &VariableStore::empty(),
        )
    }

    fn empty(config: &LuaConfig, state: &AppState, variables: &VariableStore) -> Self {
        let lua = Lua::new();
        let actions = Rc::new(LuaActionQueue::new(config.max_actions_per_hook));
        let snapshot = Rc::new(RefCell::new(snapshot_from_state(state, variables)));
        let mut engine = Self {
            enabled: config.enabled,
            lua,
            actions,
            snapshot,
            instruction_budget: config.instruction_budget,
            loaded_paths: Vec::new(),
            last_error: None,
        };
        if let Err(error) = engine.install_client_api() {
            engine.last_error = Some(error.to_string());
            engine.enabled = false;
        }
        engine
    }

    pub fn reload(
        &mut self,
        config: &LuaConfig,
        config_path: Option<&Path>,
        state: &AppState,
        variables: &VariableStore,
    ) -> Result<String, String> {
        let mut replacement = Self::empty(config, state, variables);
        if !config.enabled {
            *self = replacement;
            return Ok("Lua disabled.".to_string());
        }
        replacement.load_entrypoint(config, config_path)?;
        *self = replacement;
        Ok(self.status())
    }

    pub fn status(&self) -> String {
        if !self.enabled {
            return match &self.last_error {
                Some(error) => format!("Lua disabled.\nLast error: {error}"),
                None => "Lua disabled.".to_string(),
            };
        }
        let loaded = self
            .loaded_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let loaded = if loaded.is_empty() {
            "no scripts loaded"
        } else {
            &loaded
        };
        match &self.last_error {
            Some(error) => format!("Lua enabled.\nLoaded: {loaded}\nLast error: {error}"),
            None => format!("Lua enabled.\nLoaded: {loaded}"),
        }
    }

    pub fn call_hook(
        &mut self,
        function: &str,
        context: LuaHookContext,
        state: &AppState,
        variables: &VariableStore,
    ) -> Result<LuaCallResult, String> {
        if !self.enabled {
            return Ok(LuaCallResult {
                actions: Vec::new(),
            });
        }
        *self.snapshot.borrow_mut() = snapshot_from_state(state, variables);
        self.actions.clear();
        let allow_line_edits = context.kind == "trigger" && context.line.is_some();
        let function_value = self
            .lua
            .globals()
            .get::<Value>(function)
            .map_err(|error| format!("Lua hook `{function}` lookup failed: {error}"))?;
        let Value::Function(function_value) = function_value else {
            return Err(format!("Lua hook `{function}` is not a function"));
        };
        let context = self
            .context_table(context)
            .map_err(|error| format!("Lua hook `{function}` context failed: {error}"))?;
        let remaining = Rc::new(Cell::new(self.instruction_budget));
        self.lua
            .set_hook(HookTriggers::new().every_nth_instruction(1), {
                let remaining = Rc::clone(&remaining);
                move |_, _| {
                    let next = remaining.get().saturating_sub(1);
                    remaining.set(next);
                    if next == 0 {
                        Err(LuaError::RuntimeError(
                            "Lua instruction budget exceeded".to_string(),
                        ))
                    } else {
                        Ok(VmState::Continue)
                    }
                }
            })
            .map_err(|error| format!("Lua hook `{function}` budget setup failed: {error}"))?;
        self.actions.allow_line_edits.set(allow_line_edits);
        let call_result = function_value.call::<()>(context);
        self.lua.remove_hook();
        self.actions.allow_line_edits.set(false);
        if let Err(error) = call_result {
            self.actions.clear();
            return Err(format!("Lua hook `{function}` failed: {error}"));
        }
        Ok(LuaCallResult {
            actions: self.actions.take(),
        })
    }

    fn load_entrypoint(
        &mut self,
        config: &LuaConfig,
        config_path: Option<&Path>,
    ) -> Result<(), String> {
        if let Some(error) = &self.last_error {
            return Err(error.clone());
        }
        let names = config
            .scripts
            .clone()
            .unwrap_or_else(|| vec![config.entrypoint.clone()]);
        if names.is_empty() || names.iter().any(|name| name.trim().is_empty()) {
            return Err("Lua scripts must contain at least one nonempty script path".into());
        }
        let remaining = Rc::new(Cell::new(self.instruction_budget));
        self.lua
            .set_hook(HookTriggers::new().every_nth_instruction(1), move |_, _| {
                let next = remaining.get().saturating_sub(1);
                remaining.set(next);
                if next == 0 {
                    Err(LuaError::RuntimeError(
                        "Lua instruction budget exceeded".to_string(),
                    ))
                } else {
                    Ok(VmState::Continue)
                }
            })
            .map_err(|error| format!("Lua script budget setup failed: {error}"))?;
        let result: Result<(), String> = (|| {
            for name in names {
                let path = lua_script_path(config, config_path, &name);
                if config.scripts.is_none() && !path.exists() {
                    continue;
                }
                if path.file_name().and_then(|name| name.to_str()) == Some("bot.lua") {
                    let companion = path.with_file_name("bot_areas.lua");
                    if companion.exists() {
                        self.load_script(&companion)?;
                    }
                }
                self.load_script(&path)?;
            }
            Ok(())
        })();
        self.lua.remove_hook();
        self.actions.clear();
        result?;
        self.last_error = None;
        Ok(())
    }

    fn load_script(&mut self, path: &Path) -> Result<(), String> {
        let path = fs::canonicalize(path)
            .map_err(|error| format!("failed to locate Lua script {}: {error}", path.display()))?;
        if self.loaded_paths.contains(&path) {
            return Ok(());
        }
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read Lua script {}: {error}", path.display()))?;
        self.lua
            .load(&source)
            .set_name(path.to_string_lossy().as_ref())
            .exec()
            .map_err(|error| format!("failed to execute Lua script {}: {error}", path.display()))?;
        self.loaded_paths.push(path);
        Ok(())
    }

    fn install_client_api(&mut self) -> mlua::Result<()> {
        remove_unsafe_globals(&self.lua)?;
        let client = self.lua.create_table()?;
        client.set(
            "send",
            action_function(&self.lua, &self.actions, LuaAction::Send)?,
        )?;
        client.set(
            "execute",
            action_function(&self.lua, &self.actions, LuaAction::Execute)?,
        )?;
        client.set(
            "send_all",
            self.lua.create_function({
                let actions = Rc::clone(&self.actions);
                move |_, commands: Vec<String>| {
                    actions.extend_send(commands);
                    Ok(())
                }
            })?,
        )?;
        client.set(
            "echo",
            output_function(&self.lua, &self.actions, LuaActionKind::Echo)?,
        )?;
        client.set(
            "notify",
            output_function(&self.lua, &self.actions, LuaActionKind::Notify)?,
        )?;
        client.set("log", log_table(&self.lua, &self.actions)?)?;
        client.set(
            "var",
            variable_table(&self.lua, &self.actions, &self.snapshot)?,
        )?;
        client.set("msdp", msdp_table(&self.lua, &self.snapshot)?)?;
        client.set(
            "character",
            named_snapshot_table(&self.lua, &self.snapshot, SnapshotKind::Character)?,
        )?;
        client.set(
            "opponent",
            named_snapshot_table(&self.lua, &self.snapshot, SnapshotKind::Opponent)?,
        )?;
        client.set("group", group_table(&self.lua, &self.snapshot)?)?;
        client.set("room", room_table(&self.lua, &self.snapshot)?)?;
        let output = output_read_table(&self.lua, &self.snapshot)?;
        output.set(
            "replace",
            self.lua.create_function({
                let actions = Rc::clone(&self.actions);
                move |_, text: String| {
                    actions.require_line_context()?;
                    if text.len() > 65_536 || text.contains(['\r', '\n']) {
                        return Err(LuaError::RuntimeError(
                            "output replacement must be a single line of at most 65536 bytes"
                                .into(),
                        ));
                    }
                    actions.push(LuaAction::ReplaceLine(text));
                    Ok(())
                }
            })?,
        )?;
        output.set(
            "gag",
            self.lua.create_function({
                let actions = Rc::clone(&self.actions);
                move |_, ()| {
                    actions.require_line_context()?;
                    actions.push(LuaAction::GagLine);
                    Ok(())
                }
            })?,
        )?;
        client.set("output", output)?;
        client.set(
            "event",
            event_table(&self.lua, &self.actions, &self.snapshot)?,
        )?;
        client.set("map", map_table(&self.lua, &self.actions, &self.snapshot)?)?;
        client.set("ui", ui_table(&self.lua, &self.actions)?)?;
        client.set("time", time_table(&self.lua)?)?;
        client.set("timer", timer_table(&self.lua, &self.actions)?)?;
        self.lua.globals().set("client", client)?;
        Ok(())
    }

    fn context_table(&self, context: LuaHookContext) -> mlua::Result<Table> {
        let table = self.lua.create_table()?;
        table.set("kind", context.kind)?;
        if let Some(input) = context.input {
            table.set("input", input)?;
        }
        if let Some(line) = context.line {
            table.set("line", line)?;
        }
        if let Some(raw_line) = context.raw_line {
            table.set("raw_line", raw_line)?;
        }
        if let Some(event) = context.event {
            table.set("event", event)?;
        }
        if let Some(source) = context.source {
            table.set("source", source)?;
        }
        if let Some(category) = context.category {
            table.set("category", format!("{category:?}"))?;
        }
        table.set("captures", vec_to_lua_array(&self.lua, context.captures)?)?;
        if let Some(timer) = context.timer {
            table.set("timer", timer)?;
        }
        if let Some(tick_count) = context.tick_count {
            table.set("tick_count", tick_count)?;
        }
        if let Some(colors) = context.colors {
            let color_table = self.lua.create_table()?;
            color_table.set(
                "foregrounds",
                ansi_colors_to_table(&self.lua, colors.foregrounds)?,
            )?;
            color_table.set(
                "backgrounds",
                ansi_colors_to_table(&self.lua, colors.backgrounds)?,
            )?;
            table.set("colors", color_table)?;
        }
        Ok(table)
    }
}

fn remove_unsafe_globals(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in [
        "os", "io", "package", "debug", "require", "dofile", "loadfile",
    ] {
        globals.set(name, Value::Nil)?;
    }
    Ok(())
}

#[cfg(test)]
fn lua_entrypoint_path(config: &LuaConfig, config_path: Option<&Path>) -> PathBuf {
    lua_script_path(config, config_path, &config.entrypoint)
}

fn lua_script_path(config: &LuaConfig, config_path: Option<&Path>, name: &str) -> PathBuf {
    let script_dir = PathBuf::from(&config.script_dir);
    let base = if script_dir.is_absolute() {
        script_dir
    } else if let Some(parent) = config_path.and_then(Path::parent) {
        parent.join(script_dir)
    } else if let Some(config_dir) = ProjectDirs::from("org", "mud-client", "mud-client")
        .map(|dirs| dirs.config_dir().to_path_buf())
    {
        config_dir.join(script_dir)
    } else {
        script_dir
    };
    base.join(name)
}

#[derive(Debug)]
struct LuaActionQueue {
    actions: RefCell<Vec<LuaAction>>,
    max_actions: usize,
    allow_line_edits: Cell<bool>,
}

impl LuaActionQueue {
    fn new(max_actions: usize) -> Self {
        Self {
            actions: RefCell::new(Vec::new()),
            max_actions,
            allow_line_edits: Cell::new(false),
        }
    }

    fn require_line_context(&self) -> mlua::Result<()> {
        if self.allow_line_edits.get() {
            Ok(())
        } else {
            Err(LuaError::RuntimeError(
                "client.output edits require an incoming-line trigger context".into(),
            ))
        }
    }

    fn push(&self, action: LuaAction) {
        let mut actions = self.actions.borrow_mut();
        if actions.len() < self.max_actions {
            actions.push(action);
        }
    }

    fn extend_send(&self, commands: Vec<String>) {
        for command in commands {
            self.push(LuaAction::Send(command));
        }
    }

    fn clear(&self) {
        self.actions.borrow_mut().clear();
    }

    fn take(&self) -> Vec<LuaAction> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }
}

fn action_function(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    build: fn(String) -> LuaAction,
) -> mlua::Result<Function> {
    lua.create_function({
        let actions = Rc::clone(actions);
        move |_, text: String| {
            actions.push(build(text));
            Ok(())
        }
    })
}

#[derive(Clone, Copy)]
enum LuaActionKind {
    Echo,
    Notify,
}

fn output_function(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    kind: LuaActionKind,
) -> mlua::Result<Function> {
    lua.create_function({
        let actions = Rc::clone(actions);
        move |_, (text, opts): (String, Option<Table>)| {
            let opts = opts.map(lua_output_options).transpose()?;
            actions.push(match kind {
                LuaActionKind::Echo => LuaAction::Echo(text, opts),
                LuaActionKind::Notify => LuaAction::Notify(text, opts),
            });
            Ok(())
        }
    })
}

fn lua_output_options(table: Table) -> mlua::Result<LuaOutputOptions> {
    Ok(LuaOutputOptions {
        foreground: table.get::<Option<String>>("foreground")?,
        background: table.get::<Option<String>>("background")?,
    })
}

fn log_table(lua: &Lua, actions: &Rc<LuaActionQueue>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (name, level) in [
        ("debug", LuaLogLevel::Debug),
        ("info", LuaLogLevel::Info),
        ("warn", LuaLogLevel::Warn),
        ("error", LuaLogLevel::Error),
    ] {
        table.set(
            name,
            lua.create_function({
                let actions = Rc::clone(actions);
                move |_, message: String| {
                    actions.push(LuaAction::Log(level.clone(), message));
                    Ok(())
                }
            })?,
        )?;
    }
    Ok(table)
}

fn variable_table(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    snapshot: &Rc<RefCell<LuaSnapshot>>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "get",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |_, name: String| Ok(snapshot.borrow().variables.get(&name).cloned())
        })?,
    )?;
    table.set(
        "set",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, (name, value): (String, String)| {
                actions.push(LuaAction::SetVariable(name, value));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "unset",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, name: String| {
                actions.push(LuaAction::UnsetVariable(name));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "list",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| map_to_table(lua, &snapshot.borrow().variables)
        })?,
    )?;
    Ok(table)
}

fn msdp_table(lua: &Lua, snapshot: &Rc<RefCell<LuaSnapshot>>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "get",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, name: String| {
                let snapshot = snapshot.borrow();
                match snapshot.msdp.get(&name) {
                    Some(value) => msdp_to_lua(lua, value),
                    None => Ok(Value::Nil),
                }
            }
        })?,
    )?;
    table.set(
        "all",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| {
                let table = lua.create_table()?;
                for (key, value) in &snapshot.borrow().msdp {
                    table.set(key.as_str(), msdp_to_lua(lua, value)?)?;
                }
                Ok(table)
            }
        })?,
    )?;
    Ok(table)
}

#[derive(Clone, Copy)]
enum SnapshotKind {
    Character,
    Opponent,
}

fn named_snapshot_table(
    lua: &Lua,
    snapshot: &Rc<RefCell<LuaSnapshot>>,
    kind: SnapshotKind,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "get",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| {
                let snapshot = snapshot.borrow();
                let values = match kind {
                    SnapshotKind::Character => &snapshot.character,
                    SnapshotKind::Opponent => &snapshot.opponent,
                };
                map_to_table(lua, values)
            }
        })?,
    )?;
    Ok(table)
}

fn group_table(lua: &Lua, snapshot: &Rc<RefCell<LuaSnapshot>>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "list",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| {
                let outer = lua.create_table()?;
                for (index, member) in snapshot.borrow().group.iter().enumerate() {
                    outer.set(index + 1, map_to_table(lua, member)?)?;
                }
                Ok(outer)
            }
        })?,
    )?;
    Ok(table)
}

fn room_table(lua: &Lua, snapshot: &Rc<RefCell<LuaSnapshot>>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "current",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| match &snapshot.borrow().room {
                Some(room) => room_to_table(lua, room).map(Value::Table),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    Ok(table)
}

fn output_read_table(lua: &Lua, snapshot: &Rc<RefCell<LuaSnapshot>>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "recent",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, limit: Option<usize>| {
                let snapshot = snapshot.borrow();
                let limit = limit.unwrap_or(20).min(snapshot.output.len());
                let lines = snapshot
                    .output
                    .iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                vec_to_lua_array(lua, lines.into_iter().rev().collect())
            }
        })?,
    )?;
    table.set(
        "search",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, pattern: String| {
                let matches = snapshot
                    .borrow()
                    .output
                    .iter()
                    .filter(|line| line.contains(&pattern))
                    .cloned()
                    .collect();
                vec_to_lua_array(lua, matches)
            }
        })?,
    )?;
    Ok(table)
}

fn event_table(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    snapshot: &Rc<RefCell<LuaSnapshot>>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "emit",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, (name, source): (String, Option<String>)| {
                actions.push(LuaAction::EmitEvent(name, source));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "recent",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, limit: Option<usize>| {
                let snapshot = snapshot.borrow();
                let limit = limit.unwrap_or(10).min(snapshot.events.len());
                let result = lua.create_table()?;
                for (index, (name, source)) in snapshot.events.iter().rev().take(limit).enumerate()
                {
                    let event = lua.create_table()?;
                    event.set("name", name.as_str())?;
                    event.set("source", source.as_str())?;
                    result.set(index + 1, event)?;
                }
                Ok(result)
            }
        })?,
    )?;
    Ok(table)
}

fn map_table(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    snapshot: &Rc<RefCell<LuaSnapshot>>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "current",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, ()| match &snapshot.borrow().room {
                Some(room) => room_to_table(lua, room).map(Value::Table),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    table.set(
        "get",
        lua.create_function({
            let snapshot = Rc::clone(snapshot);
            move |lua, _room_id: Option<String>| match &snapshot.borrow().room {
                Some(room) => room_to_table(lua, room).map(Value::Table),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    for (name, prefix) in [
        ("find", "/map find"),
        ("goto", "/map goto"),
        ("run", "/map run"),
    ] {
        table.set(name, local_command_function(lua, actions, prefix)?)?;
    }
    table.set(
        "door",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, (direction, state, name): (String, String, Option<String>)| {
                let mut command = format!("/map door {direction} {state}");
                if let Some(name) = name {
                    let name = name.trim();
                    if !name.is_empty() {
                        command.push(' ');
                        command.push_str(name);
                    }
                }
                actions.push(LuaAction::LocalCommand(command));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "set_terrain",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, terrain: String| {
                actions.push(LuaAction::LocalCommand(format!(
                    "/map set terrain {terrain}"
                )));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "set_weight",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, weight: f32| {
                actions.push(LuaAction::LocalCommand(format!("/map set weight {weight}")));
                Ok(())
            }
        })?,
    )?;
    Ok(table)
}

fn ui_table(lua: &Lua, actions: &Rc<LuaActionQueue>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "toggle",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, (panel, state): (String, Option<String>)| {
                let command = match state {
                    Some(state) => format!("/toggle {panel} {state}"),
                    None => format!("/toggle {panel}"),
                };
                actions.push(LuaAction::LocalCommand(command));
                Ok(())
            }
        })?,
    )?;
    table.set(
        "reload",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, ()| {
                actions.push(LuaAction::LocalCommand("/reload".to_string()));
                Ok(())
            }
        })?,
    )?;
    Ok(table)
}

fn time_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "now_ms",
        lua.create_function(|_, ()| {
            Ok(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default())
        })?,
    )?;
    Ok(table)
}

fn timer_table(lua: &Lua, actions: &Rc<LuaActionQueue>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "set",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, (name, interval_ms, callback, repeat): (String, u64, String, Option<bool>)| {
                actions.push(LuaAction::SetTimer {
                    name,
                    interval_ms,
                    callback,
                    repeat: repeat.unwrap_or(true),
                });
                Ok(())
            }
        })?,
    )?;
    table.set(
        "cancel",
        lua.create_function({
            let actions = Rc::clone(actions);
            move |_, name: String| {
                actions.push(LuaAction::CancelTimer(name));
                Ok(())
            }
        })?,
    )?;
    Ok(table)
}

fn local_command_function(
    lua: &Lua,
    actions: &Rc<LuaActionQueue>,
    prefix: &'static str,
) -> mlua::Result<Function> {
    lua.create_function({
        let actions = Rc::clone(actions);
        move |_, target: String| {
            actions.push(LuaAction::LocalCommand(format!("{prefix} {target}")));
            Ok(())
        }
    })
}

fn snapshot_from_state(state: &AppState, variables: &VariableStore) -> LuaSnapshot {
    let mut snapshot = LuaSnapshot {
        variables: variables
            .entries()
            .unwrap_or_default()
            .into_iter()
            .map(|VariableEntry { name, value, .. }| (name, value))
            .collect(),
        msdp: state.raw_msdp.clone().into_iter().collect(),
        output: state
            .output
            .iter()
            .map(|line| {
                if line.source_id.is_some() {
                    crate::state::plain_text(&line.raw)
                } else {
                    line.normalized.clone()
                }
            })
            .collect(),
        events: state
            .script_events
            .iter()
            .map(|event| (event.name.clone(), event.source.clone()))
            .collect(),
        ..LuaSnapshot::default()
    };
    insert_optional(
        &mut snapshot.character,
        "name",
        state.character.name.as_deref(),
    );
    insert_optional(
        &mut snapshot.character,
        "race",
        state.character.race.as_deref(),
    );
    insert_optional_i64(&mut snapshot.character, "level", state.character.level);
    insert_optional_i64(&mut snapshot.character, "health", state.character.health);
    insert_optional_i64(
        &mut snapshot.character,
        "health_max",
        state.character.health_max,
    );
    insert_optional_i64(&mut snapshot.character, "mana", state.character.mana);
    insert_optional_i64(
        &mut snapshot.character,
        "mana_max",
        state.character.mana_max,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "movement",
        state.character.movement,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "movement_max",
        state.character.movement_max,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "experience",
        state.character.experience,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "experience_max",
        state.character.experience_max,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "ob",
        state.character.offensive_bonus,
    );
    insert_optional_i64(&mut snapshot.character, "db", state.character.dodge);
    insert_optional_i64(&mut snapshot.character, "pb", state.character.parry);
    insert_optional_i64(
        &mut snapshot.character,
        "attack_speed",
        state.character.attack_speed,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "strength",
        state.character.strength,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "intelligence",
        state.character.intelligence,
    );
    insert_optional_i64(&mut snapshot.character, "will", state.character.will);
    insert_optional_i64(
        &mut snapshot.character,
        "dexterity",
        state.character.dexterity,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "constitution",
        state.character.constitution,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "learning",
        state.character.learning,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "willpower",
        state.character.willpower,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "spell_save",
        state.character.spell_save,
    );
    insert_optional_i64(&mut snapshot.character, "spirit", state.character.spirit);
    insert_optional_i64(
        &mut snapshot.character,
        "spell_power",
        state.character.spell_power,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "spell_pen",
        state.character.spell_pen,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "warrior_level",
        state.character.warrior_level,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "ranger_level",
        state.character.ranger_level,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "mystic_level",
        state.character.mystic_level,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "mage_level",
        state.character.mage_level,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "health_regeneration",
        state.character.health_regeneration,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "stamina_regeneration",
        state.character.stamina_regeneration,
    );
    insert_optional_i64(
        &mut snapshot.character,
        "movement_regeneration",
        state.character.movement_regeneration,
    );
    insert_optional(
        &mut snapshot.opponent,
        "name",
        state.opponent.name.as_deref(),
    );
    insert_optional(
        &mut snapshot.opponent,
        "level",
        state.opponent.level.as_deref(),
    );
    insert_optional_i64(&mut snapshot.opponent, "health", state.opponent.health);
    insert_optional_i64(
        &mut snapshot.opponent,
        "health_max",
        state.opponent.health_max,
    );
    for member in &state.group.members {
        let mut row = BTreeMap::new();
        row.insert("name".to_string(), member.name.clone());
        insert_optional_i64(&mut row, "health_percent", member.health_percent);
        insert_optional_i64(&mut row, "mana_percent", member.mana_percent);
        insert_optional_i64(&mut row, "movement_percent", member.movement_percent);
        snapshot.group.push(row);
    }
    insert_optional(&mut snapshot.world, "time", state.world.time.as_deref());
    insert_optional(
        &mut snapshot.world,
        "weather",
        state.world.weather.as_deref(),
    );
    snapshot.room = state
        .map
        .current_room
        .as_ref()
        .and_then(|id| state.map.rooms.get(id))
        .map(room_snapshot);
    snapshot
}

fn room_snapshot(room: &Room) -> LuaRoomSnapshot {
    LuaRoomSnapshot {
        id: room.id.clone(),
        name: room.name.clone(),
        terrain: room.terrain.clone(),
        weight: room.weight.to_string(),
        exits: room.exit_order.clone(),
        x: room.x,
        y: room.y,
        z: room.z,
    }
}

fn insert_optional(map: &mut BTreeMap<String, String>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(name.to_string(), value.to_string());
    }
}

fn insert_optional_i64(map: &mut BTreeMap<String, String>, name: &str, value: Option<i64>) {
    if let Some(value) = value {
        map.insert(name.to_string(), value.to_string());
    }
}

fn map_to_table(lua: &Lua, values: &BTreeMap<String, String>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in values {
        table.set(key.as_str(), value.as_str())?;
    }
    Ok(table)
}

fn room_to_table(lua: &Lua, room: &LuaRoomSnapshot) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", room.id.as_str())?;
    table.set("name", room.name.as_str())?;
    table.set("terrain", room.terrain.as_str())?;
    table.set("weight", room.weight.as_str())?;
    table.set("x", room.x)?;
    table.set("y", room.y)?;
    table.set("z", room.z)?;
    table.set("exits", vec_to_lua_array(lua, room.exits.clone())?)?;
    Ok(table)
}

fn vec_to_lua_array(lua: &Lua, values: Vec<String>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.into_iter().enumerate() {
        table.set(index + 1, value)?;
    }
    Ok(table)
}

fn ansi_colors_to_table(lua: &Lua, values: Vec<AnsiColor>) -> mlua::Result<Table> {
    vec_to_lua_array(
        lua,
        values
            .into_iter()
            .map(|color| format!("{color:?}").to_lowercase())
            .collect(),
    )
}

fn msdp_to_lua(lua: &Lua, value: &MsdpValue) -> mlua::Result<Value> {
    match value {
        MsdpValue::String(value) => Ok(Value::String(lua.create_string(value)?)),
        MsdpValue::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.iter().enumerate() {
                table.set(index + 1, msdp_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        MsdpValue::Table(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key.as_str(), msdp_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

trait LuaDefaultState {
    fn default_for_lua() -> Self;
}

impl LuaDefaultState for AppState {
    fn default_for_lua() -> Self {
        AppState::new(&crate::config::AppConfig::default())
    }
}

impl DoorState {
    #[allow(dead_code)]
    fn lua_name(self) -> &'static str {
        match self {
            DoorState::Open => "open",
            DoorState::Closed => "closed",
            DoorState::Pickable => "pickable",
            DoorState::Locked => "locked",
            DoorState::Trigger => "trigger",
            DoorState::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        config::{AppConfig, LuaConfig},
        network::msdp::MsdpValue,
        scripting::variables::VariableStore,
        state::AppState,
    };

    use super::{LuaAction, LuaEngine, LuaHookContext, lua_entrypoint_path};

    #[test]
    fn lua_execute_is_distinct_from_direct_send() {
        let (config, path) = lua_config_with_script(
            "function commands() client.execute('p 1.orc'); client.send('p 1.orc') end",
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        let result = engine
            .call_hook("commands", LuaHookContext::default(), &state, &variables)
            .unwrap();
        assert_eq!(
            result.actions,
            vec![
                LuaAction::Execute("p 1.orc".into()),
                LuaAction::Send("p 1.orc".into())
            ]
        );
    }

    #[test]
    fn lua_hook_collects_client_actions() {
        let (config, config_path) = lua_config_with_script(
            r#"
function test_hook(ctx)
  client.echo("ctx:" .. ctx.kind, { foreground = "yellow" })
  client.send("look")
  client.var.set("target", "orc")
  client.event.emit("Ready", "lua-test")
end
"#,
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, Some(&config_path), &state, &variables);

        let result = lua
            .call_hook(
                "test_hook",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect("hook should run");

        assert_eq!(result.actions.len(), 4);
        assert!(matches!(result.actions[0], LuaAction::Echo(_, _)));
        assert_eq!(result.actions[1], LuaAction::Send("look".to_string()));
        assert_eq!(
            result.actions[2],
            LuaAction::SetVariable("target".to_string(), "orc".to_string())
        );
        assert_eq!(
            result.actions[3],
            LuaAction::EmitEvent("Ready".to_string(), Some("lua-test".to_string()))
        );
    }

    #[test]
    fn relative_lua_scripts_without_config_path_use_platform_config_directory() {
        let config = LuaConfig::default();
        let expected_base = directories::ProjectDirs::from("org", "mud-client", "mud-client")
            .expect("test platform should provide a config directory")
            .config_dir()
            .join("scripts")
            .join("init.lua");

        assert_eq!(lua_entrypoint_path(&config, None), expected_base);
    }

    #[test]
    fn lua_hook_can_read_sandboxed_state() {
        let (config, config_path) = lua_config_with_script(
            r#"
function test_hook(ctx)
  if os ~= nil or io ~= nil or package ~= nil or debug ~= nil then
    error("unsafe globals exposed")
  end
  local room = client.room.current()
  local value = client.msdp.get("ROOM_NAME")
  client.echo(room.name .. ":" .. value)
end
"#,
        );
        let mut state = AppState::new(&AppConfig::default());
        state.raw_msdp.insert(
            "ROOM_NAME".to_string(),
            MsdpValue::String("A Test Room".to_string()),
        );
        state.map.create();
        let room_id = state.map.current_room.clone().unwrap();
        state.map.rooms.get_mut(&room_id).unwrap().name = "A Test Room".to_string();
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, Some(&config_path), &state, &variables);

        let result = lua
            .call_hook(
                "test_hook",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect("hook should run");

        assert_eq!(
            result.actions[0],
            LuaAction::Echo("A Test Room:A Test Room".to_string(), None)
        );
    }

    #[test]
    fn lua_map_door_accepts_optional_name() {
        let (config, config_path) = lua_config_with_script(
            r#"
function test_hook(ctx)
  client.map.door("w", "locked", "iron gate")
end
"#,
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, Some(&config_path), &state, &variables);

        let result = lua
            .call_hook(
                "test_hook",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect("hook should run");

        assert_eq!(
            result.actions[0],
            LuaAction::LocalCommand("/map door w locked iron gate".to_string())
        );
    }

    #[test]
    fn disabled_lua_ignores_hooks() {
        let config = LuaConfig {
            enabled: false,
            ..LuaConfig::default()
        };
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, None, &state, &variables);

        let result = lua
            .call_hook(
                "missing",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect("disabled hooks should be ignored");

        assert!(result.actions.is_empty());
    }

    #[test]
    fn lua_runtime_error_is_returned() {
        let (config, config_path) = lua_config_with_script(
            r#"
function bad_hook(ctx)
  error("boom")
end
"#,
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, Some(&config_path), &state, &variables);

        let error = lua
            .call_hook(
                "bad_hook",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect_err("runtime error should be returned");

        assert!(error.contains("boom"));
    }

    #[test]
    fn lua_instruction_budget_stops_runaway_hooks() {
        let (mut config, config_path) = lua_config_with_script(
            r#"
function runaway(ctx)
  while true do end
end
"#,
        );
        config.instruction_budget = 64;
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut lua = LuaEngine::new(&config, Some(&config_path), &state, &variables);

        let error = lua
            .call_hook(
                "runaway",
                LuaHookContext {
                    kind: "manual".to_string(),
                    ..LuaHookContext::default()
                },
                &state,
                &variables,
            )
            .expect_err("budget should stop runaway hooks");

        assert!(error.contains("instruction budget"));
    }

    #[test]
    fn ordered_scripts_share_globals_and_deduplicate_companions() {
        let (mut config, path) = lua_config_with_script("error('legacy must not run')");
        let directory = std::path::Path::new(&config.script_dir);
        fs::write(directory.join("bot_areas.lua"), "count = (count or 0) + 1").unwrap();
        fs::write(
            directory.join("bot.lua"),
            "function check() client.echo(tostring(count)) end",
        )
        .unwrap();
        config.scripts = Some(vec![
            "bot_areas.lua".into(),
            "./bot_areas.lua".into(),
            "bot.lua".into(),
        ]);
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        let result = engine
            .call_hook("check", LuaHookContext::default(), &state, &variables)
            .unwrap();
        assert_eq!(result.actions, vec![LuaAction::Echo("1".into(), None)]);
        assert_eq!(engine.loaded_paths.len(), 2);
        assert!(engine.status().contains("bot.lua"));
        assert!(engine.status().contains("bot_areas.lua"));
    }

    #[test]
    fn failed_reload_preserves_previous_functions() {
        let (mut config, path) = lua_config_with_script("function check() client.echo('old') end");
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        config.scripts = Some(vec!["missing.lua".into()]);
        assert!(
            engine
                .reload(&config, Some(&path), &state, &variables)
                .is_err()
        );
        let result = engine
            .call_hook("check", LuaHookContext::default(), &state, &variables)
            .unwrap();
        assert_eq!(result.actions, vec![LuaAction::Echo("old".into(), None)]);
    }

    #[test]
    fn startup_failure_disables_partial_functions_and_actions() {
        let (config, path) = lua_config_with_script(
            "function check() client.send('unsafe') end; client.send('queued'); error('failed')",
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        assert!(engine.status().contains("Lua disabled."));
        assert!(engine.status().contains("failed"));
        assert!(engine.actions.take().is_empty());
        assert!(
            engine
                .call_hook("check", LuaHookContext::default(), &state, &variables)
                .unwrap()
                .actions
                .is_empty()
        );
    }

    #[test]
    fn script_loading_has_instruction_budget() {
        let (mut config, path) = lua_config_with_script("while true do end");
        config.instruction_budget = 64;
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        assert!(engine.status().contains("instruction budget"));
    }

    #[test]
    fn output_edits_require_incoming_trigger_and_failed_hooks_discard_actions() {
        let (config, path) = lua_config_with_script(
            "function edit() client.echo('queued'); client.output.replace('new'); client.output.gag() end; function fail() client.output.gag(); error('failed') end",
        );
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        for kind in ["manual", "alias", "timer", "event"] {
            let context = LuaHookContext {
                kind: kind.into(),
                line: Some("old".into()),
                ..LuaHookContext::default()
            };
            assert!(
                engine
                    .call_hook("edit", context, &state, &variables)
                    .is_err()
            );
            assert!(engine.actions.take().is_empty());
        }
        let context = LuaHookContext {
            kind: "trigger".into(),
            ..LuaHookContext::default()
        };
        assert!(
            engine
                .call_hook("edit", context, &state, &variables)
                .is_err()
        );
        let context = LuaHookContext {
            kind: "trigger".into(),
            line: Some("old".into()),
            ..LuaHookContext::default()
        };
        let result = engine
            .call_hook("edit", context.clone(), &state, &variables)
            .unwrap();
        assert_eq!(
            result.actions,
            vec![
                LuaAction::Echo("queued".into(), None),
                LuaAction::ReplaceLine("new".into()),
                LuaAction::GagLine
            ]
        );
        assert!(
            engine
                .call_hook("fail", context, &state, &variables)
                .is_err()
        );
        assert!(engine.actions.take().is_empty());
    }

    #[test]
    fn script_list_validation_rejects_empty_paths_and_allows_entrypoint_override() {
        let mut config = AppConfig::default();
        config.lua.entrypoint.clear();
        config.lua.scripts = Some(vec!["one.lua".into()]);
        assert!(config.validate().is_ok());
        for scripts in [vec![], vec![" ".into()]] {
            config.lua.scripts = Some(scripts);
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn output_replacement_rejects_multiline_and_oversized_text() {
        let (config, path) =
            lua_config_with_script("function edit(ctx) client.output.replace(ctx.line) end");
        let state = AppState::new(&AppConfig::default());
        let variables = VariableStore::empty();
        let mut engine = LuaEngine::new(&config, Some(&path), &state, &variables);
        for line in ["two\nlines".into(), "bad\rline".into(), "x".repeat(65_537)] {
            let context = LuaHookContext {
                kind: "trigger".into(),
                line: Some(line),
                ..LuaHookContext::default()
            };
            assert!(
                engine
                    .call_hook("edit", context, &state, &variables)
                    .is_err()
            );
            assert!(engine.actions.take().is_empty());
        }
        let context = LuaHookContext {
            kind: "trigger".into(),
            line: Some("x".repeat(65_536)),
            ..LuaHookContext::default()
        };
        assert!(
            engine
                .call_hook("edit", context, &state, &variables)
                .is_ok()
        );
    }

    fn lua_config_with_script(source: &str) -> (LuaConfig, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "mud-client-lua-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let script_dir = base.join("scripts");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(script_dir.join("init.lua"), source).unwrap();
        (
            LuaConfig {
                script_dir: script_dir.display().to_string(),
                ..LuaConfig::default()
            },
            base.join("config.toml"),
        )
    }
}
