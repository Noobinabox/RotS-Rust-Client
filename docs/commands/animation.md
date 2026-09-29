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
| Clear | Sunshine by day; a round, cratered moon at night. |
| Cloudy | Drifting clouds in front of the sun or moon. |
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

To preview every weather scene without connecting, run `cargo run -- --demo weather`
from the source checkout, or `mud-client --demo weather` after rebuilding/installing.
Left/Right selects weather, Up/Down changes the hour, Space pauses/resumes, and
q/Esc/Ctrl-C exits. The demo uses built-in defaults and does not load or save your
configuration. Its clock advances one game hour every six seconds; choose clear
weather near 5 AM or 6 PM to watch the moon/sun fade transitions.

Scene lighting follows the server's MSDP `WORLD_TIME`, not your computer's clock.
The normal RoTS value supplies an hour but not the month or seasonal sunlight
state, so the fallback uses the 6 AM hour for dawn and the 6 PM hour for dusk.
Daytime runs from 7 AM through 5 PM; the remaining hours are night. These are
display-lighting rules, not a claim about the server's season-specific sunrise.

Colors come from the World pane's theme, including any panel-specific overrides:

| Lighting | Scene colors |
|---|---|
| Dawn or dusk | Warm `warning` colors for non-clear weather except rain/storm. |
| Day | Rain/storm use `rain`; snow/blizzard use `foreground`; clouds/fog/wind/ash use `muted`; dust uses `warning`. Day scenes are not dimmed. |
| Night | Rain/storm keep their dimmed `rain` color; other non-clear weather uses dimmed `muted` scenery with `accent` highlights. |
| Missing or unrecognized time | Dimmed `muted`/`accent`, except rain/storm keep `rain`. |

Rain uses a dedicated blue `rain` theme color at every time of day, dimmed outside
daytime rather than turning gold or gray. Lightning uses non-dimmed bright-yellow `lightning`
so it remains distinct at night. Customize globally or just for the World pane:

```toml
[colors]
rain = "#61afef"
lightning = "#ffff00"

[panels.info.theme]
rain = "blue"
lightning = "lightyellow"
```

Storms show one to three small simultaneous bolts lasting half a second, with
randomized 1–5-second intervals between strike starts. Timing uses elapsed time,
independent of `weather_fps` and low-performance mode; late draws can miss a strike.
The first strike is immediate. A seeded generator makes visual variation repeatable
for tests. There are no full-pane flashes; reduced motion/disabled animation keeps
two static bolts (space and text permitting). Bolts indicate storm weather, not authoritative
spell-casting windows—use the server's weather description for gameplay decisions.

Lighting applies to every supported outdoor weather scene. Indoor and unknown
weather still have no scene. Reduced motion freezes animation, not game time:
lighting and celestial positions can still change as `WORLD_TIME` updates.

### Sun and moon paths

The World pane uses north-up orientation: east is the right edge and west is the
left. In clear or cloudy weather, the sun and moon travel from right to left.
The sun rises at 6 AM, reaches the top-center at noon, and sets at 6 PM.
After 6 PM the cratered moon follows the same arc overnight, highest at midnight,
approaching the western horizon before 6 AM. This is a decorative night cycle,
not a simulation of lunar phases or actual moonrise times.

For example, `WORLD_TIME` of `It is about 9:00 AM on ` places the sun partway
up the eastern sky; `3:00 PM` places it on the descending western side.
Positions update with server time (normally hourly), not every animation tick;
there is no guessed wall-clock interpolation between server updates.

Sun/moon changes now fade the outgoing body out over two seconds, then fade
the incoming body in over two seconds. First appearance also fades in.
Repeated time reports and ordinary same-body position updates do not restart
the fade. These short transitions use elapsed animation time, not a guessed
game clock, and respect `weather_fps` (capped at 2 FPS in low-performance mode).
Reduced motion or disabled animation shows the current body immediately.
Switching between clear and cloudy weather preserves the celestial transition.
Other weather, indoor areas, and invalid time cancel it.

The sun transitions from theme `danger` at the horizon through `warning` to
`foreground` overhead, then reverses toward sunset. The moon transitions from
`muted` at the horizon to `foreground` overhead. Both dim near the horizon.
RGB theme colors blend gradually; named and indexed colors change in palette
steps without forcing true color. World-pane theme overrides apply.
During a fade, RGB foreground/background colors blend toward the pane background;
other palettes use dimming and gradual glyph reveal instead of true transparency.
The moon is a 7-by-5 ASCII disk with craters, or a 5-by-3 disk in smaller panes.
Very small panes use a single `O` (sun) or `o` (moon); text always takes priority and may
partially or completely hide a body. Missing/unrecognized time retains the old
fixed-position, dimmed sunshine fallback in clear weather; cloudy weather shows
only clouds without a recognized time. Moving clouds cover the sun/moon where
their outlines or interiors overlap, revealing it through gaps. Cloud colors
and movement remain independent of the body's fade. Rain, storms, and other
weather retain their existing scenes without a sun or moon.

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
