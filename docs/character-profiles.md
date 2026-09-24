# Character Profiles

Character profiles keep automation and UI preferences separate for characters on Return of the Shadow. They are not server profiles and do not log in automatically.

## Create and select a character

Create the `characters` directory beside your shared `config.toml`, normally `~/.config/mud-client/characters/` on Linux. Create `aragorn.toml` inside it with only the settings you want to override:

```toml
[panels.output]
title = "Aragorn"

[variables.values]
target = "orc"

[[macros.rules]]
key = "F5"
command = "score"
```

Normally just launch the client and select your character from the MUD's account menu. When RoTS reports `CHARACTER_NAME` through MSDP, the client selects the matching lowercase file. Switching characters in the same connection switches profiles without closing the client.

You can also preselect settings before the server reports a character:

```sh
mud-client --character aragorn
# From the source checkout:
cargo run -- --character aragorn
# Same character settings against the local development server:
cargo run -- --local --character aragorn
```

Names are 1–64 ASCII letters, digits, hyphens, or underscores, starting with a letter or digit. Names are normalized to lowercase filenames: `--character Aragorn` also selects `aragorn.toml`. Missing files quietly use shared configuration, including with `--character`; profiles are optional. An empty file inherits all shared settings but gets its own runtime save file. A message identifies a profile when one is activated.

Start without `--character` to use shared configuration until MSDP identifies a character with a profile. `--character` is an initial selection, not a lock: later MSDP character changes take precedence. No automatic login or server-profile selection is provided.

RoTS publishes character state only after entering the game, not while sitting at the account menu. Profile switching therefore happens when the next character enters. Duplicate character-name reports do not reload or reset the profile. Server names cannot escape the `characters` directory; unsupported names quietly use shared settings.

Windows device names such as `CON`, `NUL`, `COM1`, and `LPT1` are reserved on every platform; choose another local profile label for those character names.

## Shared defaults and overrides

Load order is built-in defaults, shared `config.toml`, then the selected character TOML. Missing shared configuration uses built-in defaults. Tables merge recursively, and individual scalar values override shared values. Arrays replace the whole shared array rather than appending. For example, the F5 rule above replaces all shared `macros.rules`; omit that array to inherit it.

To disable all configured macros for one character:

```toml
[macros]
rules = []
```

The entire `[connection]` section is shared and is rejected in character files, including username/password fields. Enter login information manually as before. Keep passwords out of automation and version control. The existing `--local` flag still overrides the shared endpoint for development.

Relative Lua, map, and other existing config-relative resource paths still resolve beside the shared `config.toml`, not inside `characters/`. The live map stays shared across character switches; changing its persistence path does not load another map. Exit saving writes the shared live map to the then-active path, so keep map persistence settings shared unless you deliberately want that behavior. Lua script directories may differ by profile.

Terminal setup and tick timing, network/MSDP subscriptions and encoding, logging setup, and initial map loading happen at process startup. Automatic switching does not recreate them. Keep these settings in shared configuration; `--character` can select startup settings when needed. Rule engines, variables, Lua, colors, panel layouts, and runtime save ownership switch live.

## Save and reload

With the aragorn profile active, [`/save`](commands/save.md) writes `characters/aragorn.toml.runtime.toml`. That character's runtime snapshot is loaded on first activation; the unprofiled `config.toml.runtime.toml` is not inherited. Other character snapshots are neither loaded nor changed. Shared automation belongs in `config.toml`. Characters without profiles use the shared runtime session and save file.

Unsaved runtime rules, variables, event handlers, and Lua state are kept in memory for each visited profile and restored when you return. They are not automatically saved to disk. Lua timers are cancelled and path recording is reset on a character change; old timers are not resumed. A pending `/save` finishes for the outgoing profile before its session is parked. Output and the shared map remain visible.

[`/reload`](commands/reload.md) rechecks whether the current character has a profile (including newly created or deleted files), then rereads shared configuration and the active character file while preserving live runtime edits; it does not reread runtime snapshots. Cached profiles otherwise retain their in-memory settings when revisited. `/save` writes runtime automation only, not edits to panels or other configuration. Malformed or unreadable profiles still report actionable errors; automatic selection falls back to shared settings instead of continuing the previous character's automation.

Command history remains session-only. No automatic profile creation is provided. Save each profile's runtime edits before quitting; `/save` saves only the active profile.
