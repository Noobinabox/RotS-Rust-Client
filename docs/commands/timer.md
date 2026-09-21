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

```text
/timer set heartbeat 1000 bot_status once
/timer set bot-controller 750 bot_tick repeat
/timer cancel heartbeat
/timer clear
```

Timers do not run arbitrary command strings. They call a named Lua function, which can queue safe actions through `client.send`, `client.map.run`, and related APIs. See [Lua Botting](../botting.md) for the bot controller’s timer usage.
