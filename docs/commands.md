# Command Reference

Every local command starts with `/` and is handled by the client instead of being sent to the MUD. This page is the command index; each command has a dedicated reference page with syntax, behavior, failure cases, and examples.

## Core commands

- [`/help`](commands/help.md) — command and topic help.
- [`/clear`](commands/clear.md) — clear output and reset output styling.
- [`/quit`](commands/quit.md) — exit the client safely.
- [`/reload`](commands/reload.md) — reload configuration and scripts.
- [`/reconnect`](commands/reconnect.md) — request a network reconnect.
- [`/msdp`](commands/msdp.md) — inspect stored MSDP values.
- [`/toggle`](commands/toggle.md) — toggle optional panels for the session.
- [`/echo`](commands/echo.md) — add styled local output without sending to the MUD.

## Scripting commands

- [`/variable`](commands/variable.md) — inspect and modify runtime variables.
- [`/alias`](commands/alias.md) — define command aliases.
- [`/triggers`](commands/triggers.md) — react to incoming text.
- [`/highlight`](commands/highlight.md) — style matching output.
- [`/event`](commands/event.md) — inspect or emit script events.
- [`/handler`](commands/handler.md) — react to script events.
- [`/lua`](commands/lua.md) — inspect and run Lua hooks.

## Mapping and paths

- [`/map`](commands/map.md) — map editing, landmarks, persistence, and weighted destination routing.
- [`/path`](commands/path.md) — TinTin++-style path recording, editing, and replay.

## Input behavior

- [`Input and keyboard behavior`](commands/input.md) — editing, history, completion, scrolling, repeat syntax, and terminal controls.

Commands without `/` pass through variable expansion, aliases, triggers, movement tracking, command echo, and the network pipeline. `Ctrl-C` clears the current input; `/quit` is the exit command.
