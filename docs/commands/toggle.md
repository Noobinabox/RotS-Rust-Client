# `/toggle`

## Purpose

Enable, disable, or invert an optional panel for the current session. The change is in memory and does not rewrite `config.toml`.

## Syntax

```text
/toggle <group|opponent|social> [on|off]
```

Omitting `on` or `off` flips the current state.

## Examples

```text
/toggle group on
/toggle opponent off
/toggle social
```
