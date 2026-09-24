# `/reload`

## Purpose

Reload the active `config.toml` and rebuild configuration-driven aliases, triggers, highlights, event handlers, variables, and Lua settings. Runtime overrides are preserved where supported.

Saved runtime rules load at startup or on first activation of a character profile. `/reload` preserves the current in-memory rules, including unsaved edits and removals, rather than rereading the saved snapshot. Use [`/save`](save.md) to persist those changes.

With a character profile active, reload reads both shared `config.toml` and that character file. MSDP selects profiles as characters enter the game. For example, edit `characters/aragorn.toml`, then enter `/reload` while playing Aragorn. See [character profiles](../character-profiles.md).

## Syntax

```text
/reload
```

Relative paths resolve beside the active configuration file, normally `~/.config/mud-client/config.toml`. Validation errors are printed locally and the current valid configuration remains active.

## Examples

After changing panel colors or adding a configured macro, save the active `config.toml` and enter:

```text
/reload
```

Look for the `Config Reloaded` message and inspect the changed panel or binding. If an error is reported, correct the file and retry. Reload reads the file; it does not save session changes back to disk. For a walkthrough, see [panel examples](panels.md#examples).
