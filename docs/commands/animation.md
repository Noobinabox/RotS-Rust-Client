# Animation Help

The World pane shows a weather scene in its background, behind the server's weather description.
Scenes stay inside the pane's content area and leave text unobscured; they never overlay
command input or MUD output. `/help weather` and `/help animation`
show this reference; there is no separate `/weather` command.

## RoTS weather

RoTS defines six sky states in `src/structs.h`. Its `src/weather.cpp` supplies
sector-specific descriptions, sent as MSDP `WEATHER` by `src/comm.cpp`.
The client recognizes all nonempty descriptions, including the crack-sector
water-stream and `slowflake` messages. Snowstorms mean blizzards; mist in a
cloudless description still means clear weather.

| RoTS state | Background scene |
|---|---|
| Clear | Sunshine by day; a crescent at night. |
| Cloudy | Drifting clouds. |
| Rain | Falling rain. |
| Lightning | A storm scene with lightning. |
| Snow | Falling snow. |
| Blizzard | Wind-driven snow. |

Scenes animate without shifting the text. Indoor, empty, and
unrecognized descriptions have no scene; the
description remains visible. Sunrise and sunset are time announcements, not
additional sky states. Generic fog, wind, ash, and dust descriptions remain
supported with background scenes, although RoTS has no separate sky constants for them.

## Game time and colors

Scene lighting follows the server's MSDP `WORLD_TIME`, not your computer's clock.
The normal RoTS value supplies an hour but not the month or seasonal sunlight
state, so the fallback uses the 6 AM hour for dawn and the 6 PM hour for dusk.
Daytime runs from 7 AM through 5 PM; the remaining hours are night. These are
display-lighting rules, not a claim about the server's season-specific sunrise.

Colors come from the World pane's theme, including any panel-specific overrides:

| Lighting | Scene colors |
|---|---|
| Dawn or dusk | Warm `warning` colors. |
| Day | Rain/storm use `accent`; snow/blizzard use `foreground`; clouds/fog/wind/ash use `muted`; sunshine/dust use `warning`. Day scenes are not dimmed. |
| Night | Dimmed `muted` scenery with `accent` highlights. Clear skies show a crescent, not a simulated lunar phase. |
| Missing or unrecognized time | The original dimmed `muted`/`accent` palette. |

Lighting applies to every supported outdoor weather scene. Indoor and unknown
weather still have no scene. Reduced motion freezes animation, not game time:
lighting and the sun/crescent selection can still change as `WORLD_TIME` updates.

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
  scene and stops its animation, retaining the server's text. `show_info_marker`
  is the legacy setting name; it now controls the background, not a cycling glyph.
- `animation.enabled = false` or `animation.reduced_motion = true` keeps a
  static first-frame scene.
- `animation.low_performance = true` caps weather motion at two frames per second.
- `animation.weather_fps` selects the elapsed-time animation rate (must be positive).
  Weather changes and rate changes restart the loop.
- Timer-driven redraws use `terminal.tick_rate_ms`; visible motion is also
  limited by the World pane's `panels.info.refresh_ms`. Leave that at `0`
  for unthrottled pane refresh. Slower rendering can skip animation frames.
- Scene colors follow game time and the World pane's theme as described above.
  Text remains readable above the background; borders and neighboring panes are unaffected.

Small or text-filled panes may show only fragments or no decoration: text always
takes priority. Increase the World pane's height to leave more space for scenery.

The default is 2 scheduler phases per second (one phase every 500 ms).
Clouds, sunshine, and fog change more slowly than falling rain.
For example, set `weather_fps = 1` for slower motion, or set
`reduced_motion = true` for a static weather background, then `/reload`.
