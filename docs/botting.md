# Lua Botting

The client includes an opt-in Lua bot controller in `scripts/bot.lua`. It supports timed movement, exact mob-description matching, combat pausing, recovery, multiple areas, chained paths, and optional path cycling.

Botting is never started automatically. The user must explicitly call `bot_start`.

## Enable the bot script

Configure Lua to use the bot entrypoint:

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "bot.lua"
instruction_budget = 100000
max_actions_per_hook = 32
runtime_errors_to_output = true
```

Reload the configuration:

```text
/reload
/lua status
```

When `bot.lua` is the entrypoint, the client automatically loads the companion `scripts/bot_areas.lua` file.

## Basic controls

Lua functions are called through the local `/lua` command:

```text
/lua call bot_start
/lua call bot_status
/lua call bot_pause
/lua call bot_resume
/lua call bot_stop
```

`bot_start` starts the repeating `bot-controller` timer. `bot_stop` cancels it. `bot_pause` preserves the current path position while stopping movement and attacks. `bot_resume` clears the combat pause and allows the controller to continue.

## Bot configuration

The top of `scripts/bot.lua` contains the default configuration:

```lua
bot_config = {
  tick_ms = 750,
  restart_paths = true,
  cycle_paths = true,
  recovery_destination = "Town",
  paths = {
    {
      name = "example-hunt",
      waypoints = { "Town", "Hunting Grounds" },
      creatures = { "goblin", "orc" },
      start_room = "Town",
      commands = nil,
      mob_triggers = {},
    },
  },
}
```

Important fields:

- `tick_ms`: interval between controller checks.
- `restart_paths`: restart at the first path after the final path completes.
- `cycle_paths`: advance through the next configured path.
- `recovery_destination`: destination used after a flee or repeated movement failures.
- `waypoints`: named or numbered destinations passed to `client.map.run`.
- `commands`: optional one-command-at-a-time route. Use this when an area requires special actions such as `ride`, `dismount`, `od e`, or `#5 pick door`.
- `start_room`: recovery/reset room for the area.
- `mob_triggers`: exact room descriptions that are safe to attack.

## Exact mob triggers

The bot never attacks based on a broad keyword. Each mob trigger compares the complete incoming line with `mob_triggers[].text`:

```lua
mob_triggers = {
  {
    text = "A large grey troll grumbles ominously here.",
    attack = "troll",
  },
  {
    text = "A huge, bulky troll sits in the corner, slowly eating an old corpse.",
    attack = "bulky",
    hard = true,
  },
}
```

`attack` is the argument sent to `kill`. `hard = true` records that the target is dangerous and can be used by class-specific combat logic. Do not replace the complete text with `troll`, `spider`, or another partial pattern.

The trigger rule that passes room output to the bot is:

```toml
[[triggers.rules]]
name = "bot-exact-mob-selector"
pattern = ".+"
lua = "bot_mob_line"
priority = 1
```

The Lua function rejects every line that does not exactly match a configured mob trigger.

## Loading an area

The bundled area table can be loaded at runtime:

```text
/lua call bot_load_area ancient
/lua call bot_load_area semirkwood
/lua call bot_load_area thogs
/lua call bot_start
```

Use `/lua call bot_status` to verify the active path, waypoint, recovery state, and running state.

## Adding a new area

Add an entry to `scripts/bot_areas.lua`:

```lua
my_area = {
  title = "My Hunting Area",
  start_room = "1000",
  cycle = true,
  commands = {
    "n", "e", "od s", "ride", "s",
  },
  mob_triggers = {
    {
      text = "A small green goblin sharpens a rusty blade.",
      attack = "goblin",
    },
    {
      text = "A towering goblin war chief watches the passage.",
      attack = "war chief",
      hard = true,
    },
  },
}
```

Then reload Lua and load it:

```text
/lua reload
/lua call bot_load_area my_area
/lua call bot_start
```

For a mapper-driven route, use `waypoints` instead of `commands`:

```lua
my_area = {
  title = "Mapper Area",
  start_room = "Town",
  waypoints = { "Town", "North Gate", "Hunting Grounds" },
  mob_triggers = {},
}
```

Mapper routes continue to respect mapped room flags, doors, and room weights.

## Chaining areas

Put multiple path entries in `bot_config.paths`:

```lua
bot_config.paths = {
  bot_areas.ancient,
  bot_areas.thogs,
  bot_areas.waterfall,
}
bot_config.cycle_paths = true
bot_config.restart_paths = true
```

The controller advances to the next path when the current path reaches its final command or waypoint. With `restart_paths = true`, completing the last path returns to the first path. With `restart_paths = false`, the bot stops after the final path.

Runtime toggles are available:

```text
/lua call bot_set_cycle on
/lua call bot_set_cycle off
/lua call bot_set_restart on
/lua call bot_set_restart off
```

## Movement verification and recovery

The controller uses the current room snapshot to verify mapper movement. For command routes, room-change events clear the pending movement state. A failed movement callback increments `bot_state.failed_step`; after five failures, `bot_reset` routes to the area's `start_room`.

Configure a room-change handler:

```toml
[[events.handlers]]
name = "bot-room-changed"
event = "RoomChanged"
lua = "bot_room_changed"
```

Configure a failed-step trigger for the messages used by the game:

```toml
[[triggers.rules]]
name = "bot-movement-failed"
pattern = "You can not do it this way\\."
lua = "bot_failed_step"
```

## Flee recovery

Add triggers for every flee message the game can produce:

```toml
[[triggers.rules]]
name = "bot-flee"
pattern = "You flee"
lua = "bot_flee_line"
priority = 100
```

When `bot_flee_line` runs, the controller pauses normal path movement and routes to `recovery_destination`. Once there, it restarts the current path from its first waypoint.

## Combat events

Combat should pause movement so the bot does not walk away from an active target. Configure game-specific triggers or events:

```toml
[[triggers.rules]]
name = "bot-combat-start"
pattern = "You attack"
lua = "bot_combat_started"

[[triggers.rules]]
name = "bot-combat-finished"
pattern = "slumps over dead"
lua = "bot_combat_finished"

[[triggers.rules]]
name = "bot-target-missing"
pattern = "They aren't here\\."
lua = "bot_combat_finished"
```

`bot_combat_started` increments the engaged counter and pauses movement. `bot_combat_finished` decrements it and resumes movement when no engagements remain.

## Timers

The bot uses the named repeating timer `bot-controller`:

```lua
client.timer.set("bot-controller", 750, "bot_tick", true)
client.timer.cancel("bot-controller")
```

Timers can also be managed manually:

```text
/timer list
/timer set test 1000 bot_status once
/timer set bot-controller 750 bot_tick repeat
/timer cancel test
/timer clear
```

Timer callbacks receive:

- `ctx.kind == "timer"`
- `ctx.timer`
- `ctx.tick_count`

Callbacks should remain short and queue actions through `client.send`, `client.map.run`, or other safe APIs.

## Complete function reference

| Function | Purpose |
| --- | --- |
| `bot_start` | Start the controller timer and reset bot state. |
| `bot_stop` | Stop the controller and cancel its timer. |
| `bot_status` | Display active path and recovery state. |
| `bot_load_area` | Load one entry from `bot_areas.lua`. |
| `bot_tick` | Verify movement, route, attack, cycle, and recover. |
| `bot_pause` | Pause movement and attacks. |
| `bot_resume` | Resume movement after combat or manual recovery. |
| `bot_reset` | Route to the area start room and reset progress. |
| `bot_mob_line` | Attack only exact configured mob descriptions. |
| `bot_combat_started` | Increment combat engagement and pause. |
| `bot_combat_finished` | Decrement engagement and resume when clear. |
| `bot_failed_step` | Track failed movement and reset after five failures. |
| `bot_room_changed` | Clear pending movement and reset failed-step state. |
| `bot_flee_line` | Start recovery after fleeing. |
| `bot_add_path` | Add a simple runtime path from command arguments. |
| `bot_set_restart` | Enable or disable path restart. |
| `bot_set_cycle` | Enable or disable multi-path cycling. |

## Safety checklist

Before starting a bot:

1. Confirm every mob description is exact and intentional.
2. Confirm hard or boss mobs have `hard = true` or are omitted.
3. Test the area manually with `bot_start` stopped until movement is correct.
4. Verify flee and failed-movement messages match the configured triggers.
5. Confirm `/lua call bot_stop` and `/timer clear` stop all automation.
