# `/map`

## Purpose

Manage the local room graph, landmarks, room metadata, flags, persistence, and weighted destination routing. Map commands never send their slash syntax to the MUD.

## Syntax

```text
/map
/map <subcommand> [arguments]
```

## Common workflow

Use this on a new/test map: `/map create` resets the current local map. Back up an existing map with `/map write maps/backup.toml` first. The workflow edits the mapper; it does not establish that your character is physically in these rooms. Run each line separately.

```text
/map create
/map goto 1
/map dig n
/map dig e
/map landmark Home 2 My starting room
/map landmarks
/map find Home
/map write maps/rots.toml
```

This creates a test graph, names room `2` as `Home`, previews a route, and saves the map. Only use `/map run Home` after confirming the map matches your character's actual location; unlike `/map find`, it sends movement commands.

## Subcommands

- `/map create` resets the local map and creates room `1`.
- `/map get` or `/map info` displays the current room, exits, terrain, weight, symbol, and note.
- `/map map` appends a full map snapshot to output.
- `/map goto <vnum|name> [dig]` selects a room; `dig` creates a missing room.
- `/map move <direction>` changes mapper state only.
- `/map dig`, `/map link`, and `/map unlink` edit exits.
- `/map set roomterrain <name>` and `/map set roomweight <number>` control path costs.
- `/map flag <name> [on|off]` changes mapper-wide behavior.
- `/map roomflag <flag> [on|off]` changes current-room flags.
- `/map exitflag <direction> <flag> [on|off]` changes exit flags.
- `/map door <direction> [state|none] [name]` stores directional door metadata.
- `/map list [query]` lists matching rooms.
- `/map landmark <name> <vnum> [description] [size]` creates or updates a landmark.
- `/map landmarks [query]` lists landmarks.
- `/map unlandmark <name-or-pattern>` removes matching landmarks.
- `/map find <target>` displays the lowest-cost route.
- `/map run <target>` sends that route.
- `/map read <file>` and `/map write <file>` load or save TOML map state.

## Room notes and visible legend

While standing in the innkeeper's room, run each line separately:

```text
/map set roomsymbol Zzz
/map set roomnote Dol-Goldur Innkeeper
/map info
/map write maps/rots.toml
```

The regular map shows `Zzz` and its legend shows `Zzz - Dol-Goldur Innkeeper` while that room is visible. The current room keeps its player marker; its legend still uses `Zzz`. Notes without a custom symbol use the room's ordinary terrain marker. Rooms without nonblank notes never appear in the legend. Notes and symbols survive MSDP updates and are saved with the map, not `/save`.

The legend lists at most three visible noted rooms, nearest first by straight-line room-grid distance from the player (room ID breaks ties), with `+N more` for overflow. Rooms outside the drawing viewport or on another layer, and hidden/void rooms excluded by the map renderer, are omitted. Separate rooms retain separate entries even if their notes match. Long notes are clipped without wrapping.

The live legend updates as you move, resize, or edit notes. `/map map` captures the same legend in a historical snapshot; the dense Nearby Map ignores custom room symbols and uses configured terrain/stub symbols and its existing connected-road markers, retaining the configured player marker. The footer reserves up to four rows, capped at one-third of the inner pane height, and stays blank if no rooms qualify. Smaller panes show fewer entries with an overflow count; the footer is hidden if fewer than two rows are available. Disable it with `[map] show_legend = false` and `/reload`.

Symbols accept up to three single-cell, non-control characters. Unsupported widths or longer symbols are rejected without changing the previous value. Clear either field by omitting its value:

```text
/map set roomsymbol
/map set roomnote
/map write maps/rots.toml
```

`symbol` and `note` are aliases for `roomsymbol` and `roomnote`.

## Hidden exits and MSDP

MSDP reports add or update exits; they do not remove known exits just because the server omits them (including empty reports). Manually linked hidden passages and their door/flag metadata survive leaving and re-entering a room. Explicitly reported destinations can still update existing links.

For example, while in the source room, `/map link n 2942 both` records a hidden north exit and its reverse link. Use `/map unlink n` or `/map delete n` to remove the current room's exit deliberately; remove the reverse link separately if needed. A later server report can relearn an explicitly removed exit. Save the graph with `/map write maps/rots.toml` for reuse after restarting.

## Door symbols and gates

Set a door's state and name, then optionally tag it as a gate. The gate tag is independent of open/closed/pickable/locked state, so movement commands still use that state.

```text
/map door n locked iron gate
/map exitflag n gate on
/map door n open
/map exitflag n gate off
```

These commands require an existing mapped north exit. Gate tags and door metadata are directional: set them on the reverse exit separately if needed. `gate off` restores the ordinary door symbol without changing its state. `/map door n none` removes the door marker; the gate tag remains available if a door is set again. Save with `/map write maps/rots.toml`.

Customize `[map.doors]` in `config.toml`, then `/reload`. The supplied configuration uses `□` open (green), `▣` closed (yellow), `⊞` pickable (yellow), and `⊠` locked (red). Tagged gates use `╪` north/south or `╫` east/west, retaining the door-state color. Diagonal and up/down gates use the ordinary state symbol. Trigger/unknown doors use the shared `glyph`; absent per-state settings also fall back to that shared symbol. Gate tagging alone does not create a door.

## Routing examples

```text
/map set roomterrain Swamp
/map set roomweight 8
/map roomflag avoid on
/map exitflag e block on
/map find 2942
/map run Home
```

`/map find` and `/map run` use room weights and skip blocked or avoided rooms and exits. This is destination routing; for recording a route manually, use [`/path`](path.md).

Relative map file paths resolve beside the active `config.toml`, normally `~/.config/mud-client`.

Room flags are `avoid`, `block`, `curved`, `fog`, `hide`, `invis`, `leave`, `noglobal`, `static`, and `void`. Exit flags are `avoid`, `block`, `hide`, `invis`, `teleport`, and `gate`. Door states are `trigger`, `unknown`, `open`, `closed`, `pickable`, `locked`, and `none`.
