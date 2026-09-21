# `/reload`

## Purpose

Reload the active `config.toml` and rebuild configuration-driven aliases, triggers, highlights, event handlers, variables, and Lua settings. Runtime overrides are preserved where supported.

## Syntax

```text
/reload
```

Relative paths resolve beside the active configuration file, normally `~/.config/mud-client/config.toml`. Validation errors are printed locally and the current valid configuration remains active.

## Example

```text
/reload
```
