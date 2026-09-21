# `/variable`

## Purpose

Inspect or modify session runtime variables. Runtime values override configured `[variables.values]` entries and survive `/reload` for the current process.

## Syntax

```text
/variable
/variable {name} {value}
/variable unset {name}
```

Use braces when names or values contain spaces or punctuation. Expand values as `${name}` in aliases, triggers, handlers, events, Lua actions, and direct input. Use `$${name}` for a literal reference.

## Examples

```text
/variable
/variable {target} {Orc warlord}
/variable {home_path} {3n2e}
/variable unset {target}
/echo Current target: ${target}
```
