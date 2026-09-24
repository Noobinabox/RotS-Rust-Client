# `/variable`

## Purpose

Inspect or modify runtime variables. Runtime values override configured `[variables.values]` entries and survive `/reload`. Use [`/save`](save.md) to persist them with the active profile's aliases and other runtime rules. Values are saved in plain text; do not store secrets in saved variables.

## Syntax

```text
/variable
/variable name value text
/variable {name} {value}
/variable unset name
/variable unset {name}
```

Without braces, the first word is the name and the rest is its value: `/variable food meat pie` stores `meat pie`. Internal spaces are preserved; surrounding spaces are trimmed. Use braces to preserve surrounding spaces (`/variable label {  hello  }`) or set an empty value (`/variable label {}`). Semicolons are stored without execution, so `/variable route north;east` stores `north;east`. Mixed syntax such as `/variable food {meat pie}` also works. A value starting with `{` is treated as braced; wrap literal leading braces in an outer pair. Variable names still follow the existing identifier rules; braces do not make spaces in names valid. Brace the name `unset` when creating a variable with that name.

Expand values as `${name}` in aliases, triggers, handlers, events, Lua actions, and direct input. Use `$${name}` for a literal reference. Braces and variable references inside an unbraced value remain part of that value; shorthand does not expand templates early.

Variables expand before regular-expression compilation and `{1}` capture substitution. `[variables.values]` supplies configured defaults; `[variables] max_expansion_depth` and `max_expanded_bytes` bound expansion. Removing a runtime override reveals any configured value of the same name.

## Examples

```text
/variable
/variable food meat pie
/variable {target} {Orc warlord}
/variable {home_path} {3n2e}
/echo Current target: ${target}
/save
```

Wait for `Saved runtime settings` before continuing:

```text
/variable unset {target}
/save
```

This lists variables, stores a target and route, prints `Current target: Orc warlord`, and saves them. It then removes the target and saves that removal; `home_path` remains saved. Expand a variable before unsetting it; referencing a missing variable produces an error.
