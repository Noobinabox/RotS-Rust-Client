# Lua API Reference

Lua hooks run from aliases, triggers, event handlers, or `/lua call <function>`. Hooks receive a context table and use the global `client` table to enqueue safe actions. The client applies those actions through the normal command, output, event, variable, mapper, and UI pipelines.

## Setup

Enable Lua in `config.toml`:

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "init.lua"
instruction_budget = 100000
max_actions_per_hook = 32
runtime_errors_to_output = true
```

Attach Lua hooks to configured rules:

```toml
[[aliases.rules]]
name = "smart-kill"
match_type = "regex"
pattern = "^sk\\s+(.+)$"
lua = "smart_kill"

[[triggers.rules]]
name = "enemy-arrives"
match_type = "regex"
pattern = "^(.+) arrives\\.$"
lua = "enemy_arrives"

[[events.handlers]]
name = "low-health"
event = "LowHealth"
lua = "low_health"
```

Reload scripts while the client is running:

```text
/lua reload
/lua status
/lua call smoke_test
```

## Hook Contexts

Alias hooks receive command input and regex captures:

```lua
function smart_kill(ctx)
  -- ctx.kind == "alias"
  -- ctx.input is the typed command
  -- ctx.captures is a 1-based Lua array
  local target = ctx.captures[1]
  client.send("kill " .. target)
end
```

Trigger hooks receive normalized output, raw output, category, captures, and colors:

```lua
function enemy_arrives(ctx)
  -- ctx.kind == "trigger"
  -- ctx.line has visible text without ANSI controls
  -- ctx.raw_line preserves raw line content
  -- ctx.category is "Normal", "Prompt", "Combat", and similar
  -- ctx.colors.foregrounds/backgrounds contain active parsed colors
  local name = ctx.captures[1]
  client.notify("Targeting " .. name, { foreground = "yellow" })
end
```

Event hooks receive the event name, source, and captures:

```lua
function combat_started(ctx)
  -- ctx.kind == "event"
  -- ctx.event is the emitted event name
  -- ctx.source is "manual", "trigger", "lua", or another client source
  local target = ctx.captures[1] or "unknown"
  client.var.set("last_target", target)
end
```

Manual calls receive a manual context:

```lua
function smoke_test(ctx)
  -- ctx.kind == "manual"
  client.echo("Lua is loaded.", { foreground = "green" })
end
```

## Actions

### `client.send(command)`

Queues one MUD command through variables, aliases, door-aware movement, command echo, and network sending.

```lua
client.send("look")
client.send("kill " .. (client.var.get("last_target") or "orc"))
```

### `client.send_all(commands)`

Queues several MUD commands in order.

```lua
client.send_all({ "stand", "open gate w", "w" })
```

### `client.echo(text, opts)`

Adds local output to the MUD output pane. Use it for script messages that should not fire trigger notifications.

```lua
client.echo("No target set.", { foreground = "yellow" })
client.echo("Danger", { foreground = "white", background = "red" })
```

### `client.notify(text, opts)`

Adds a triggered local notification. Use it for automation feedback.

```lua
client.notify("Low health.", { foreground = "red" })
```

### `client.log.debug/info/warn/error(message)`

Queues a tracing log message.

```lua
client.log.info("route hook called")
client.log.warn("missing target variable")
```

## Variables

### `client.var.get(name)`

Reads the effective runtime/config variable.

```lua
local target = client.var.get("last_target") or "orc"
```

### `client.var.set(name, value)`

Sets a runtime variable for this session.

```lua
client.var.set("last_target", "Orc warlord")
```

### `client.var.unset(name)`

Removes a runtime variable.

```lua
client.var.unset("last_target")
```

### `client.var.list()`

Returns a table of effective variables.

```lua
for name, value in pairs(client.var.list()) do
  client.log.debug(name .. "=" .. value)
end
```

## MSDP And Snapshots

### `client.msdp.get(name)`

Reads one raw MSDP value by server variable name.

```lua
local room_name = client.msdp.get("ROOM_NAME")
if room_name ~= nil then
  client.echo("Room: " .. tostring(room_name))
end
```

### `client.msdp.all()`

Returns all stored raw MSDP values.

```lua
for name, value in pairs(client.msdp.all()) do
  client.log.debug(name .. "=" .. tostring(value))
end
```

### `client.character.get()`

Returns the latest character snapshot.

```lua
local ch = client.character.get()
local hp = tonumber(ch.health or "0") or 0
local max_hp = tonumber(ch.health_max or "0") or 0
if max_hp > 0 and hp * 100 <= max_hp * 30 then
  client.send("flee")
end
```

Available character fields include `name`, `race`, `level`, `health`, `health_max`, `mana`, `mana_max`, `movement`, `movement_max`, `experience`, `experience_max`, `ob`, `db`, `pb`, `attack_speed`, `strength`, `intelligence`, `will`, `dexterity`, `constitution`, `learning`, `willpower`, `spell_save`, `spirit`, `spell_power`, `spell_pen`, class levels, and regeneration fields.

### `client.opponent.get()`

Returns the current opponent snapshot.

```lua
local opponent = client.opponent.get()
if opponent.name ~= nil then
  client.echo("Opponent: " .. opponent.name)
end
```

### `client.group.list()`

Returns group member snapshots.

```lua
for _, member in ipairs(client.group.list()) do
  client.log.info(member.name .. " HP " .. tostring(member.health_percent))
end
```

### `client.room.current()`

Returns the current map room snapshot or `nil`.

```lua
local room = client.room.current()
if room ~= nil then
  client.var.set("last_room", room.name)
end
```

## Output

### `client.output.recent(limit)`

Returns recent normalized output lines. The default limit is 20.

```lua
for _, line in ipairs(client.output.recent(5)) do
  client.log.debug(line)
end
```

### `client.output.search(text)`

Returns normalized output lines containing literal text.

```lua
local matches = client.output.search("door")
if #matches > 1 then
  client.notify("Multiple door lines found.", { foreground = "yellow" })
end
```

## Events

### `client.event.emit(name, source)`

Queues a script event.

```lua
client.event.emit("CombatStarted:Orc warlord", "lua")
```

### `client.event.recent(limit)`

Returns recent event records.

```lua
for _, event in ipairs(client.event.recent(3)) do
  client.log.debug(event.name .. " from " .. event.source)
end
```

## Mapper

### `client.map.current()` and `client.map.get(room_id)`

Return a map room snapshot. `get` currently returns the current room snapshot.

```lua
local room = client.map.current()
if room ~= nil then
  client.echo(room.id .. " " .. room.name)
end
```

### `client.map.find(query)`

Queues `/map find <query>`.

```lua
client.map.find("Bree")
```

### `client.map.goto(target)`

Queues `/map goto <target>`.

```lua
client.map.goto("2942")
```

### `client.map.run(target)`

Queues `/map run <target>`.

```lua
client.map.run("Bree")
```

### `client.map.door(direction, state, name)`

Queues `/map door <direction> <state> [name]`.

```lua
client.map.door("w", "closed", "gate")
client.map.door("n", "locked", "iron door")
client.map.door("e", "trigger", "lever")
client.map.door("s", "none")
```

Door states are `trigger`, `unknown`, `open`, `closed`, `pickable`, `locked`, and clear states `none`, `clear`, or `off`.

### `client.map.set_terrain(terrain)`

Queues `/map set terrain <terrain>`.

```lua
client.map.set_terrain("Forest")
```

### `client.map.set_weight(weight)`

Queues `/map set weight <weight>`.

```lua
client.map.set_weight(4)
```

## UI

### `client.ui.toggle(panel, state)`

Queues `/toggle <panel> [state]`.

```lua
client.ui.toggle("group", "on")
client.ui.toggle("opponent", "off")
client.ui.toggle("social")
```

### `client.ui.reload()`

Queues `/reload`.

```lua
client.ui.reload()
```

## Time

### `client.time.now_ms()`

Returns Unix time in milliseconds.

```lua
local started = client.time.now_ms()
client.log.debug("started at " .. tostring(started))
```

## Editor Completion

The repository includes `scripts/mud-client-api.lua` with LuaLS annotations and `.luarc.json` that adds `scripts` as a workspace library. Open the repository root in an editor with Lua Language Server support to get completion for `client.*`, hook context fields, snapshot types, and door state strings.

## Safety Limits

- Filesystem, OS, process, package, and debug globals are disabled.
- Hooks cannot mutate Rust state directly.
- Hooks are stopped by `lua.instruction_budget`.
- Hooks can enqueue only `lua.max_actions_per_hook` actions.
- Runtime errors are shown in output when `runtime_errors_to_output = true`.
