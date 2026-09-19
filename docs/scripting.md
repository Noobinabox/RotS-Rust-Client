# Scripting

The client has declarative scripting for simple automation and Lua for procedural logic.

## Variables

Variables use `${name}` syntax in command input, aliases, triggers, and event handlers.

```toml
[variables.values]
target = "orc"
attack_command = "kill ${target}"
```

Runtime commands:

```text
/variable
/variable {target} {troll}
/variable unset {target}
```

Use `$${name}` to send literal `${name}` text.

## Aliases

Aliases match typed commands before they are sent to the MUD.

```toml
[[aliases.rules]]
name = "kill"
match_type = "regex"
pattern = "^k\\s+(.+)$"
commands = ["kill {1}"]
priority = 100
```

Runtime aliases are in-memory:

```text
/alias {k} {kill {1}}
/alias {rr} {recall} {look}
/alias unset {k}
/alias clear
```

`unset` and `clear` only remove runtime aliases. Remove configured aliases from `config.toml` and run `/reload`.

Repeated command syntax runs through the same alias pipeline:

```text
#3 {k orc}
#2 {look;score};rest
```

Only the braced group repeats. A semicolon after the closing brace starts the next command normally.

## Triggers

Triggers inspect normalized MUD output and can send commands, emit events, and call Lua.

```toml
[[triggers.rules]]
name = "enemy_arrives"
match_type = "regex"
pattern = "^(.+) arrives\\.$"
commands = ["target {1}"]
event = "EnemyEntered:{1}"
cooldown_ms = 1000
```

Color filters can require ANSI foreground or background colors:

```text
/triggers plain --fg lightred --bg index:17 {Danger} {flee}
/triggers unset {Danger}
/triggers clear
```

`unset` matches the runtime trigger pattern. Configured triggers are removed from `config.toml` and reloaded.

## Highlights

Highlights style matching output without changing raw text.

```toml
[[highlights.rules]]
name = "damage"
match_type = "plain"
pattern = "You are hit"
foreground = "#f7768e"
bold = true
```

Runtime highlights:

```text
/highlight {You are hit} {red}
/highlight regex {You receive \d+ gold} {yellow} {none} {bold}
/highlight unset {You are hit}
/highlight clear
```

Runtime highlight removal does not edit configured highlight rules.

## Events

Events separate detection from reaction. Triggers, Lua, application state, or `/event` can emit events.

```toml
[[events.handlers]]
name = "target-arriving-enemy"
match_type = "regex"
event = "^EnemyEntered:(.+)$"
commands = ["target {1}"]
notification = "Targeting {1}"
emit = ["CombatStarted:{1}"]
```

Runtime event handlers:

```text
/handler {LowHealth} {flee}
/handler regex {^EnemyEntered:(.+)$} {target {1}} {/echo Targeting {1}}
/handler unset {LowHealth}
/handler clear
```

Runtime handler removal does not edit configured handlers.

## Lua Hooks

Lua hooks are named functions loaded from `scripts/init.lua`.

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "init.lua"

[[aliases.rules]]
name = "smart-kill"
match_type = "regex"
pattern = "^sk\\s+(.+)$"
lua = "smart_kill"
```

Hooks receive a context table and enqueue safe actions through the global `client` API:

```lua
function smart_kill(ctx)
  local target = ctx.captures[1]
  client.var.set("target", target)
  client.send("kill " .. target)
end
```

The Lua environment removes filesystem, process, package, OS, and debug access. Hooks are bounded by `lua.instruction_budget` and `lua.max_actions_per_hook`.

Editor completion is provided by `scripts/mud-client-api.lua` and `.luarc.json` for Lua Language Server.

See [Lua API Reference](lua-api.md) for every exposed `client` API and [Lua Scripting Examples](lua-scripting-examples.md) for complete examples.
