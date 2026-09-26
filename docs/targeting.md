# RoTS targeting

`scripts/targeting.lua` ports the numbered targeting workflow from the TinTin++ `targeting/` and `target/` modules. It is independent of `init.lua`; both load in the same Lua environment with prefixed `targeting_*` hook names.

## Setup

The bundled `config.toml` already includes the targeting aliases, triggers, and script list. For an existing installation, copy `scripts/targeting.lua` beside your active `scripts/init.lua`, merge the `targeting-*` alias/trigger rules from the bundled configuration, and update the existing Lua table (do not add a duplicate table):

```toml
[lua]
enabled = true
script_dir = "scripts"
scripts = ["init.lua", "targeting.lua"]
```

Keep any other scripts in the list, including `bot.lua` if you use it. Paths resolve beside the active configuration file. An explicit `scripts` list replaces the legacy `entrypoint` selection; it does not load `init.lua` implicitly.

Configure RoTS room titles as **bright yellow** and character/mobile descriptions as **bright cyan**. The corresponding trigger filters are `lightyellow` and `lightcyan`. Other server text with these colors can cause false resets/sightings; this is a color-based heuristic, not a server-provided roster. Existing example arrival hooks remain enabled and may run their own actions independently.

Run `/reload`, then `look`. Each matching mobile line receives an in-place `(1)`, `(2)`, etc. prefix. Leading ANSI styling is applied before the number so it matches the mob text; embedded color changes and trailing resets remain intact. Nothing attacks automatically merely because a mobile is observed.

## Everyday commands

Type each command separately:

```text
look
vt
l1
k2
```

`vt` lists the current numbered targets. `l1` examines the first sighting; `k2` attacks the second. **Only run an attack shortcut when you intend to fight.** These shortcuts enqueue commands through the usual alias/command pipeline.

Two wolf descriptions produce entries `1.wolf` and `2.wolf`. Display numbers select entries, while the server-side number counts sightings with that keyword: an orc between those wolves changes the second wolf's display index, not its `2.wolf` selector. Unknown descriptions still receive a display index but cannot be attacked through it; set a manual keyword instead.

| Shortcut | Queued action |
|---|---|
| `k1` / `kt` | `kill <target>` |
| `l1` / `lt` | `exam <target>` |
| `p1` / `pt` | `p <target>` |
| `f1` / `ft` | `f <target>` |
| `c1` / `ct` | `c <target>` |
| `a1` / `at` | `a <target>` |
| `b1` / `bt` | `b <target>` |
| `q1` / `qt` | `q <target>` |
| `x1` / `xt` | `x <target>` |
| `h1` / `ht` | `h <target>` |
| `i1` / `it` | `i <target>` |
| `y1` / `yt` | `y <target>` |

Replace `1` with a display index from 1 through 100. Except for `kill` and `exam`, these actions retain the TinTin short alias names: define your own spell/skill aliases for `p`, `f`, `c`, `a`, `b`, `q`, `x`, `h`, `i`, and `y`. Without those aliases, the short commands go to the server unchanged. The port does not invent spell mappings.

## Manual override

```text
target orc
lt
target 2.wolf
vt
target
```

`target <keyword>` sets a local manual override; it is not sent to the MUD. The `t` shortcuts use that override, or the first sighting when no override is active. Numbered shortcuts always use their selected sighting, never the override. `target` alone clears it. Scripts use `client.execute("target orc")` for this local override; `client.send("target orc")` still sends directly to the server.

Manual targets accept one ASCII keyword (starting with a letter, then letters, hyphens, or apostrophes), optionally prefixed with `1.` through `100.`. Whitespace, command separators, variable expressions, and unsupported punctuation are rejected without replacing the previous override. Multiword descriptions are not manual command targets.

## Resets and limitations

- Bright-yellow room headers clear sightings and duplicate counts, as do the Word of Sight line `Your mind probes the area seeking other souls.` and Reveal line `A surge of light reveals to you every corner of the room.` Manual overrides survive those resets.
- Reloading Lua or configuration clears both sightings and the manual override. They are ephemeral Lua state, not saved settings.
- At most 100 sightings are retained per reset. Further lines remain unnumbered and produce only one limit warning. Use `look` to refresh.
- Keyword extraction preserves the TinTin special-description overrides and bad-word exclusions, with one final keyword per sighting. It remains heuristic; verify with `vt` and `exam` before attacking.
- There is no authoritative room roster, arrival/death removal, or automatic renumbering after combat. A fresh `look` is needed when inhabitants change. Repeated cyan lines without a reset can create duplicate sightings.
- PK-mode coupling and bot `_mob_check` integration are deliberately excluded. This script does not choose combat strategy, auto-attack, or synchronize another bot's target state.
