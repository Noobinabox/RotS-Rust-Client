# Animation Help

The World pane shows a compact animated marker beside the server's weather description.
Weather never overlays command input or MUD output. `/help weather` and `/help animation`
show this reference; there is no separate `/weather` command.

## RoTS weather

RoTS defines six sky states in `src/structs.h`. Its `src/weather.cpp` supplies
sector-specific descriptions, sent as MSDP `WEATHER` by `src/comm.cpp`.
The client recognizes all nonempty descriptions, including the crack-sector
water-stream and `slowflake` messages. Snowstorms mean blizzards; mist in a
cloudless description still means clear weather.

| RoTS state | Animation frames |
|---|---|
| Clear | `☼` → `☉` |
| Cloudy | `☁` → `~` → `☁` → `-` |
| Rain | `╱` → `│` → `╲` |
| Lightning | `ϟ` → `*` → `ϟ` |
| Snow | `*` → `·` → `+` |
| Blizzard | `*` → `+` → `*` → `x` |

These one-cell markers loop without shifting the text. Indoor, empty, and
unrecognized descriptions have no marker; the
description remains visible. Sunrise and sunset are time announcements, not
additional sky states. Generic fog, wind, ash, and dust descriptions remain
supported, although RoTS has no separate sky constants for them.

## Configuration example

Edit the existing sections in `config.toml`, then run `/reload`:

```toml
[weather]
enabled = true
show_info_marker = true

[animation]
enabled = true
reduced_motion = false
low_performance = false
weather_fps = 2
```

- `weather.enabled = false` or `weather.show_info_marker = false` hides the
  marker and stops its animation, retaining the server's text.
- `animation.enabled = false` or `animation.reduced_motion = true` keeps a
  static first-frame marker.
- `animation.low_performance = true` caps weather motion at two frames per second.
- `animation.weather_fps` selects the elapsed-time animation rate (must be positive).
  Weather changes and rate changes restart the loop.
- Timer-driven redraws use `terminal.tick_rate_ms`; visible motion is also
  limited by the World pane's `panels.info.refresh_ms`. Leave that at `0`
  for unthrottled pane refresh. Slower rendering can skip animation frames.
- Marker color follows the World pane's theme accent.

The default is 2 frames per second (one change every 500 ms).
For example, set `weather_fps = 1` for slower motion, or set
`reduced_motion = true` for static weather symbols, then `/reload`.
