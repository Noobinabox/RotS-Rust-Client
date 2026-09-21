# `/event`

## Purpose

Inspect recent script events or emit an event manually. Event handlers and Lua hooks receive the event through the bounded dispatch pipeline.

## Syntax

```text
/event
/event {name}
```

Built-in names include `LowHealth`, `RoomChanged`, and `WeatherChanged`; custom names are also supported.

## Examples

```text
/event
/event {LowHealth}
/event {EnemyEntered:orc captain}
```
