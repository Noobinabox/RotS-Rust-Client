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

Styles include `bold`, `dim`, `italic`, `underline`, and `reverse`.

## Examples

```text
/highlight
/highlight {You are hit} {red}
/highlight regex {You receive \d+ gold} {yellow} {none} {bold}
/highlight plain {Group} {green} {none} {bold}
/highlight unset {You are hit}
/highlight clear
```
