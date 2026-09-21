# Mapper

The mapper is driven by RoTS MSDP data. Movement commands are sent to the MUD, but they do not move the local map by themselves. The current room, exits, and links update when fresh MSDP room data arrives.

## MSDP Source Data

Required mapping values are configured under `[msdp.mapping]`:

```toml
[msdp.mapping]
room = "ROOM"
room_name = "ROOM_NAME"
room_vnum = "ROOM_VNUM"
room_exits = "ROOM_EXITS"
```

The mapper uses the MSDP `ROOM` table for destination room IDs and the `ROOM_EXITS` array for direction order. Displayed room text can be turned off in-game and is not used for topology.

## Current Room Display

The map header shows the current room name and exits. The current room marker uses `map.current_room_symbol` and `map.current_room_color`.

Up and down exits are shown as vertical-layer indicators. Moving through `u` or `d` changes the visible z layer so same-layer rooms connected to the new room remain visible while rooms on the previous layer disappear.

When multiple mapped rooms resolve to the same display position, the map keeps the first room reached from the current room and hides later overlapping rooms and their branches. This matches TinTin-style map display behavior and keeps overlapping loops from drawing unrelated rooms over the current route.

## Commands

```text
/map
/map get
/map map
/map create
/map goto <vnum|name> [dig]
/map move <dir>
/map dig <dir> [new|vnum]
/map link <dir> <vnum> [both]
/map unlink <dir>
/map delete <dir|vnum>
/map undo
/map return
/map leave
/map door <dir> [trigger|unknown|open|closed|pickable|locked|none] [name]
/map set roomname <name>
/map set roomdesc <description>
/map set roomarea <area>
/map set roomnote <note>
/map set roomterrain <name>
/map set roomsymbol <symbol>
/map set roomweight <value>
/map flag <static|nofollow|direction|unicode|asciigraphics|asciivnums> [on|off]
/map roomflag [<avoid|block|curved|fog|hide|invis|leave|noglobal|static|void>[;...] [on|off|get <variable>]]
/map exitflag <dir> <avoid|block|hide|invis|teleport> [on|off]
/map list [query]
/map landmark [query]
/map landmarks [query]
/map landmark <name> <vnum> [description] [size]
/map unlandmark <name-or-pattern>
/map find <vnum|name>
/map run <vnum|name>
/map write <file>
/map read <file>
```

`/map get` displays the current room, terrain, and path weight. `/map run` sends the optimized lowest-cost path to the MUD.

## Command Details

| Command | Description |
|---|---|
| `/map` / `/map help` | Show structured map help. |
| `/map map` | Append a full MUD-output-pane map snapshot to scrollback. |
| `/map get` / `/map info` | Show current room id, name, coordinates, area, terrain, weight, and exits. |
| `/map list [query]` | List up to 12 mapped rooms matching id, name, area, description, note, or terrain. |
| `/map landmark [query]` / `/map landmarks [query]` | List landmarks, optionally filtered by name. |
| `/map landmark <name> <vnum> [description] [size]` | Create or update a named landmark pointing to a mapped room. |
| `/map unlandmark <name-or-pattern>` | Remove landmarks matching an exact name or `*`/`?` wildcard pattern. |
| `/map create` | Clear in-memory map data and create room `1`. |
| `/map goto <vnum|name> [dig]` | Select an existing room; `dig` creates it if missing. |
| `/map move <dir>` | Move only the local mapper. |
| `/map dig <dir> [new|vnum]` | Create a new generated room or link to a provided room id. |
| `/map link <dir> <vnum> [both]` | Link an exit to a target room; `both` creates the reverse exit. |
| `/map unlink <dir>` | Remove one exit from the current room. |
| `/map delete <dir|vnum>` | Delete an exit or remove a room and incoming links to it. |
| `/map undo` | Undo the last mapper-created move. |
| `/map return` | Restore the previous mapper room. |
| `/map leave` | Clear current mapper location while saving it for `/map return`. |
| `/map set <option> <value>` | Set current-room metadata. Options are listed below. |
| `/map flag <name> [on|off]` | Toggle mapper-wide behavior. |
| `/map roomflag [flag[;...] [on|off|get <variable>]]` | List, toggle, set, or read TinTin-style room flags on the current room. |
| `/map exitflag <dir> <flag> [on|off]` | Toggle an exit flag on a mapped exit. |
| `/map door <dir> [state|none] [name]` | Set or clear a directional door state and optional name. Omitted state means `closed`. |
| `/map find <vnum|name>` | Display the optimized weighted path to a target room. |
| `/map run <vnum|name>` | Send each movement command in the optimized weighted path. |
| `/map read <file>` | Load map state from TOML. |
| `/map write <file>` | Save map state as TOML, creating parent directories if needed. |

## Room Flags

Room flags follow TinTin++ mapper behavior where it applies to this client:

| Flag | Behavior |
|---|---|
| `avoid` | Excluded from optimized paths. |
| `block` | Treated as blocked for optimized paths. |
| `curved` | Stored as display metadata for curved link rendering. |
| `fog` | Displays the room, but map traversal does not continue past it unless it is the current room. |
| `hide` | Hidden from map display and display traversal. |
| `invis` | Displays normally with the configured invisible color. |
| `leave` | Entering the room through mapper movement leaves the map. |
| `noglobal` | Stored for TinTin compatibility; global exits are not implemented yet. |
| `static` | Prevents automatic room creation from this room. |
| `void` | Acts as connector geometry: near-zero path cost and overlap-friendly tunnel traversal. |

Examples:

```text
/map roomflag
/map roomflag avoid on
/map roomflag avoid;fog off
/map roomflag block get is_blocked
```

## Metadata Options

`/map set <option> <value>` accepts:

| Option | Aliases | Description |
|---|---|---|
| `roomname` | `name` | Room display name. |
| `roomdesc` | `desc`, `description` | Room description. |
| `roomarea` | `area` | Area name. |
| `roomnote` | `note` | Mapper note. |
| `roomterrain` | `terrain` | Terrain type. Also updates the default path weight for known RoTS terrain names. |
| `roomsymbol` | `symbol` | Custom room symbol, truncated to three characters. |
| `roomweight` | `weight` | Numeric pathing weight used by `/map find` and `/map run`. |

## Flags

Mapper-wide flags:

- `static`
- `nofollow`
- `direction`
- `unicode`
- `asciigraphics`
- `asciivnums`

Room flags:

- `avoid`
- `block`
- `curved`
- `fog`
- `hide`
- `invis`
- `leave`
- `noglobal`
- `void`
- `static`

Exit flags:

- `avoid`
- `block`
- `hide`
- `invis`
- `teleport`

Omitting `on` or `off` toggles the current flag value.

## Doors

Doors exist once between two connected rooms on the rendered link. Door state and name are stored on the room where the user set them because doors can be named differently from each side.

The default door glyph is `╬`. State is shown by color:

- trigger
- unknown
- open
- closed
- pickable
- locked
- none, clear, or off to remove door data

Automatic movement commands only run for named doors:

- `trigger`, `unknown`, and `open` add no special commands.
- `closed` sends `open <name> <direction>` before movement.
- `pickable` sends `pick <name> <direction>` before movement.
- `locked` sends `unlock <name> <direction>` and then `open <name> <direction>` before movement.

For example, `/map door w locked gate` followed by `w` sends `unlock gate w`, `open gate w`, and then `w`.

Customize it in `[map.doors]`; see [Configuration Reference](configuration.md).

## Teleports

Some directional exits teleport instead of physically connecting adjacent rooms. Mark those exits with:

```text
/map exitflag <dir> teleport on
```

The default teleport glyph is `◇`, rendered on the exit link. Customize it in `[map.teleport]`.

## Terrain

Terrain controls room appearance and path weight. Defaults:

| Terrain | Weight |
|---|---:|
| Floor | 1 |
| Road | 2 |
| Field | 3 |
| Forest | 4 |
| Hills | 5 |
| Dense Forest | 6 |
| Mountains | 7 |
| Swamp | 8 |
| Water | 9 |
| Water Noswim | 10 |
| Underwater | 11 |
| Crack | 12 |

Road and City terrain default to white and use joined line glyphs where possible so important routes read as continuous paths. Route glyphs consider incoming and outgoing mapped links, which keeps one-way MSDP exit reports from making road segments look disconnected.

Terrain density, spread, fade, symbol, color, and double-width behavior are configured under `[map.terrain.<name>]`.

## Persistence

Use `/map write <file>` to save map data and `/map read <file>` to load it. Runtime state and config are separate: config controls display defaults, while map files contain discovered rooms and links.

Map files can also be loaded at startup:

```toml
[map.persistence]
load_on_startup = true
path = "maps/rots.toml"
save_on_exit = false
```

Relative map file paths, including interactive `/map read` and `/map write` paths, resolve beside the active `config.toml`. Absolute paths remain unchanged. If no config file path is active, relative paths use the current working directory.
