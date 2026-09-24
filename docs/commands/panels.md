# Panel Customization

Edit `panels.info`, `social`, `map`, `opponent`, `group`, `character`, `output`, or `input` in your configuration, then run `/reload`.

- `border_style`: `plain`, `rounded`, `double`, `thick`, or `none`.
- `alignment`: `left`, `center`, or `right` for text. Map geometry and full-width gauges keep their layout.
- `refresh_ms`: `0` renders each frame; values up to `60000` throttle panel content refreshes. Input must remain `0`.
- `[panels.<name>.theme]`: override individual global color keys such as `background`, `foreground`, `border`, and `title`.

Omitted settings retain their defaults. Borderless panels keep titles and the same content inset. Responsive layout still controls panel placement and visibility. Nearby Map uses the map panel's styling.

Refresh intervals never delay state updates or commands. User interaction and reload invalidate cached displays; input remains immediate. Use short intervals for combat and MUD output. Explicit ANSI/highlight, map, and gauge colors retain their existing precedence.

## Examples

These are **configuration-file examples**, not commands to type into the MUD. Edit the active `config.toml` (normally `~/.config/mud-client/config.toml`). Replace the matching existing panel table or merge these fields into it; do not add a second table with the same name. Other panel settings remain unchanged.

### Rounded output panel with custom colors

Keep live output immediate, use rounded borders, and override only this panel's background, text, border, and title colors:

```toml
[panels.output]
enabled = true
title = "Adventure"
min_width = 20
min_height = 5
priority = 100
visible_modes = []
border_style = "rounded"
alignment = "left"
refresh_ms = 0

[panels.output.theme]
background = "#101820"
foreground = "#d0e8ff"
border = "cyan"
title = "lightcyan"
```

MUD-provided ANSI colors and highlights still take precedence over the generic foreground color.

### Centered Social panel with slower refreshes

Use double borders and refresh the Social display at most once every 250 ms during normal background updates. The panel appears in an ultrawide dashboard when Social is enabled; this does not change your layout.

```toml
[panels.social]
enabled = true
title = "Conversations"
min_width = 30
min_height = 5
priority = 95
visible_modes = ["ultrawide"]
border_style = "double"
alignment = "center"
refresh_ms = 250
```

To enable Social for the current session, enter:

```text
/toggle social on
```

User interaction can refresh a cached panel before its normal interval expires.

### Borderless, right-aligned command input

Hide the input border while keeping its title, inset, and insertion cursor. Input must always use `refresh_ms = 0`:

```toml
[panels.input]
enabled = true
title = "Command"
min_width = 20
min_height = 3
priority = 100
visible_modes = []
border_style = "none"
alignment = "right"
refresh_ms = 0
```

For thick borders instead, change `border_style` to `"thick"`. For the standard appearance, use `"plain"` and `alignment = "left"`. Remove the panel's `theme` overrides to inherit global colors again.

### Apply and check your changes

Save the file, then enter:

```text
/reload
```

The changed panels update without restarting. If validation fails, correct the setting named in the error and run `/reload` again; the previous valid configuration stays active.

Open this guide in the client with:

```text
/help panels
```

See [panel configuration](../configuration.md#panels) for the full option reference.
