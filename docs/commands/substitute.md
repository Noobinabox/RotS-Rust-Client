# Substitution Help

Substitutions change displayed incoming MUD text, not the original text seen by
triggers or the commands sent to the server. They also apply to server prompts.

- `/substitute` lists configured and runtime rules.
- `/substitute plain {an orc} {an enemy}` replaces every matching occurrence.
- `/substitute regex {^(.+) arrives\.$} {Arrival: {1}}` uses regex captures.
- `/substitute plain {orc} {${enemy_label}}` expands a client variable at display time.
- `/substitute plain {unwanted text} {}` removes matched text, not the whole output row.
- `/substitute unset {an orc}` removes that runtime rule.
- `/substitute clear` removes all runtime rules, retaining configured rules.
- `/save` persists runtime substitutions; `/reload` reloads configuration.

Patterns are literal strings or Rust regular expressions. Captures use `{0}` for
the full match and `{1}` onward for groups. This is not TinTin pattern syntax.
Rules run once each in priority order; later rules see earlier replacements.
Replacements never retrigger automation. ANSI colors outside matches are retained.
Replacements are bounded to 64 KiB; failures leave the original line visible.

## Lua display edits

In an incoming-line trigger, `client.output.replace(text)` replaces that line;
`client.output.gag()` hides it without a blank row. Gag wins over replacement.
Explicit `client.echo` calls still appear and never retrigger automation.
These operations are invalid from aliases, timers, or event hooks. A failed hook
discards its pending edits. Configured substitutions run after Lua replacements.
Replaced rows retain their original raw text. Gagged rows are removed from
scrollback; their original text remains available during trigger processing,
not as a retained output row.

## TOML rules

```toml
[substitutions]
enabled = true

[[substitutions.rules]]
name = "arrival"
enabled = true
priority = 10
match_type = "regex"
pattern = '^(.+) arrives\.$'
replacement = 'Arrival: {1}'
categories = ["normal"]
```

Optional `foreground` and `background` filter the original server colors, using
the same named, indexed, or RGB color values as triggers. Rules with greater
priority run first; names break ties. Runtime rules use priority 10000.
Pattern variables are not expanded; replacement variables are expanded when
the rule matches. Remove configured rules from TOML and `/reload`.

For inline numbering, use `client.output.replace("(2) " .. ctx.raw_line)`.
The TinTin-style alternative is to echo a numbered line and call
`client.output.gag()` to suppress the original. Neither affects subsequent lines.
