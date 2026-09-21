# `/handler`

## Purpose

Create runtime event handlers. Handler commands execute through the local command pipeline and may send MUD commands, invoke local commands, set variables, or emit permitted follow-up events.

## Syntax

```text
/handler
/handler [plain|regex] {event-pattern} {command}...
/handler unset {event-pattern}
/handler clear
```

Use `regex` when event names contain captures. Capture references such as `{1}` are substituted into handler commands.

## Examples

```text
/handler
/handler {LowHealth} {flee}
/handler regex {^EnemyEntered:(.+)$} {target {1}} {consider {1}}
/handler unset {LowHealth}
/handler clear
```
