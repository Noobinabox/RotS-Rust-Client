# Panel Customization

Edit `panels.info`, `social`, `map`, `opponent`, `group`, `character`, `output`, or `input` in your configuration, then run `/reload`.

- `border_style`: `plain`, `rounded`, `double`, `thick`, or `none`.
- `alignment`: `left`, `center`, or `right` for text. Map geometry and full-width gauges keep their layout.
- `refresh_ms`: `0` renders each frame; values up to `60000` throttle panel content refreshes. Input must remain `0`.
- `[panels.<name>.theme]`: override individual global color keys such as `background`, `foreground`, `border`, and `title`.

Omitted settings retain their defaults. Borderless panels keep titles and the same content inset. Responsive layout still controls panel placement and visibility. Nearby Map uses the map panel's styling.

Refresh intervals never delay state updates or commands. User interaction and reload invalidate cached displays; input remains immediate. Use short intervals for combat and MUD output. Explicit ANSI/highlight, map, and gauge colors retain their existing precedence.

See `docs/configuration.md` for the complete panel options and a TOML example.
