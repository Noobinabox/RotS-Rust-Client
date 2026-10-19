# `/highlight`

## Purpose

Apply local styling to matching MUD output without changing the raw text stored for diagnostics and scripting.

## Syntax

```text
/highlight
/highlight [plain|regex] {pattern} {foreground|none} [background|none] [styles]
/highlight unset {pattern}
/highlight clear
```

Styles include `bold`, `dim`, `italic`, `underline`, `reverse`, and `blink`. `blink` uses the terminal's slow-blink attribute and depends on terminal support.

## Keep a Highlight After Restarting

```text
/highlight {You are hit} {red}
/save
```

Wait for `Saved runtime settings`. Run `/highlight unset {You are hit}` and `/save` to persist its removal. This does not remove configured highlights. See [`/save`](save.md).

## Examples

```text
/highlight
/highlight {You are hit} {red}
/highlight regex {You receive \d+ gold} {yellow} {none} {bold}
/highlight plain {DANGER} {red} {none} {blink bold}
/highlight plain {Group} {green} {none} {bold}
/highlight unset {You are hit}
/highlight clear
```
