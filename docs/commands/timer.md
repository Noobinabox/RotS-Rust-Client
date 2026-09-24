# `/timer`

Schedule named Lua callbacks on the client event loop.

## Syntax

```text
/timer
/timer list
/timer set <name> <interval_ms> <lua_function> [once|repeat]
/timer cancel <name>
/timer clear
```

## Examples

First enable/configure Lua as described in [the Lua guide](lua.md) and add this callback to your entrypoint:

```lua
function timer_notice(ctx)
  client.echo("Timer fired")
end
```

Reload the script and schedule a single local message after approximately one second:

```text
/lua reload
/timer set reminder 1000 timer_notice once
/timer list
```

For a repeating reminder, enter:

```text
/timer set reminder 5000 timer_notice repeat
```

This prints a local message approximately every five seconds. Stop just that timer with:

```text
/timer cancel reminder
```

To remove **all** timers, including timers used by other scripts:

```text
/timer clear
```

Timers do not run arbitrary command strings. They call a named Lua function, which can queue safe actions through `client.send`, `client.map.run`, and related APIs. See [Lua Botting](../botting.md) for the bot controller’s timer usage.
