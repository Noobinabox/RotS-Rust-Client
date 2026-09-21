# `/echo`

## Purpose

Write local text into the output pane without sending it to the MUD. Echoed communication is also processed by the Social panel classifier.

## Syntax

```text
/echo [--fg <color>] [--bg <color>] <text>
/echo --fg=<color> --bg=<color> <text>
```

Colors may be named terminal colors, indexed colors such as `index:17`, or RGB values such as `#d45c5c`. Use `--` before text beginning with an option-like token.

## Examples

```text
/echo Server test complete
/echo --fg yellow Warning: low health
/echo --fg #65b875 --bg #101010 Ready
/echo -- --not-an-option
```
