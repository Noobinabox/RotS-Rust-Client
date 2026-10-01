# `/theme`

Inspect and switch the client palette while it is running.

`/theme list` (or `/theme`) displays a Markdown table with each built-in/custom theme and its light or dark appearance.

```text
/theme list
/theme current
/theme use nord
/theme use catppuccin-mocha
/theme use solarized-light
/theme auto on
/theme auto off
```

Runtime selections last for the current session. Configure a persistent selection with `[theme].active`, then use `/reload`. Race themes can be selected manually or enabled automatically with `[theme].auto_race = true`. `/theme use` disables race auto-selection so the manual choice remains active. `/theme auto on` immediately applies the current recognized race, or waits for a race value if none is available.

Catppuccin themes use the [official palette](https://catppuccin.com/palette/):

- `catppuccin-latte` — light background with dark text plus adaptive ANSI and map contrast.
- `catppuccin-frappe` — subdued dark palette (Frappé).
- `catppuccin-macchiato` — medium-dark palette.
- `catppuccin-mocha` — darkest palette.

All four map Base/Text to the background/text, Blue/Mauve to titles/accents, Green/Yellow/Red to status colors, and Teal to player accents. Server ANSI foregrounds and configured map colors that lack contrast are adjusted against the actual background being rendered. Explicit ANSI foreground/background pairs are evaluated together rather than against the global theme background.

Additional light terminal themes use their official upstream palettes:

- `solarized-light` — Solarized Base3 with its blue, violet, cyan, green, yellow, and red accents.
- `gruvbox-light` — Gruvbox's medium light background with faded light-mode accents.
- `rose-pine-dawn` — Rosé Pine Dawn's warm base with pine, foam, iris, gold, and love accents.

All light themes use the same adaptive contrast behavior as Catppuccin Latte. Palette definitions retain their upstream colors; runtime semantic foregrounds are darkened only when necessary to keep ordinary UI text readable. Server ANSI foregrounds and configured map colors are also adapted against their final rendered background.

Race palettes use high-identity cinematic colors, dark scene-setting backgrounds, and readable parchment or silver text. They draw directly from the locations, costumes, materials, and lighting language of *The Lord of the Rings* and *The Hobbit* films while incorporating the game's custom race lore:

| Theme | Visual identity |
|---|---|
| `wood-elf` | Thranduil's Mirkwood: blue-black forest, autumn leaf gold, elegant silver-green, pale carved stone, and cold mist. |
| `hobbit` | Hobbiton in *The Fellowship of the Ring*: saturated hill and round-door green, warm plaster, polished wood, party lanterns, and golden hearthlight. |
| `human` | Gondor: Minas Tirith stone and White Tree silver against night-blue shadow, steel, royal blue, and restrained gold. |
| `dwarf` | Erebor in *The Hobbit*: near-black carved stone, immense molten gold, bronze machinery, emerald treasure, and warm ivory firelight. |
| `beorning` | Beorn's homestead in *The Hobbit*: dark timber hall, black-brown bear fur, dominant honey amber, firelight, rough leather, meadow green, and warm cream. |
| `uruk-hai` | The game's Dol Guldur-based Uruk-Hai, grounded in *The Hobbit* films: wet black masonry, dead woodland, spectral necromancer green, corroded iron, smoke, and furnace rust. |
| `common-orc` | Mordor orcs in *The Lord of the Rings*: ash, weathered bone, diseased olive, scavenged iron, torch smoke, and dominant jagged rust-red armor. |
| `olog-hai` | The game's troll/Uruk-Hai hybrids, drawing on Mordor's armored film trolls: Morannon basalt, black iron, furnace bronze, scar tissue, dried blood, and sick green. |
| `uruk-lhuth` | Custom Dol Guldur dark casters: black-violet sorcery, necromancer smoke, venom green, and bloody magenta. |
| `haradrim` | Haradrim in *The Lord of the Rings*: sun-baked crimson, Mûmakil leather, black-red shadow, brass, desert gold, and turquoise patina. |

Accent, success, warning, player, danger, and enemy roles remain visually separated within every race palette. These cinematic palettes use RGB colors; terminals that quantize RGB may reduce those distinctions because curated ANSI-16 race variants are not currently provided. Explicit panel overrides can still change the final appearance.
