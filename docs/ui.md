# User Interface

The UI is rendered with Ratatui and Crossterm in the alternate screen. It adapts to terminal columns and rows, not physical monitor size.

## Responsive Profiles

| Profile | Selection | Behavior |
|---|---|---|
| Mobile | Below mobile width or height | Map above output, command at bottom, compact status only. |
| Tablet | Below tablet width or height | Taller map above output, command at bottom, optional status. |
| 1080p | Standard desktop size | Sidebar plus MUD output and command input. |
| Ultrawide | At least ultrawide width and height | Top dashboard, details/character on the left, and map on the right. |

Breakpoints and pane dimensions are configured in `[layout.*]`; see [Configuration Reference](configuration.md).

## Panels

The 1080p sidebar is split into these logical panels from top to bottom:

1. World
2. Map
3. Opponent
4. Group
5. Character

World remains a compact fixed-height pane. Map, group, and character share the remaining available sidebar space when enabled, while opponent also stays compact. If the sidebar is on the left, the command input starts at the same column as the MUD output.

In ultrawide mode, the top dashboard shows World, Social, and a compressed Nearby Map. Social captures tells, chats, says, narrates, group-says, yells, and sings with local machine `HH:MM` timestamps, extracts the text inside RoTS single quotes, displays `[HH:MM](channel) Prefix Text`, word-wraps long messages, and can be scrolled with the mouse wheel while the pointer is over the panel. Nearby Map centers on the current room, uses the same room and terrain symbols as the full map, omits link lines, butts adjacent road and city route symbols together, overlays up/down exit indicators when needed, and keeps the full linked map on the right side pane.

Optional panels can be toggled for the current session:

```text
/toggle opponent off
/toggle group
/toggle social
```

## Social Pane

The Social pane mirrors the RoTS C++ communication output for incoming and outgoing tells, chats, says, narrates, group-says, yells, and sings. Messages remain in the main MUD output and are also copied into Social as `[HH:MM](channel) Prefix Text`, where `Text` is the payload extracted from the single-quoted RoTS message. Other players are prefixed with their name, tells use `from Name -` or `to Name -`, and your own chat/narrate/sing/yell/say/group-say omit the name prefix. The `/echo` command runs through the same Social capture path, which makes panel testing possible without waiting for live MUD communication.

## Output Pane

The output pane preserves MUD spacing and ANSI styling. It supports styled, plain, and debug display modes. Use `F2` to cycle modes; the mode indicator appears briefly in the title.

Scrolling:

- `PageUp` and `PageDown` scroll by page.
- `Ctrl-Up` and `Ctrl-Down` scroll by one line.
- Mouse wheel scrolls when `[terminal] mouse = true`.
- `Ctrl-E` follows newest output.

Scrolling stops at the oldest full output page, so the output pane does not show a mostly empty viewport above the first retained line.

Command echo is controlled by `terminal.echo_commands`. Echoed commands are appended as local output lines without a prompt prefix.

## Gauges

Character, opponent, and group vitals use gauges. Character health and opponent health are red, mana is blue, and movement is green. Full character, opponent, and TNL gauges expand across the available panel row while keeping fixed-width numeric suffixes so value changes do not shift the layout. Compact gauges keep the configured fixed bar width. The Character sheet separates gauges from stats with a blank row, shows base stats on one row, then leaves another blank row before one class-specific detail row. Mage shows `Mana Regen`, `Spell Power`, and `Spell Pen`; Mystic shows `Willpower`, `Spirits`, `Health Regen`, and `Movement Regen`.

Group display uses a richer layout for small groups and a compact layout for large groups so more members remain visible.

## World Pane

The World pane shows MSDP time and weather. Time text is trimmed at `AM` or `PM` to avoid RoTS suffix fragments, and weather has room to wrap to two lines in the default sidebar layout.

## Mouse Resize

When mouse support is enabled, dragging the main side divider changes pane width for the current session. Config reload restores configured dimensions unless the runtime layout keeps the current resize override.

## Colors

Global UI colors live under `[colors]`. Map doors and terrain have dedicated color options under `[map.doors]` and `[map.terrain.*]`.
