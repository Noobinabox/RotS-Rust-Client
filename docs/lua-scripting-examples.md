# Lua Scripting Examples

The client loads Lua functions from `scripts/init.lua`. Aliases, triggers, and event handlers call those functions by setting `lua = "function_name"` in `config.toml`.

Lua hooks do not change client state directly. They use the global `client` API, and the Rust client applies those actions through the normal command, output, event, variable, map, and UI pipelines.

Editor completion comes from `scripts/mud-client-api.lua` and `.luarc.json`. Editors using Lua Language Server should complete `client.*` APIs and hook context fields while editing files in `scripts/`.

## Setup

Keep the Lua runtime enabled:

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "init.lua"
instruction_budget = 100000
max_actions_per_hook = 32
runtime_errors_to_output = true
```

After changing Lua scripts or config, reload in the client:

```text
/reload
/lua reload
/lua status
```

`/reload` reloads config and Lua. `/lua reload` reloads only the Lua entrypoint.

## Alias Examples

### Smart Kill

Input:

```text
sk orc
```

Config:

```toml
[[aliases.rules]]
name = "smart-kill"
match_type = "regex"
pattern = "^sk\\s+(.+)$"
lua = "smart_kill"
```

Lua:

```lua
function smart_kill(ctx)
  local target = ctx.captures[1]
  if target == nil or target == "" then
    client.echo("Usage: sk <target>", { foreground = "yellow" })
    return
  end

  client.var.set("last_target", target)
  client.send("target " .. target)
  client.send("kill " .. target)
end
```

What it does:

- Stores the target in `last_target`.
- Sends `target <name>`.
- Sends `kill <name>`.
- Shows a local usage message if no target was captured.

### Run Weighted Map Path

Input:

```text
runto Bree
```

Config:

```toml
[[aliases.rules]]
name = "run-path"
match_type = "regex"
pattern = "^runto\\s+(.+)$"
lua = "run_path"
```

Lua:

```lua
function run_path(ctx)
  local destination = ctx.captures[1]
  if destination == nil or destination == "" then
    client.echo("Usage: runto <room name or vnum>", { foreground = "yellow" })
    return
  end

  client.notify("Running path to " .. destination, { foreground = "cyan" })
  client.map.run(destination)
end
```

What it does:

- Uses the existing weighted mapper route through `client.map.run`.
- Sends each movement command through the normal command pipeline.

## Trigger Examples

### Target Arriving Enemies

MUD output:

```text
Orc warlord arrives.
```

Config:

```toml
[[triggers.rules]]
name = "enemy-arrives-lua"
match_type = "regex"
pattern = "^(.+) arrives\\.$"
lua = "enemy_arrives"
```

Lua:

```lua
function enemy_arrives(ctx)
  local name = ctx.captures[1]
  if name == nil or name == "" then
    return
  end

  client.notify("Targeting " .. name, { foreground = "yellow" })
  client.send("target " .. name)
  client.event.emit("CombatStarted:" .. name, "lua")
end
```

What it does:

- Reads the enemy name from the regex capture.
- Sends `target <name>`.
- Emits `CombatStarted:<name>` for event handlers.

### Remember Current Room

MUD output:

```text
A Shadowy Forest    Exits are: S W
```

Config:

```toml
[[triggers.rules]]
name = "room-header-memory"
match_type = "regex"
pattern = "^(.+)\\s+Exits are:"
lua = "remember_room"
```

Lua:

```lua
function remember_room(ctx)
  local room = client.room.current()
  if room == nil then
    return
  end

  client.var.set("last_room", room.name)
end
```

What it does:

- Reads the current mapper room snapshot.
- Stores its name as `last_room`.

### Hunger and Thirst Notices

MUD output:

```text
You are hungry.
You are thirsty.
```

Config:

```toml
[[triggers.rules]]
name = "needs-warning"
match_type = "regex"
pattern = "^You are (hungry|thirsty)\\."
lua = "needs_warning"
```

Lua:

```lua
function needs_warning(ctx)
  local line = ctx.line or ""
  if line:find("hungry", 1, true) then
    client.notify("You are hungry.", { foreground = "yellow" })
  elseif line:find("thirsty", 1, true) then
    client.notify("You are thirsty.", { foreground = "cyan" })
  end
end
```

What it does:

- Uses `ctx.line`, the normalized visible MUD line.
- Writes a local highlighted notification.
- Does not send `eat` or `drink` automatically, so it is safe as a default example.

## Event Handler Examples

### Low Health

Manual test:

```text
/event {LowHealth}
```

Config:

```toml
[[events.handlers]]
name = "low-health-warning"
event = "LowHealth"
lua = "low_health"
```

Lua:

```lua
function low_health(ctx)
  local character = client.character.get()
  local hp = tonumber(character.health or "0") or 0
  local max_hp = tonumber(character.health_max or "0") or 0

  if max_hp > 0 and hp * 100 <= max_hp * 30 then
    client.notify("Low health: fleeing", { foreground = "red" })
    client.send("flee")
  else
    client.log.info("LowHealth event received without critical HP")
  end
end
```

What it does:

- Reads the current character health snapshot.
- Sends `flee` only when HP is at or below 30%.
- Logs the event when HP is not critical.

### Combat Started

This pairs with the `enemy_arrives` trigger, which emits `CombatStarted:<name>`.

Config:

```toml
[[events.handlers]]
name = "combat-started-lua"
match_type = "regex"
event = "^CombatStarted:(.+)$"
lua = "combat_started"
```

Lua:

```lua
function combat_started(ctx)
  local target = ctx.captures[1] or client.var.get("last_target") or "unknown"
  client.var.set("last_target", target)
  client.notify("Combat started: " .. target, { foreground = "red" })
end
```

What it does:

- Reads the target from the event regex capture.
- Stores the target in `last_target`.
- Displays a local combat notification.

### Route Requested

Manual test:

```text
/event {RouteRequested:Bree}
```

Config:

```toml
[[events.handlers]]
name = "route-requested-lua"
match_type = "regex"
event = "^RouteRequested:(.+)$"
lua = "route_requested"
```

Lua:

```lua
function route_requested(ctx)
  local destination = ctx.captures[1]
  if destination == nil or destination == "" then
    client.echo("RouteRequested event needs a destination.", { foreground = "yellow" })
    return
  end

  client.map.find(destination)
end
```

What it does:

- Finds the weighted route to a named room or vnum.
- Displays the route through the existing `/map find` command path.

## Hook Context Quick Reference

Alias hooks receive:

```lua
ctx.kind
ctx.input
ctx.captures
```

Trigger hooks receive:

```lua
ctx.kind
ctx.line
ctx.raw_line
ctx.category
ctx.captures
ctx.colors.foregrounds
ctx.colors.backgrounds
```

Event hooks receive:

```lua
ctx.kind
ctx.event
ctx.source
ctx.captures
```

## Safe API Notes

- Use `client.send` for MUD commands.
- Use `client.echo` or `client.notify` for local output.
- Use `client.event.emit` to chain behavior.
- Use `client.var.*` for session variables.
- Use `client.map.*` for mapper command wrappers.
- Filesystem, OS, package, process, and debug globals are disabled.
- Long-running hooks are stopped by `lua.instruction_budget`.
- Hooks can enqueue only `lua.max_actions_per_hook` actions.

For the full API surface, including `client.msdp`, snapshots, output search, UI toggles, map door helpers, and `client.time`, see [Lua API Reference](lua-api.md).
