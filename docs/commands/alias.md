# `/alias`

## Purpose

Create and manage runtime aliases. Alias actions enter the same pipeline as typed input, including local commands, movement tracking, command echo, and network sends.

## Syntax

```text
/alias
/alias pattern command text
/alias {pattern} {command} [{command}...]
/alias unset pattern
/alias unset {pattern}
/alias clear
```

Without braces, the first word is the alias pattern and everything after it is one command. For example, `/alias k kill {1}` defines `k orc` as `kill orc`. Internal spaces in the command are preserved. Braces remain available for patterns containing spaces, empty fields, or multiple action fields; mixed syntax such as `/alias rr {recall} {look}` also works.

If a value starts with `{`, it is parsed as a braced field; wrap literal leading braces in an outer pair. Semicolons in a definition are stored without executing anything: `/alias rr recall;look` runs `recall` and `look` only when you invoke `rr`. Separate braced actions also work. `clear` and `unset` remain subcommands; brace those words when using them as alias patterns.

## Keep an alias after restarting

```text
/alias {rr} {look}
/save
```

Wait for `Saved runtime settings`. To remove the saved alias, run `/alias unset rr` and `/save` again. Configured aliases are not deleted. Runtime variables and dependent aliases save together; see [`/save`](save.md).

Capture references such as `{1}` use text captured after the alias pattern. Recursion and expansion counts are bounded by configuration.

## Examples

```text
/alias
/alias k kill {1}
/variable food bread
/alias eatfood eat ${food}
/alias {k} {kill {1}}
/alias {rr} {recall} {look}
/alias {gs} {gtell {1}}
/alias unset {k}
/alias clear
```
