# Command Reference

Local client commands start with `/`. Commands without `/` are processed through variables, aliases, semicolon splitting, command echo, and then sent to the MUD.

## Base Commands

| Command | Description |
|---|---|
| `/help` | Show command help. |
| `/help <topic>` | Show topic help such as `/help map`, `/help alias`, `/help map door`, or `/help event lua`. |
| `/clear` | Clear the output pane. |
| `/quit` | Quit the client. |
| `/reload` | Reload config and Lua from disk. |
| `/reconnect` | Request a network reconnect. |
| `/msdp` | Display all currently stored MSDP values. |
| `/toggle <panel> [on|off]` | Toggle optional UI panels for this session. Supported panels include `group`, `opponent`, and `social`. |

`Esc` is not mapped to quit.

Examples:

```text
/help
/help map door
/clear
/reload
/reconnect
/msdp
/toggle group on
/toggle opponent off
/toggle social
```

## Input Behavior

| Input | Behavior |
|---|---|
| `Enter` | Send the current command. If the input is empty, send a blank line to the MUD. |
| `Enter` on highlighted last command | Resend the last command. |
| `;` | Split one input line into multiple MUD commands. |
| `#<count> {command}` | Repeat one command or braced command group. |
| `Up` / `Down` | Move through command history. Typed text filters history by prefix. |
| `Left` / `Right` / `Home` / `End` | Edit the current command. |
| `Tab` | Complete the current word from recent MUD output. |
| `Shift-Tab` | Cycle completion backward. |
| `PageUp` / `PageDown` | Scroll MUD output. |
| Mouse wheel over output | Scroll MUD output. |
| Drag main divider | Resize side panes for the current session when mouse mode is enabled. |
| `Ctrl-Up` / `Ctrl-Down` | Scroll output one line. |
| `Ctrl-E` | Follow newest output. |
| `Ctrl-L` | Clear output. |
| `Ctrl-F` | Search output. |
| `Ctrl-N` / `Ctrl-P` | Next or previous search match. |

MUD output scrolling clamps to the oldest full visible page instead of scrolling beyond retained text into blank space.
| `F2` | Cycle styled, plain, and debug output views. The title indicator appears briefly. |
| `Ctrl-C` | Quit. |

Repeat examples:

```text
#10 {kill orc}
#3 {look;score}
#2 {look;score};rest
```

The semicolon after a repeated `{}` group starts a new command. In `#2 {look;score};rest`, only `look;score` repeats; `rest` is sent once. To repeat a later command too, prefix that command with its own repeat count.

## Scripting Commands

| Command | Description |
|---|---|
| `/variable` | List effective variables. |
| `/variable {name} {value}` | Set a runtime variable. |
| `/variable unset {name}` | Remove a runtime variable. |
| `/alias` | List aliases. |
| `/alias {pattern} {command} [{command}...]` | Add or replace an in-memory alias. |
| `/alias unset {pattern}` | Remove an in-memory alias. |
| `/alias clear` | Remove all in-memory aliases. |
| `/triggers` | List triggers. |
| `/triggers [plain|regex] [--fg color] [--bg color] {pattern} {command}...` | Add or replace an in-memory trigger. |
| `/triggers unset {pattern}` | Remove an in-memory trigger. |
| `/triggers clear` | Remove all in-memory triggers. |
| `/highlight` | List highlights. |
| `/highlight [plain|regex] {pattern} {fg|none} [bg|none] [styles]` | Add or replace an in-memory highlight. |
| `/highlight unset {pattern}` | Remove an in-memory highlight. |
| `/highlight clear` | Remove all in-memory highlights. |
| `/event` | Show event summary and recent history. |
| `/event {name}` | Emit an event manually. |
| `/handler` | List event handlers. |
| `/handler [plain|regex] {event} {command}...` | Add or replace an in-memory event handler. |
| `/handler unset {event}` | Remove an in-memory event handler. |
| `/handler clear` | Remove all in-memory event handlers. |
| `/lua status` | Show Lua runtime status. |
| `/lua reload` | Reload Lua entrypoint. |
| `/lua call <function>` | Call a no-argument Lua function. |
| `/echo [--fg color] [--bg color] text` | Append local output without sending it to the MUD. |

Examples:

```text
/variable
/variable {target} {Orc warlord}
/variable unset {target}
/alias
/alias {k} {kill {1}}
/alias {rr} {recall} {look}
/alias unset {k}
/triggers
/triggers plain {You are hungry} {eat bread}
/triggers regex {^(.+) arrives\.$} {target {1}} {consider {1}}
/triggers plain --fg lightred --bg index:17 {Danger} {flee}
/triggers unset {You are hungry}
/highlight
/highlight {You are hit} {red}
/highlight regex {You receive \d+ gold} {yellow} {none} {bold}
/highlight unset {You are hit}
/event
/event {LowHealth}
/handler
/handler {LowHealth} {flee}
/handler regex {^EnemyEntered:(.+)$} {target {1}}
/handler unset {LowHealth}
/lua status
/lua reload
/lua call smoke_test
/echo --fg yellow Script loaded
```

## Map Commands

Common mapper commands are documented in [Mapper](mapping.md).

| Command | Description |
|---|---|
| `/map` | Show map help. |
| `/map get` | Show current room details, including terrain and path weight. |
| `/map create` | Create or reset local map state. |
| `/map map` | Append a map snapshot to the output pane. |
| `/map goto <vnum|name> [dig]` | Move local map focus to an existing room, or create it with `dig`. |
| `/map move <direction>` | Move only the local mapper; does not send movement to the MUD. |
| `/map dig <direction> [new|vnum]` | Create or link a room from the current room. |
| `/map link <direction> <vnum> [both]` | Link the current room to a target room, optionally with a reverse exit. |
| `/map unlink <direction>` | Remove an exit link from the current room. |
| `/map delete <direction|vnum>` | Delete an exit from the current room or delete a room. |
| `/map undo` | Undo the last mapper-created move. |
| `/map return` | Return to the previously tracked mapper room. |
| `/map leave` | Leave the map while remembering the current room for `/map return`. |
| `/map set <option> <value>` | Set current-room metadata. |
| `/map set roomterrain <name>` | Set the current room terrain. |
| `/map set roomweight <value>` | Set the current room path weight. |
| `/map flag <name> [on|off]` | Toggle mapper-wide flags. |
| `/map roomflag [flag[;...] [on|off|get <variable>]]` | List, toggle, set, or read TinTin-style room flags on the current room. |
| `/map exitflag <direction> <flag> [on|off]` | Toggle a flag on an exit from the current room. Flags include `avoid`, `block`, `hide`, `invis`, and `teleport`. |
| `/map door <direction> [state|none] [name]` | Set or clear a directional door state and optional name on an exit. Closed, pickable, and locked named doors expand movement into open, pick, or unlock/open commands. |
| `/map list [query]` | List mapped rooms, optionally filtered. |
| `/map find <vnum|name>` | Show an optimized route to a room. |
| `/map run <vnum|name>` | Send the optimized route to the MUD. |
| `/map read <file>` | Load a map file. |
| `/map write <file>` | Save a map file. |

Examples:

```text
/map
/map get
/map create
/map map
/map goto 2942
/map goto Bree dig
/map move n
/map dig e
/map dig s 2811
/map link w 2800 both
/map unlink e
/map delete w
/map delete 2811
/map undo
/map return
/map leave
/map set roomname A Shadowy Forest
/map set roomterrain Forest
/map set roomweight 4
/map flag nofollow on
/map roomflag avoid on
/map roomflag fog;void on
/map roomflag block get is_blocked
/map exitflag u teleport on
/map door w closed gate
/map door n locked iron door
/map door e trigger hidden lever
/map door s none
/map list forest
/map find Bree
/map run Bree
/map read maps/rots.toml
/map write maps/rots.toml
```
