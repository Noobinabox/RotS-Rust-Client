# Configuration Reference

The active config file is normally `~/.config/mud-client/config.toml`. The repository `config.toml` is a full example and can be symlinked or copied into that location.

Colors accept named terminal colors, bright color names such as `lightred`, indexed colors such as `index:17`, or RGB values such as `#7aa2f7`.

MSDP character-name reports automatically layer `characters/name.toml` over shared configuration; `--character NAME` optionally preselects it at startup. Missing profiles are silently ignored. Tables merge; arrays replace. Connection settings remain shared and runtime saves are isolated per profile. See [character profiles](character-profiles.md) for setup, examples, and reload behavior.

## macros

Runtime variables, macros, aliases, triggers, and highlights can also be saved with [`/save`](commands/save.md). This writes a versioned sidecar, normally `config.toml.runtime.toml` beside the active config, and restores it at startup or first activation of a character profile. Saved runtime variables override configured values and are restored before dependent rules. Main configuration and comments are untouched. `/reload` retains live variables and rules; it does not reread the sidecar. Handlers, timers, and printable-key mode are not included. Saved variable values are plain text; do not save secrets.

See [key macro configuration](commands/macro.md#persistent-configuration): `[[macros.rules]]` accepts `key`, `command`, `enabled`, `override_builtin`, and `allow_repeat`. No macros are installed by default. Printable-key mode is session-only and starts off.

## connection

| Option | Meaning |
|---|---|
| `host` | MUD host name or IP address. The default is `rotsmud.org`. |
| `port` | MUD TCP port. RoTS uses `3791`. |
| `username` | Optional stored login username. Empty string disables stored username behavior. |
| `password` | Optional stored password. Avoid committing real credentials. |
| `auto_reconnect` | Whether disconnects should request reconnect behavior. |
| `line_ending` | Bytes appended to sent commands. RoTS uses `"\r\n"`. |

## terminal

| Option | Meaning |
|---|---|
| `tick_rate_ms` | Main event/render tick interval in milliseconds. Must be greater than zero. |
| `animation_fps` | Maximum animation frame rate. Must be greater than zero. |
| `mouse` | Enables mouse events such as output scrolling and pane resizing. |
| `true_color` | Enables RGB color output when the terminal supports it. |
| `echo_commands` | Appends sent MUD commands as local output lines without a prompt prefix. |
| `input_mode` | `"standard"` (default) or opt-in `"vim"`. Reloadable and character-profile-aware; see [Vim editing](commands/vim.md). |
| `multiline_input` | Defaults to `false`: pasted newlines become spaces. When `true`, preserve pasted newlines and use Alt+Enter to add a line; Enter submits nonblank lines in order. See [input examples](commands/input.md#paste-and-multiline-editing). |

## layout

| Option | Meaning |
|---|---|
| `show_character` | Initial character panel visibility. Can be toggled in-memory. |
| `show_opponent` | Initial opponent panel visibility. Can be toggled in-memory. |
| `show_group` | Initial group panel visibility. Defaults to `false`; can be toggled in-memory. |
| `show_social` | Initial Social panel visibility. Can be toggled in-memory. |
| `show_output` | Output pane visibility. Output is considered required by the UI. |
| `scrollback_lines` | Maximum retained output scrollback lines. |

### layout.breakpoints

Terminal dimensions select the responsive profile.

| Option | Meaning |
|---|---|
| `mobile_width`, `mobile_height` | Below either value, use mobile layout. |
| `tablet_width`, `tablet_height` | Below either value, use tablet layout. |
| `ultrawide_width`, `ultrawide_height` | At or above both values, use ultrawide layout. |

Breakpoints must increase from mobile to tablet to ultrawide.

### layout.mobile and layout.tablet

| Option | Meaning |
|---|---|
| `map_height` | Rows reserved for the map above output. |
| `show_status` | Shows compact status information when space allows. |

### layout.full_hd

| Option | Meaning |
|---|---|
| `sidebar_position` | `left` or `right`. When left, command input aligns with MUD output. |
| `sidebar_width` | Sidebar width in terminal columns. Runtime mouse resizing can override it for the current session. |

### layout.ultrawide

| Option | Meaning |
|---|---|
| `top_height` | Rows reserved for the top pane. |
| `left_width` | Columns reserved for the left pane. |
| `right_width` | Columns reserved for the right pane. |
| `top`, `left`, `right` | Pane roles: `map`, `status_dashboard`, `details`, `info`, or `none`. Exactly one slot must contain `map`. Defaults place `details` on the left and `map` on the right. |

## colors

These are the global Tokyo Night themed defaults used by panels and widgets unless a more specific option overrides them.

| Option | Meaning |
|---|---|
| `background` | Default background. |
| `foreground` | Default foreground text. |
| `border` | Panel borders. |
| `title` | Panel titles. |
| `accent` | Selected or emphasized UI elements. |
| `success` | Healthy or positive state. |
| `warning` | Warning state. |
| `danger` | Low health, errors, or dangerous state. |
| `muted` | Inactive, empty, or secondary text. |
| `player` | Player marker and player-focused accents. |
| `enemy` | Opponent marker and enemy-focused accents. |

## gauges

| Option | Meaning |
|---|---|
| `unicode` | Uses Unicode bar glyphs when true; ASCII fallback when false. |
| `width` | Gauge bar width in cells for compact gauges and the fallback width when a full gauge cannot expand. Full character, opponent, and TNL gauges otherwise fill the available row width. |

## animation

| Option | Meaning |
|---|---|
| `enabled` | Master animation toggle. |
| `reduced_motion` | Prefer less motion while keeping static indicators visible. |
| `low_performance` | Reduces animation work for slower terminals. |
| `map_fps` | Target map animation frames per second. Must be greater than zero. |
| `weather_fps` | Target weather animation frames per second. Default: 2 (one change every 500 ms). Must be greater than zero. |

## weather

| Option | Meaning |
|---|---|
| `enabled` | Enables the World pane weather background and its animation; server weather text remains visible when false. |
| `show_info_marker` | Legacy setting name retained for compatibility: shows the weather background scene, not a cycling marker. |

See [weather types and animation examples](commands/animation.md). All six RoTS
sky states are recognized, with sunshine, clouds, rain, lightning, snow, and blizzard
scenes; generic fog, wind, ash, and dust also have scenes. Backgrounds stay inside
the World pane's content area and do not obscure text. Lighting follows the
server's `WORLD_TIME`, not the local clock: warm theme `warning` colors at dawn/dusk,
weather-specific non-dimmed colors by day, and dimmed `muted`/`accent` colors at
night. Clear nights show a crescent, not a lunar-phase simulation. Unknown time
retains the dimmed `muted`/`accent` palette. The normal RoTS hour-only time uses
6 AM dawn and 6 PM dusk as display fallbacks, rather than seasonal sunrise data.
See the animation guide for the complete theme mapping.

Disabled animation or reduced motion retains a static first-frame scene;
game-time lighting still updates. Low-performance mode caps weather motion at 2 FPS. Visible
motion is limited by terminal ticks and the World pane's configured refresh interval.

## substitutions

`[substitutions] enabled` controls configured display rules. Runtime rules remain
active independently. `[[substitutions.rules]]` supports `name`, `enabled`,
`priority`, `match_type` (`plain` or `regex`), `pattern`, `replacement`, optional
`foreground`/`background`, and `categories`. See [examples and Lua line edits](commands/substitute.md).

## panels

Each panel table supports the same fields:

| Option | Meaning |
|---|---|
| `enabled` | Enables the panel at startup. Required panels may still be shown by the layout. |
| `title` | Border title. |
| `min_width` | Minimum useful width in columns. |
| `min_height` | Minimum useful height in rows. |
| `priority` | Higher priority panels survive first when space is limited. |
| `visible_modes` | Responsive modes where the panel may show. Empty means all applicable modes. |
| `border_style` | `plain` (default), `rounded`, `double`, `thick`, or `none`. Borderless panels retain their title and one-cell content inset. |
| `alignment` | Text alignment: `left` (default), `center`, or `right`. Map geometry, output map snapshots, and full-width gauge rows retain their layout. |
| `refresh_ms` | Minimum content-refresh interval, 0–60000 ms; `0` (default) renders each frame. Input must use `0`. |
| `theme` | Partial color overrides in `[panels.<name>.theme]`; omitted colors inherit `[colors]`. |

Panel tables are `panels.info`, `panels.social`, `panels.map`, `panels.opponent`, `panels.group`, `panels.character`, `panels.output`, and `panels.input`. The `panels.info` table controls the World pane title and compact sidebar height. The `panels.social` table controls the ultrawide Social dashboard pane.

Accepted `visible_modes` values are `mobile`, `tablet`, `1080p`, and `ultrawide`. Legacy aliases `tiny`, `compact`, `standard`, and `wide` are also accepted.

For example, edit the existing panel table (do not duplicate it):

```toml
[panels.social]
enabled = true
title = "Social"
min_width = 30
min_height = 5
priority = 95
visible_modes = ["ultrawide"]
border_style = "rounded"
alignment = "left"
refresh_ms = 250

[panels.social.theme]
background = "#101820"
foreground = "#d0e8ff"
border = "cyan"
title = "lightcyan"
```

Theme keys are `background`, `foreground`, `border`, `title`, `accent`, `success`, `warning`, `danger`, `muted`, `player`, and `enemy`; unknown keys and invalid colors are rejected. Explicit MUD ANSI colors, highlights, map marker/terrain colors, and existing gauge-specific colors retain precedence over generic foreground overrides. The compact status strip is not a bordered panel and continues to use the global theme. Nearby Map shares `panels.map` styling and refresh settings, with its existing distinct title.

Refresh intervals throttle only rendering, never networking, scripting, or state updates. A due panel refreshes on the next application draw, so the timer cadence (`terminal.tick_rate_ms`) and event traffic also affect observed refresh timing. Cached content is copied into every frame; it is not erased between refreshes. Keyboard/mouse input, resize, successful `/reload`, changed panel settings/geometry, and hiding/revealing a panel invalidate cached content. Input and completion remain immediate. Use short intervals for output or combat panels if delayed information is undesirable.

Responsive layout still controls placement, dimensions, visibility, and priority. Style overrides do not force hidden panels into small layouts or change their minimum sizes. `/reload` applies changes; invalid configuration leaves the current settings active. See `/help panels` for a short in-client reference.

Aligned command/search input reserves one content cell for the insertion cursor and scrolls horizontally when needed to keep it visible. Titles stay left-aligned; `alignment` affects content, not border titles.

## social

| Option | Meaning |
|---|---|
| `scrollback_lines` | Maximum retained Social panel messages. |

The Social panel captures incoming and outgoing tells, chats, says, narrates, group-says, yells, and sings with local machine `HH:MM` timestamps. It extracts the text inside RoTS single quotes and displays messages as `[HH:MM](channel) Prefix Text`. Other players are prefixed with their name, tells use `from Name -` or `to Name -`, and your own chat/narrate/sing/yell/say/group-say omit the name prefix. Mouse wheel scrolling over the Social panel moves through previous captured messages.

## map

| Option | Meaning |
|---|---|
| `show_links` | Draws links between rooms. |
| `room_spacing_columns` | Horizontal distance between room centers. Must leave enough space for links and doors. |
| `room_spacing_rows` | Vertical distance between room centers. |
| `current_room_symbol` | One-cell symbol for the current room. |
| `current_room_color` | Color for the current room symbol. |
| `stub_symbol` | One-cell symbol for unknown or stub exits. |
| `link_color` | Default link color. Terrain-specific road and city rendering can override link appearance. |
| `avoid_color` | Color for avoid-flagged rooms and links. |
| `block_color` | Color for block-flagged rooms and links. |
| `hide_color` | Color reserved for hidden map elements. Hidden rooms are normally not rendered. |
| `invis_color` | Color for invis-flagged rooms and links. |
| `fog_color` | Color for fog-flagged rooms and links. |
| `void_color` | Color for void connector links and current void rooms. |

### map.persistence

| Option | Meaning |
|---|---|
| `load_on_startup` | Loads the configured map file when the client starts. Errors are shown in the output pane and do not stop startup. |
| `path` | Map TOML path. Relative paths resolve beside the active `config.toml`; interactive `/map read` and `/map write` paths use the same base. Without a config file path they resolve from the current working directory. |
| `save_on_exit` | Saves the current map to `path` during graceful client shutdown. |

### map.doors

Doors use one shared glyph and state-specific colors so door identity stays consistent.
Door movement behavior is state-driven: `trigger`, `unknown`, and `open` add no automatic command; `closed` opens the named door; `pickable` picks the named door; `locked` unlocks and then opens the named door.

| Option | Meaning |
|---|---|
| `show` | Enables door glyphs on map links. |
| `glyph` | One-cell door symbol. Default is `╬`. |
| `open_color` | Door exists and is open. |
| `closed_color` | Door is closed. |
| `pickable_color` | Door can be picked. |
| `locked_color` | Door needs a key. |
| `trigger_color` | Door is opened by a trigger such as a lever or spoken word. |
| `unknown_color` | Door state is unknown. |

### map.teleport

Teleport markers show exits flagged with `/map exitflag <dir> teleport on`.

| Option | Meaning |
|---|---|
| `show` | Enables teleport glyphs on map links. |
| `glyph` | One-cell teleport symbol. Default is `◇`. |
| `color` | Teleport glyph color. Default is cyan. |

### map.terrain

Terrain tables are keyed by terrain name, for example `[map.terrain.Forest]` or `[map.terrain.Dense_forest]`.

| Option | Meaning |
|---|---|
| `symbol` | Terrain glyph. One terminal cell unless `double = true`. |
| `color` | Terrain glyph color. |
| `density` | Decorative density: `dense`, `sparse`, `scant`, or empty to disable density fill. |
| `spread` | Decorative spread: `narrow`, `wide`, `vast`, or empty. |
| `fade` | Decorative fade: `fadein`, `fadeout`, or empty. |
| `double` | True when `symbol` is two one-cell characters. |

Configured terrain names in the default file are `City`, `Road`, `Floor`, `Field`, `Forest`, `Dense_forest`, `Hills`, `Mountain`, `Water`, `Water_noswim`, `Underwater`, `Swamp`, and `Crack`.

Terrain also controls path weight for `/map find` and `/map run`: Floor `1`, Road `2`, Field `3`, Forest `4`, Hills `5`, Dense Forest `6`, Mountains `7`, Swamp `8`, Water `9`, Water Noswim `10`, Underwater `11`, and Crack `12`.

## msdp

| Option | Meaning |
|---|---|
| `client_id` | Client name sent during MSDP negotiation. |
| `client_version` | Client version sent during MSDP negotiation. |
| `ansi_colors` | Requests ANSI color support. |
| `xterm_256_colors` | Requests 256-color support. |
| `utf_8` | Requests UTF-8 and encodes outgoing commands as UTF-8 when `true`. The RoTS default is `false` because the server emits raw ISO-8859-1; use `true` for a server that actually speaks UTF-8. |
| `report_variables` | MSDP variable names to request from the server. Required mapped variables are added automatically if omitted. |

### msdp.mapping

The mapping table converts server-specific MSDP names into client state fields.

Character fields: `character_name`, `race`, `level`, `health`, `health_max`, `mana`, `mana_max`, `movement`, `movement_max`, `experience`, `experience_max`, `offensive_bonus`, `dodge`, `parry`, `attack_speed`, `strength`, `intelligence`, `will`, `dexterity`, `constitution`, `learning`, `willpower`, `spell_save`, `spirit`, `spell_power`, `spell_pen`, `warrior_level`, `ranger_level`, `mystic_level`, `mage_level`, `health_regeneration`, `stamina_regeneration`, and `movement_regeneration`. RoTS reports mana regeneration through `STAMINA_REGENERATION`.

Opponent fields: `opponent_name`, `opponent_health`, `opponent_health_max`, and `opponent_level`.

Room and world fields: `room`, `room_name`, `room_vnum`, `room_exits`, `world_time`, and `weather`.

Group fields: `group`, `group_members`, `group_member_name`, `group_member_health`, `group_member_mana`, and `group_member_movement`.

For RoTS mapping, the mapper is driven by the MSDP `ROOM` table plus `ROOM_EXITS`; displayed room text is not used for map topology.

## variables

| Option | Meaning |
|---|---|
| `max_expansion_depth` | Maximum nested `${name}` expansion depth. |
| `max_expanded_bytes` | Maximum expanded template size. |

`[variables.values]` contains user-defined persistent variables. Runtime `/variable` values override config values for the current session and survive `/reload`.

## aliases

| Option | Meaning |
|---|---|
| `enabled` | Enables configured and runtime aliases. |
| `max_expansion_depth` | Maximum recursive alias expansion depth. |
| `max_expanded_commands` | Maximum commands produced by alias expansion. |

### aliases.rules

| Option | Meaning |
|---|---|
| `name` | Display name. Must not be empty. |
| `enabled` | Enables this alias. Defaults to true. |
| `priority` | Higher priority rules match first. |
| `match_type` | `exact`, `prefix`, or `regex`. |
| `pattern` | Text or regex pattern to match command input. |
| `commands` | Commands to enqueue after capture and variable expansion. |
| `lua` | Optional Lua function name to call instead of or in addition to commands. |

An alias must define at least one `commands` entry or `lua` hook.

## lua

| Option | Meaning |
|---|---|
| `enabled` | Enables Lua runtime loading and Lua hooks. |
| `script_dir` | Directory containing Lua scripts. Relative paths resolve beside the active config file, or from the platform config directory (normally `~/.config/mud-client`) when the default config is used. |
| `entrypoint` | Legacy Lua file loaded from `script_dir`, usually `init.lua`; used only when `scripts` is omitted. |
| `scripts` | Optional nonempty ordered list of Lua files under `script_dir`; overrides `entrypoint`. Files share a Lua environment and duplicate canonical paths load once. Explicitly listed missing files fail loading. |
| `instruction_budget` | Instruction limit for hooks and script loading to stop runaway scripts. Must be greater than zero. |
| `max_actions_per_hook` | Maximum actions a hook may enqueue. Must be greater than zero. |
| `runtime_errors_to_output` | Shows Lua hook errors in the MUD output pane. |

Example:

```toml
[lua]
enabled = true
script_dir = "scripts"
entrypoint = "init.lua"
instruction_budget = 100000
max_actions_per_hook = 32
runtime_errors_to_output = true
```

Lua function names are referenced by aliases, triggers, and event handlers:

```toml
[[aliases.rules]]
name = "smart-kill"
match_type = "regex"
pattern = "^sk\\s+(.+)$"
lua = "smart_kill"
```

See [Lua API Reference](lua-api.md) for every `client` API with examples.

## triggers

| Option | Meaning |
|---|---|
| `enabled` | Enables configured and runtime triggers. |
| `max_commands_per_line` | Maximum total actions allowed for one trigger rule. |

### triggers.rules

| Option | Meaning |
|---|---|
| `name` | Display name. Must not be empty. |
| `enabled` | Enables this trigger. Defaults to true. |
| `priority` | Higher priority rules run first. |
| `match_type` | `plain` substring matching or `regex`. |
| `pattern` | Text or regex matched against normalized MUD output. |
| `foreground` | Optional required ANSI foreground color. |
| `background` | Optional required ANSI background color. |
| `commands` | Commands to enqueue when the trigger matches. |
| `event` | Event name to emit when the trigger matches. |
| `lua` | Lua function name to call when the trigger matches. |
| `cooldown_ms` | Minimum milliseconds between matches for this rule. |
| `one_shot` | Disables the trigger after its first match. |
| `categories` | Output categories this trigger applies to. Empty means default categories. |

Output categories include normal MUD output, prompts, system messages, errors, combat, communication, snapshots, and triggered output where produced by the client.

## events

| Option | Meaning |
|---|---|
| `enabled` | Enables configured and runtime event handlers. |
| `max_dispatch_depth` | Maximum nested dispatch depth. |
| `max_events_per_dispatch` | Maximum emitted events processed in one dispatch cascade. |
| `max_commands_per_dispatch` | Maximum commands a handler may enqueue. |
| `history_limit` | Number of recent events retained for `/event`. |

### events.handlers

| Option | Meaning |
|---|---|
| `name` | Display name. Must not be empty. |
| `enabled` | Enables this handler. Defaults to true. |
| `priority` | Higher priority handlers run first. |
| `match_type` | `plain` exact event name or `regex`. |
| `event` | Event name or regex pattern. |
| `commands` | Commands to enqueue when the handler matches. |
| `emit` | Follow-up events to emit. |
| `notification` | Local notification text to append to output. |
| `lua` | Lua function name to call when the handler matches. |

An event handler must define at least one command, emitted event, notification, or Lua hook.

## highlights

| Option | Meaning |
|---|---|
| `enabled` | Enables configured and runtime highlights. |

### highlights.rules

| Option | Meaning |
|---|---|
| `name` | Display name. Must not be empty. |
| `enabled` | Enables this highlight. Defaults to true. |
| `priority` | Higher priority rules style first. |
| `match_type` | `plain` substring matching or `regex`. |
| `pattern` | Text or regex matched against normalized MUD output. |
| `foreground` | Optional foreground color. |
| `background` | Optional background color. |
| `bold`, `dim`, `italic`, `underline`, `reverse` | Optional terminal text styles. |
| `categories` | Output categories this highlight applies to. Empty means default categories. |

A highlight must define at least one color or style.

## logging

| Option | Meaning |
|---|---|
| `level` | Tracing filter, for example `mud_client=info` or `mud_client=debug`. |
| `raw_protocol` | Enables low-level protocol diagnostics when supported. Keep false unless debugging. |

## Validation Notes

- Required names and patterns must not be empty.
- Regex patterns are compiled during config validation.
- Map room, stub, door, and teleport glyphs must occupy one terminal cell.
- Double terrain symbols must be exactly two one-cell characters.
- Required panel visibility cannot remove the map, output, or command input from all modes.
- Lua, trigger, event, variable, animation, and terminal limits must be greater than zero.
