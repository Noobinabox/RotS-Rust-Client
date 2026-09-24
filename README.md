# mud-client

Terminal MUD client for Return of the Shadow.

The current implementation includes:

- Connects asynchronously to `rotsmud.org:3791` by default.
- Uses Ratatui/Crossterm with raw mode and the alternate screen.
- Renders MUD output, command input, map, and responsive info, opponent, group, and character panels.
- Negotiates MSDP over Telnet and requests RoTS status reports.
- Parses nested MSDP strings, arrays, and tables, including the RoTS `GROUP` table.
- Maps RoTS `HEALTH`, `MANA`, and `MOVEMENT` values to terminal gauges.
- Executes configurable key macros through the command pipeline; see [macros](docs/commands/macro.md). Try `/macro {F5} {look}` and press F5.
- Supports dedicated numpad macros such as `/macro {Numpad8} {north}` on terminals that report keypad identity, without capturing top-row digits.
- Supports per-panel borders, text alignment, theme overrides, and refresh intervals; see [panel configuration](docs/configuration.md#panels) or `/help panels`.

## Run

```sh
cargo run
```

The default connection has no stored username or password. Enter login text directly in the command input.

The `--local` flag also forces `localhost:3791`, which is useful when an installed config points somewhere else:

```sh
cargo run -- --local
```

CLI help is available without entering terminal UI:

```sh
cargo run -- --help
```

## Configuration

The repository includes a default `config.toml`. At runtime, the client reads TOML configuration from the platform config directory, normally `~/.config/mud-client/config.toml` on Linux, when present. Missing configuration uses safe defaults.

Important defaults:

```toml
[connection]
host = "rotsmud.org"
port = 3791
line_ending = "\r\n"

[terminal]
echo_commands = true

[layout]
show_opponent = true
show_group = false
show_social = true

[msdp]
client_id = "mud-client"
ansi_colors = true
xterm_256_colors = true
utf_8 = false
```

The default MSDP reports use RoTS variable names, including `MOVEMENT`, `MOVEMENT_MAX`, combat bonuses, character stats, magic stats, `SPIRIT`, `EXPERIENCE`, `EXPERIENCE_MAX`, and `GROUP`.

## Documentation

End-user documentation lives in [`docs/`](docs/index.md):

- [Getting Started](docs/getting-started.md)
- [Configuration Reference](docs/configuration.md)
- [Command Reference](docs/commands.md)
- [User Interface](docs/ui.md)
- [Mapper](docs/mapping.md)
- [Scripting](docs/scripting.md)
- [Lua API Reference](docs/lua-api.md)
- [Lua Scripting Examples](docs/lua-scripting-examples.md)
- [Troubleshooting](docs/troubleshooting.md)

## Responsive Display Profiles

Terminal dimensions select the layout profile; physical display pixels do not. A terminal that is wide but too short falls back to the profile supported by its row count. Every breakpoint and profile-specific dimension can be changed in TOML.

| Profile | Terminal dimensions | Layout |
|---|---|---|
| Mobile | width below 80 or height below 24 | Map above MUD output, command at bottom |
| Tablet | width below 120 or height below 30 | Larger map above MUD output, command at bottom |
| 1080p | width below 200 or height below 45 | Classic configurable left or right sidebar beside output/command |
| Ultrawide | at least 200 columns and 45 rows | Expanded top dashboard and wider map/detail side panes |

Map, MUD output, and command input are required in every profile:

```toml
[layout.breakpoints]
mobile_width = 80
mobile_height = 24
tablet_width = 120
tablet_height = 30
ultrawide_width = 200
ultrawide_height = 45

[layout.mobile]
map_height = 5
show_status = false

[layout.tablet]
map_height = 8
show_status = true

[layout.full_hd]
sidebar_position = "right"
sidebar_width = 34

[layout.ultrawide]
top_height = 9
left_width = 42
right_width = 46
top = "status_dashboard"
left = "details"
right = "map"
```

The 1080p sidebar composes World, Map, Opponent, Group, and Character panels, dropping lower-priority optional panels when rows are limited; the map remains mandatory. World and Opponent stay compact so Map, Group, and Character can use the remaining space. Ultrawide defaults put details, including the Character panel, on the left and the full map on the right. The top dashboard shows World, Social, and a compressed Nearby Map. Social captures tells, chats, says, narrates, group-says, yells, and sings with local machine `HH:MM` timestamps, extracts the text inside the RoTS single quotes, displays `[HH:MM](channel) Prefix Text`, and has independent mouse-wheel scrollback plus word wrapping. Other players are prefixed with their name, tells use `from Name -` or `to Name -`, and your own chat/narrate/sing/yell/say/group-say omit the name prefix. `/echo` uses the same Social capture path for testing. Nearby Map omits link lines; road and city room symbols are adjacent so routes read as continuous paths, and up/down exit indicators overlay nearby cells. Ultrawide slot roles are `map`, `status_dashboard`, `details`, `info`, and `none`, and exactly one slot must contain the map. Existing panel visibility values `tiny`, `compact`, `standard`, and `wide` remain accepted as aliases for `mobile`, `tablet`, `1080p`, and `ultrawide`.

## Mapping

The client includes a TinTin++-style mapper using local slash commands. Local commands are prefixed with `/` so they are not sent directly to the MUD.

Common commands:

```text
/map map
/map create
/map goto 1
/map move n
/map dig e
/map link s 12 both
/map unlink s
/map delete 12
/map door e trigger hidden lever
/map door w closed gate
/map set roomname Grassy Scrubland
/map set roomterrain Forest
/map set roomweight 4
/map flag nofollow on
/map roomflag avoid on
/map roomflag fog;void on
/map exitflag e block on
/map list scrubland
/map landmark Home 1
/map landmarks
/map unlandmark Home
/map find 12
/map run 12
/map undo
/map return
/map leave
/map write maps/rots.toml
/map read maps/rots.toml
```

Map persistence can be enabled in config:

```toml
[map.persistence]
load_on_startup = true
path = "maps/rots.toml"
save_on_exit = false
```

`/map map` appends a snapshot centered on the current room and sizes it to the MUD output content area's current dimensions. The snapshot preserves the map panel's configured room, terrain, link, door, header, and theme colors. Bare `/map` displays structured Markdown-style command help.

The client also provides a TinTin++-style local path recorder. Use `/path create` or `/path start` before moving to record directions, then use `/path describe`, `/path walk`, `/path run`, `/path swap`, `/path zip`, and `/path unzip` to inspect and replay the recorded path. `/path save` and `/path load` use the existing runtime variable system. `/map find` and `/map run` remain the weighted destination-routing commands.

Normal movement commands are sent to the MUD without moving the local mapper. The map position and room links are synced from fresh RoTS MSDP `ROOM` table updates, which provide the current room vnum and destination vnums. The latest MSDP `ROOM_EXITS` array is used only to label those destination vnums with directions, matching the TinTin++ RoTS mapping script behavior.

RoTS MSDP room values are mapped with:

```toml
[msdp.mapping]
room = "ROOM"
room_name = "ROOM_NAME"
room_vnum = "ROOM_VNUM"
room_exits = "ROOM_EXITS"
world_time = "WORLD_TIME"
weather = "WEATHER"
```

When RoTS reports the MSDP `ROOM` table, the current room, exit directions from `ROOM_EXITS`, and target room vnums from `ROOM.EXITS` are synced into the mapper automatically. Standalone `ROOM_VNUM` and displayed room text do not drive mapping.

The sidebar info pane displays MSDP `WORLD_TIME` and `WEATHER`. The map pane header displays the current room name and exits instead of the room id.

Room terrain automatically sets pathing weight for `/map find` and `/map run`. `/map run` sends the optimized lowest-cost route, not just the route with the fewest rooms. Terrain weights are Floor `1`, Road `2`, Field `3`, Forest `4`, Hills `5`, Dense Forest `6`, Mountains `7`, Swamp `8`, Water `9`, Water Noswim `10`, Underwater `11`, and Crack `12`.

Map room centers default to three columns and two rows apart, substantially shortening links while keeping door and teleport markers visible. TinTin-style overlap handling keeps the first room reached at a display position and hides later overlapping rooms and their branches. Users can override link rendering, room spacing, the current-room marker, stub marker, the shared door glyph and per-state door colors, the teleport glyph/color, and terrain symbols/colors in config. Configured room, stub, door, and teleport glyphs must occupy one terminal cell; terrain symbols use one cell normally or exactly two one-cell characters when `double = true`. Doors and teleports render on cardinal links; RoTS mapping uses `n`, `e`, `s`, `w`, `u`, and `d`. Door names are directional map data: `/map door w closed gate` stores `gate` only on the current room's west exit. `trigger`, `unknown`, and `open` doors do not add movement commands; `closed` doors send `open <name> <dir>`, `pickable` doors send `pick <name> <dir>`, and `locked` doors send `unlock <name> <dir>` then `open <name> <dir>` before moving.

```toml
[map]
show_links = true
room_spacing_columns = 3
room_spacing_rows = 2
current_room_symbol = "X"
current_room_color = "#cc0000"
stub_symbol = "∘"
link_color = "#505050"
avoid_color = "#e0af68"
block_color = "#f7768e"
hide_color = "#565f89"
invis_color = "#7aa2f7"
fog_color = "#89ddff"
void_color = "#414868"

[map.doors]
show = true
glyph = "╬"
open_color = "#65b875"
closed_color = "#d6ad55"
pickable_color = "#61afef"
locked_color = "#d45c5c"
trigger_color = "#c678dd"
unknown_color = "#777777"

[map.teleport]
show = true
glyph = "◇"
color = "#61afef"

[map.terrain.Forest]
symbol = "♣"
color = "#2e7d32"
density = "dense" # dense, sparse, scant, or empty
spread = "wide"   # narrow, wide, vast, or empty
fade = "fadeout"  # fadein, fadeout, or empty
double = false    # true when symbol is two terminal cells
```

### Panels and Reload

Panel titles and visibility can be configured in TOML. `visible_modes` accepts `mobile`, `tablet`, `1080p`, and `ultrawide`; leave it empty to show the panel in every responsive mode where that panel type is available. The legacy names `tiny`, `compact`, `standard`, and `wide` are also accepted.

```toml
[panels.output]
enabled = true
title = "MUD Output"
min_width = 20
min_height = 5
priority = 100
visible_modes = []

[panels.map]
enabled = true
title = "Map"
min_width = 20
min_height = 5
priority = 90
visible_modes = ["1080p", "ultrawide"]

[panels.social]
enabled = true
title = "Social"
min_width = 30
min_height = 5
priority = 95
visible_modes = ["ultrawide"]
```

Use `/reload` to reload config from disk. Validation errors are written to the output pane, and in-memory group/opponent/social toggles are preserved for the current session.

### Script Variables

Variables provide shared values for alias patterns and commands, trigger patterns, commands, and emitted events, and event handler patterns, commands, notifications, and emitted events. Use `${name}` to interpolate a value and `$${name}` to preserve literal `${name}` text. Variable expansion happens before regular-expression compilation and before `{1}` capture substitution.

```toml
[variables]
max_expansion_depth = 8
max_expanded_bytes = 65536

[variables.values]
target = "orc"
attack_command = "kill ${target}"
```

Configured values are persistent. Runtime values override them for the current session and survive `/reload`:

```text
/variable
/variable {target} {troll}
/variable unset {target}
```

`/variable` lists every effective value with its configured or runtime source. Variable names start with an ASCII letter or underscore and contain only ASCII letters, digits, or underscores. Missing references, recursive references, invalid names, and expansions beyond the configured depth or byte limits are rejected without partially updating the scripting engines. Because `/variable` is part of the normal local-command pipeline, aliases, triggers, and event handlers can set runtime variables.

Variables are also expanded in commands entered directly in the command pane:

```text
kill ${target}
/echo Current Target: ${target}
say $${target}
```

With `target = "dragon"`, these become `kill dragon`, local output `Current Target: dragon`, and the literal server command `say ${target}`. Runtime `/alias`, `/trigger`, and `/variable` definitions preserve `${name}` templates so they continue to react to later variable changes.

### Local Echo

`/echo` writes a local line to the MUD output pane without sending it to the server. Foreground and background colors are optional and accept `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`, `white`, the bright forms `darkgray` and `lightred` through `lightcyan`, or `#RRGGBB`:

```text
/echo Ready
/echo --fg yellow Current Target: ${target}
/echo --fg=#ffffff --bg=#800000 Danger: ${target}
/echo -- --text beginning with dashes
```

Use `/help echo` for the in-client reference. Echo commands can be entered directly or executed by aliases, triggers, and event handlers.

### Aliases

Aliases are expanded before commands are sent to the MUD. They can be configured in TOML or added for the current session with `/alias`.

```toml
[aliases]
enabled = true
max_expansion_depth = 8
max_expanded_commands = 256

[[aliases.rules]]
name = "kill"
match_type = "regex"
pattern = "^k\\s+(.+)$"
commands = ["kill {1}"]
priority = 100

[[aliases.rules]]
name = "group-say"
match_type = "prefix"
pattern = "gs"
commands = ["gtell {1}"]

[[aliases.rules]]
name = "recall"
match_type = "exact"
pattern = "rr"
commands = ["recall", "look"]
```

Runtime aliases are in-memory only:

```text
/alias
/alias {k} {kill {1}}
/alias {rr} {recall} {look}
/alias unset {k}
/alias clear
```

`/alias unset {pattern}` removes one session alias, and `/alias clear` removes all session aliases. Configured aliases are removed from `config.toml` and reloaded.

### Triggers and Highlights

Triggers inspect incoming MUD output and can send commands. Highlights inspect the same output and style matching lines.

```toml
[triggers]
enabled = true
max_commands_per_line = 8

[[triggers.rules]]
name = "enemy_arrives"
match_type = "regex"
pattern = "^(.+) arrives\\.$"
commands = ["target {1}"]
event = "EnemyEntered:{1}"
cooldown_ms = 1000

[[triggers.rules]]
name = "red_danger"
match_type = "plain"
pattern = "Danger"
foreground = "lightred"
background = "index:17"
commands = ["flee"]

[highlights]
enabled = true

[[highlights.rules]]
name = "damage"
match_type = "plain"
pattern = "You are hit"
foreground = "#f7768e"
bold = true
```

Use `/triggers` to list every trigger currently loaded. `/trigger` remains available as a singular alias. Runtime triggers use the same braced command style as aliases and remain in memory only:

```text
/triggers
/triggers plain {You are hungry} {eat bread}
/triggers regex {^(.+) arrives\.$} {target {1}} {consider {1}}
/triggers plain --fg lightred --bg index:17 {Danger} {flee}
/triggers unset {You are hungry}
/triggers clear
```

Runtime triggers use plain substring matching unless a command references a capture such as `{1}`. Capture references infer a regular expression for backward compatibility; `plain` and `regex` select the behavior explicitly. Optional `--fg` and `--bg` filters require that ANSI foreground/background color somewhere in the MUD output line. Filters accept named and bright colors (`white` is ANSI 37; use `brightwhite` for ANSI 97), indexed colors as `index:N`, and RGB colors as `#RRGGBB`. Colors inherit across lines until the MUD resets them; prompt boundaries reset inherited color in both trigger matching and rendering. Trigger actions pass through local commands, aliases, movement tracking, and command echo. Adding the same runtime pattern again replaces its previous session definition.
Use `/triggers unset {pattern}` to remove one runtime trigger or `/triggers clear` to remove all runtime triggers.

Triggers inspect both completed output lines and live prompts. ANSI styling is removed before matching, so anchored regular expressions operate on the visible MUD text. Persistent trigger rules can emit an event through their `event` field.

Use `/highlight` to list configured and session-only highlight rules. Add or replace a runtime highlight with a plain pattern by default or select regex matching explicitly:

```text
/highlight
/highlight {You are hit} {red}
/highlight regex {You receive \d+ gold} {yellow} {none} {bold}
/highlight plain {IMPORTANT} {white} {red} {bold underline}
/highlight unset {You are hit}
/highlight clear
```

The fields are `{pattern}`, `{foreground|none}`, optional `{background|none}`, and optional `{styles}`. Styles may contain `bold`, `dim`, `italic`, `underline`, or `reverse`, separated by commas or spaces. Runtime highlights apply to normal MUD output, take priority over configured rules, survive `/reload`, and remain in memory only for the current session. Configured rules can use `categories` to target normal MUD output or prompts.
Use `/highlight unset {pattern}` to remove one runtime highlight or `/highlight clear` to remove all runtime highlights.

### Script Events

Events separate detection from reaction. Triggers, application code, or `/event {name}` emit a typed built-in or custom event; configured handlers can run commands, display notifications, and emit bounded follow-up events.

```toml
[events]
enabled = true
max_dispatch_depth = 8
max_events_per_dispatch = 32
max_commands_per_dispatch = 16
history_limit = 100

[[events.handlers]]
name = "target-arriving-enemy"
match_type = "regex"
event = "^EnemyEntered:(.+)$"
commands = ["target {1}"]
notification = "Targeting {1}"
emit = ["CombatStarted:{1}"]
priority = 100
```

`plain` handlers match an event name exactly. `regex` handlers can substitute captures into commands, notifications, and emitted event names. Handler commands pass through local commands, aliases, movement tracking, command echo, and the network pipeline. Cascades use a bounded FIFO queue, and dispatch fails visibly if its depth, event count, or command count limit is exceeded.

Use `/event` to show a compact handler summary, dispatch status, and the ten most recent retained events. Use `/event {LowHealth}` or `/event {EnemyEntered:Orc}` to emit an event manually.

Use `/handler` to inspect complete handler definitions, including their configured/runtime source, match type, enabled state, priority, commands, notification, and emitted events. Add or replace a session-only command handler with `/handler [plain|regex] {event} {command} [{command}...]`:

```text
/handler {LowHealth} {flee}
/handler regex {^EnemyEntered:(.+)$} {target {1}} {/echo Targeting {1}}
/handler unset {LowHealth}
/handler clear
```

When the match type is omitted, a handler using capture references infers regex matching; otherwise it uses exact plain matching. Adding the same runtime event pattern again replaces its previous session definition. Runtime handlers survive `/reload` but are not persisted across restarts.
The global `[events] enabled` setting applies to configured and runtime handlers.
Use `/handler unset {event}` to remove one runtime handler or `/handler clear` to remove all runtime handlers.

Use `/help trigger`, `/help event`, and `/help highlight` for the in-client field reference.

### Lua Scripts

Lua is the procedural scripting layer for logic that is awkward in declarative aliases, triggers, or event handlers. The simple systems remain available and are still the best fit for direct command substitutions.

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "init.lua"
instruction_budget = 100000
max_actions_per_hook = 32
runtime_errors_to_output = true

[[aliases.rules]]
name = "smart-kill"
match_type = "regex"
pattern = "^sk\\s+(.+)$"
lua = "smart_kill"

[[triggers.rules]]
name = "enemy-arrives-lua"
match_type = "regex"
pattern = "^(.+) arrives\\.$"
lua = "enemy_arrives"

[[events.handlers]]
name = "lua-low-health"
event = "LowHealth"
lua = "low_health"
```

Lua hooks are named functions loaded from `scripts/init.lua` relative to the active config file. When the default config is used, relative script paths resolve from the platform config directory, normally `~/.config/mud-client`, rather than the current working directory. The repository includes an example `scripts/init.lua` with `smart_kill`, `enemy_arrives`, `low_health`, and `remember_room` hooks. Filesystem, OS, process, package, and debug globals are removed from the Lua environment. Hooks cannot mutate state directly; they enqueue actions through the safe `client` API and Rust applies those actions through the existing command, output, event, variable, map, and UI paths.

```lua
function enemy_arrives(ctx)
  local name = ctx.captures[1]
  client.echo("Targeting " .. name, { foreground = "yellow" })
  client.send("target " .. name)
  client.event.emit("CombatStarted:" .. name, "lua")
end
```

Available APIs:

- `client.send(command)` and `client.send_all(commands)`
- `client.echo(text, opts)` and `client.notify(text, opts)`
- `client.log.debug/info/warn/error(message)`
- `client.var.get/set/unset/list`
- `client.msdp.get/all`
- `client.character.get`, `client.opponent.get`, `client.group.list`, `client.room.current`
- `client.output.recent(limit)` and `client.output.search(text)`
- `client.event.emit(name, source)` and `client.event.recent(limit)`
- `client.map.current/get/find/goto/run/door/set_terrain/set_weight`
- `client.ui.toggle(panel, state)` and `client.ui.reload`
- `client.time.now_ms`

Use `/lua status`, `/lua reload`, and `/lua call <function>` for runtime inspection. Use `/help lua`, `/help alias lua`, `/help trigger lua`, and `/help event lua` inside the client. See [Lua API Reference](docs/lua-api.md) for every exposed API and [Lua Scripting Examples](docs/lua-scripting-examples.md) for copy-ready alias, trigger, and event-handler examples.

Editor completion is provided through `scripts/mud-client-api.lua`, which declares the global `client` API and hook context types with LuaLS/EmmyLua annotations. The repository `.luarc.json` adds `scripts` as a Lua workspace library, so editors using Lua Language Server should show completions for `client.send`, `client.msdp.get`, hook context fields, and the rest of the API while editing scripts.

### Animation

Animation timing is centralized in a scheduler so map effects and weather can advance by elapsed time instead of render counts.

```toml
[animation]
enabled = true
reduced_motion = false
low_performance = false
map_fps = 12
weather_fps = 10

[weather]
enabled = true
show_info_marker = true
```

Use `/help animation` for the in-client field reference.

Use `;` to send multiple MUD commands from one input:

```text
look;score
k bear;look
```

Set `terminal.echo_commands = false` to stop appending sent MUD commands to the output pane. When enabled, echoed commands are appended as plain command text without a prompt prefix.

The Opponent, Group, and Social panels are optional. Group is off by default. Set `layout.show_opponent`, `layout.show_group`, or `layout.show_social` in config, or toggle them for the current session:

```text
/toggle opponent off
/toggle group
/toggle social
```

The Opponent pane uses a compact fixed-height layout with a full-width health gauge. The Character pane separates the resource gauges from the stat sheet with a blank row, renders base stats (`Str`, `Int`, `Wil`, `Dex`, `Con`, `Lea`) on one row, then leaves another blank row before a single class-specific detail row. If Mage is the highest class level, it shows `Mana Regen`, `Spell Power`, and `Spell Pen`. If Mystic is highest, it shows `Willpower`, `Spirits`, `Health Regen`, and `Movement Regen`; these metrics wrap between complete label/value pairs in narrow panes. The Group pane adapts to group size. Small groups use the richer two-line gauge layout, while larger groups switch to compact multi-column rows so more member vitals remain visible in the fixed sidebar space.

## Keys

- `Enter`: send command, or send a blank line when the input is empty
- `;`: separate multiple MUD commands in one input
- `#<count> {command}`: repeat one command or braced command group, for example `#10 {kill orc}` or `#2 {look;score};rest`
- `Enter` on highlighted last command: resend command
- `Tab`: complete the current word from recent MUD output
- `Shift-Tab`: cycle to the previous completion
- `Up` / `Down`: navigate command history, filtered by typed prefix when input is not empty
- `Left` / `Right` / `Home` / `End`: edit highlighted last command or move within input
- `PageUp` / `PageDown`: scroll MUD output
- Mouse wheel over MUD output: scroll MUD output
- Mouse wheel over Social: scroll captured social messages
- Drag either large-layout side divider: resize that pane for the current session
- `Ctrl-Up` / `Ctrl-Down`: scroll MUD output one line
- `Ctrl-E`: follow newest output
- `Ctrl-L`: clear output
- `Ctrl-F`: search output
- `Ctrl-N` / `Ctrl-P`: next/previous search match

MUD output scrolling stops at the oldest full visible page, so the pane does not scroll beyond retained text into mostly blank space.
- `F2`: cycle styled, plain, and debug output views; the mode indicator appears briefly in the output title
- `Ctrl-C`: clear the command input; use `/quit` to exit
- `/help`: show local client commands and help topics
- `/help map`, `/help alias`, `/help path`: show topic-specific help
- `/help trigger`, `/help highlight`: show scripting and styling help
- `/reload`: reload config from disk
- `/reconnect`: request a network reconnect
- `/variable`: list, set, or remove script variables
- `/alias`: list, add, or remove in-memory aliases
- `/triggers`: list, add, or remove in-memory triggers
- `/highlight`: list, add, or remove in-memory highlights
- `/handler`: list, add, or remove in-memory event handlers
- `/toggle group|opponent|social [on|off]`: toggle optional panels for this session
- `/msdp`: show the current stored MSDP values
- `/clear`: clear output
- `/quit`: quit

## Validate

Common workflows are available through `make`:

```sh
make help
make local
make ci
```

Diagnostics:

- `/msdp`: show current stored MSDP values.
- `/reload`: reload config and display validation errors.
- `/reconnect`: request reconnect through the network command channel.
- `logging.level`: tracing filter, for example `mud_client=debug`.
- `logging.raw_protocol`: reserved protocol diagnostics flag; keep disabled unless troubleshooting.

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
git diff --check
```
