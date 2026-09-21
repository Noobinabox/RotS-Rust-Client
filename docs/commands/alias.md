# `/alias`

## Purpose

Create and manage runtime aliases. Alias actions enter the same pipeline as typed input, including local commands, movement tracking, command echo, and network sends.

## Syntax

```text
/alias
/alias {pattern} {command} [{command}...]
/alias unset {pattern}
/alias clear
```

Capture references such as `{1}` use text captured after the alias pattern. Recursion and expansion counts are bounded by configuration.

## Examples

```text
/alias
/alias {k} {kill {1}}
/alias {rr} {recall} {look}
/alias {gs} {gtell {1}}
/alias unset {k}
/alias clear
```
