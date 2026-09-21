# `/triggers`

## Purpose

Run commands when incoming MUD output matches text or a regular expression. Actions are queued through the application pipeline and do not block the network reader.

## Syntax

```text
/triggers
/triggers [plain|regex] [--fg <color>] [--bg <color>] {pattern} {command}...
/triggers unset {pattern}
/triggers clear
```

`/trigger` is accepted as a singular alias for `/triggers`. `/highlights` is not a local alias; it is sent to the MUD.

`plain` performs substring matching. `regex` supports captures such as `{1}`. Color filters require the matching ANSI color on the input line.

## Examples

```text
/triggers
/triggers plain {You are hungry} {eat bread}
/triggers regex {^(.+) arrives\.$} {consider {1}}
/triggers plain --fg lightred {Danger} {flee}
/triggers unset {You are hungry}
/triggers clear
```
