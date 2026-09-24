# `/save`

Save all current runtime variables, macros, aliases, triggers, and highlights for the next launch. Saving is explicit, not automatic on quit.

## Syntax

```text
/save
```

## Examples

Define a key and an alias, then save their current definitions:

```text
/macro {F5} {look}
/alias {rr} {look}
/save
```

Wait for `Saved runtime settings` before assuming the write succeeded. The save runs in the background; edits made after `/save` need another save. Graceful exit waits for a pending save. F5 and `rr` will work again after restarting.

Variables and aliases that use them are saved together:

```text
/variable {food} {bread}
/alias {eatfood} {eat ${food}}
/save
```

After restarting or activating this profile, `eatfood` expands to `eat bread`. Variable references are preserved, not flattened; all saved variables are restored together before dependent rules and Lua startup are initialized. Runtime values override configured values. To remove a saved runtime variable, remove any dependent rules first, run `/variable unset {food}`, then `/save`. A configured value of the same name becomes visible again.

To remove these runtime rules permanently:

```text
/macro remove {F5}
/alias unset {rr}
/save
```

Removal affects runtime rules only; configured rules remain available. Clearing a category and saving persists that cleared category without clearing other categories. Saving no runtime rules writes an empty snapshot.

## File and reload behavior

With the aragorn profile active (selected by MSDP or initially by `--character aragorn`), the snapshot is `characters/aragorn.toml.runtime.toml`. It is isolated from other profiles and the unprofiled snapshot. Missing profiles use the shared save file. `/save` saves only the active session, not every cached character. See [character profiles](../character-profiles.md).

The snapshot is stored beside the active config with `.runtime.toml` appended to its filename, normally `~/.config/mud-client/config.toml.runtime.toml`. `config.toml` is never rewritten. Save files contain command text: do not put passwords or other secrets in automation. On Unix new save files are owner-readable/writable only.

Saved rules load as runtime rules at startup or on the first automatic activation of a character profile. Returning to a cached profile restores its live edits. `/reload` reloads configuration and preserves live runtime rules, including unsaved edits and removals; it does not reread the snapshot. Existing precedence is unchanged: runtime macros replace matching configured keys, aliases use priority ordering, highlights prefer matching runtime rules, and configured and runtime triggers can both fire. Removing a runtime rule never deletes its configured counterpart.

Event handlers, Lua scripts/timers, printable-key macro mode, history, and temporary trigger cooldown/one-shot state are not saved. Printable-key mode starts off; trigger state starts fresh. Alias/trigger templates validate against configured plus saved runtime variables. Missing references, cycles, invalid variable names, and expansion-limit violations reject the snapshot without partially applying it. Older snapshots without variables still load. Earlier client versions cannot load snapshots containing the new variables field; keep a backup before downgrading.

Variable values are saved as plain text, including variables created by scripts. Do not store passwords, tokens, or other secrets in runtime variables you intend to save. Variables share the active profile's save file and the combined 4096-entry limit with rules.

## Failures and conflicts

Invalid or unsupported saved files load no runtime rules; configured rules remain usable. Saving is blocked after a startup-load error so the file cannot be accidentally replaced. Back up and repair or move that file, then restart.

Atomic replacement protects against partial writes, but durability across sudden power loss is not guaranteed because the containing directory is not synced.

Writes use a same-directory temporary file and rename. Failed writes leave the previous snapshot intact. Files are limited to 1 MiB and 4096 rules and variables combined. Saves from cooperating clients are serialized with a `.lock` file; external changes since startup or the last successful save cause a conflict instead of silently overwriting data. Back up your unsaved definitions and restart to load the latest snapshot before retrying. Manual edits while a save is actively running are unsupported. If a crashed process left a lock, remove that specific `.lock` file only after confirming no client is saving.
