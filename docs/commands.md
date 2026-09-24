# Command Reference

Every local command starts with `/` and is handled by the client instead of being sent to the MUD. This page is the command index; each command has a dedicated reference page with syntax, behavior, failure cases, and examples.

## Quick examples

Type one command at a time into the client and press Enter. These are independent examples, **not a script to paste in full**. Commands inside `{braces}` retain the braces; angle-bracket placeholders in syntax sections must be replaced with your own values. MUD commands such as `look` and `say` depend on your server.

| Command | Example | What it does |
|---|---|---|
| Help | `/help panels` | Shows panel settings and configuration examples. |
| Clear | `/clear` | Removes retained output; does not disconnect. |
| Quit | `/quit` | Disconnects and closes the client. |
| Reload | `/reload` | Applies your saved configuration changes. |
| Save | `/save` | Saves current runtime variables, macros, aliases, triggers, and highlights for restart. |
| Reconnect | `/reconnect` | Reports a request only; currently requires a client restart to reconnect. |
| MSDP | `/msdp` | Shows server variables received so far. |
| Toggle | `/toggle group on` | Enables the Group panel where the layout allows it. |
| Echo | `/echo --fg yellow Ready` | Prints a yellow local message. |
| Macro | `/macro {F5} {look}` | Makes F5 send `look`; press F5 to try it. |
| Variable | `/variable {target} {orc}` | Stores `orc` for later `${target}` expansion. |
| Alias | `/alias {rr} {look}` | Makes typing `rr` send `look`. |
| Trigger | `/triggers plain {You are hungry} {eat bread}` | Sends `eat bread` when matching server output arrives. |
| Highlight | `/highlight {You are hit} {red}` | Colors matching output red. |
| Event | `/event {ExampleNotice}` | Emits a local event; matching handlers may execute commands. |
| Handler | `/handler {ExampleNotice} {/echo Event received}` | Prints a message when you subsequently emit `ExampleNotice`. |
| Lua | `/lua status` | Shows Lua configuration/loading status without calling a script. |
| Timer | `/timer list` | Lists scheduled Lua callbacks; see the timer page to create one. |
| Map | `/map info` | Inspects the current mapped room without moving your character. |
| Path | `/path describe` | Inspects the currently recorded path without running it. |

Examples that add aliases, macros, triggers, or handlers change session behavior. Remove test rules afterward using the matching page's remove/unset command. Panel TOML examples belong in `config.toml`, not the command input. The linked pages below explain prerequisites, cleanup, and longer workflows.

## Core commands

- [`/help`](commands/help.md) — command and topic help.
- [`/clear`](commands/clear.md) — clear output and reset output styling.
- [`/quit`](commands/quit.md) — exit the client safely.
- [`/reload`](commands/reload.md) — reload configuration and scripts.
- [`/save`](commands/save.md) — persist runtime automation without rewriting `config.toml`.
- [`/reconnect`](commands/reconnect.md) — reconnect request placeholder; restart workaround documented.
- [`/msdp`](commands/msdp.md) — inspect stored MSDP values.
- [`/toggle`](commands/toggle.md) — toggle optional panels for the session.
- [`/echo`](commands/echo.md) — add styled local output without sending to the MUD.

## Scripting commands

- [`/macro`](commands/macro.md) — bind keys to immediate commands.

- [`/variable`](commands/variable.md) — inspect and modify runtime variables.
- [`/alias`](commands/alias.md) — define command aliases.
- [`/triggers`](commands/triggers.md) — react to incoming text.
- [`/highlight`](commands/highlight.md) — style matching output.
- [`/event`](commands/event.md) — inspect or emit script events.
- [`/handler`](commands/handler.md) — react to script events.
- [`/lua`](commands/lua.md) — inspect and run Lua hooks.
- [`/timer`](commands/timer.md) — schedule named Lua callbacks.

## Mapping and paths

- [`/map`](commands/map.md) — map editing, landmarks, persistence, and weighted destination routing.
- [`/path`](commands/path.md) — TinTin++-style path recording, editing, and replay.

## Input behavior

- [`/help panels`](commands/panels.md) — panel appearance and refresh configuration (help topic, not a `/panels` command).

- [`Input and keyboard behavior`](commands/input.md) — editing, history, completion, scrolling, repeat syntax, and terminal controls.

Commands without `/` pass through variable expansion, aliases, triggers, movement tracking, command echo, and the network pipeline. `Ctrl-C` clears the current input; `/quit` is the exit command.
