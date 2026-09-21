# `/map`

## Purpose

Manage the local room graph, landmarks, room metadata, flags, persistence, and weighted destination routing. Map commands never send their slash syntax to the MUD.

## Syntax

```text
/map
/map <subcommand> [arguments]
```

## Common workflow

```text
/map create
/map goto 1
/map dig n
/map dig e
/map landmark Home 2 My starting room
/map landmarks
/map find Home
/map run Home
/map write maps/rots.toml
```

## Subcommands

- `/map create` resets the local map and creates room `1`.
- `/map get` or `/map info` displays the current room, exits, terrain, and weight.
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

Room flags are `avoid`, `block`, `curved`, `fog`, `hide`, `invis`, `leave`, `noglobal`, `static`, and `void`. Exit flags are `avoid`, `block`, `hide`, `invis`, and `teleport`. Door states are `trigger`, `unknown`, `open`, `closed`, `pickable`, `locked`, and `none`.
