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
| Cloudy | Overlapping cloud layers drifting in front of the sun or moon, with sunrise/sunset gradients. |
| Rain | Falling rain over clouds; no sun/moon after the transition. |
| Lightning | Rain and clouds with immediate bright lightning. |
| Snow | Falling snow over clouds; no sun/moon after the transition. |
| Blizzard | Wind-driven snow over clouds; no sun/moon after the transition. |

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

Colors come from the World pane's theme, including panel-specific overrides,
except snow/blizzard flakes, which stay neutral white or gray:

| Lighting | Scene colors |
|---|---|
| Dawn or dusk | Cloud gradients use `muted`, `danger`, and `warning`; other non-clear weather uses warm `warning` except rain/storm and snow/blizzard. |
| Day | Rain/storm use `rain`; snow/blizzard use contrasting white/gray shades; clouds/fog/wind/ash use `muted`; dust uses `warning`. Day scenes are not dimmed. |
| Night | Rain/storm keep dimmed `rain`; snow/blizzard keep neutral shades, dimmed only when no contrast correction is needed; other non-clear weather uses dimmed `muted` scenery with `accent` highlights. |
| Missing or unrecognized time | Dimmed `muted`/`accent`, except rain/storm keep `rain` and snow/blizzard keep contrasting neutral shades. |

Snow dots use soft white and star-shaped flakes use bright white on dark
backgrounds, dimmed outside daytime. Pale panel backgrounds use contrasting
neutral gray without dimming, including Rose Pine Dawn and panel overrides.
Theme accents and sunrise/sunset do not tint flakes. Snow transitions
use spatial reveal instead of blending with colored backgrounds or clouds.
These whites use terminal gray/white palette entries, so terminal palette
customization can affect their appearance. Clouds retain their own lighting.

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

Sun/moon changes fade the outgoing body out over 0.5 seconds, then fade
the incoming body in over 0.5 seconds. First appearance also fades in.
Repeated time reports and ordinary same-body position updates do not restart
the fade. These short transitions use elapsed animation time, not a guessed
game clock. Celestial appearance/body-change fades use at least 10 animation
steps per second so the short fade remains visible with the default particle FPS;
low-performance mode caps those steps at 2 FPS. Actual display steps depend on draws.
Reduced motion or disabled animation shows the current body immediately.
Outdoor weather changes preserve the celestial clock and transition, even while
other outdoor weather hides the body after its 0.5-second weather-layer fade-out.
Indoor areas, unknown weather, and invalid time cancel it immediately.

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
and movement remain independent of the body's fade. Rain, storms, snow, and
blizzards share the cloud layer, but only clear/cloudy weather keeps the sun/moon visible.

## Shared weather transitions

Weather changes blend clouds and particles over 1.5 seconds; the sun/moon layer
fades in or out more quickly, over 0.5 seconds.
Cloudy → rain keeps the clouds while rain appears; rain → snow fades between
particle types; cloudy → clear thins the clouds to reveal the same sun/moon.
Changing from clear/cloudy to rain, storms, snow, blizzard, fog, wind, ash, or dust
fades the sun/moon out over 0.5 seconds. It stays hidden until clear/cloudy
weather returns, then fades back in at the current game-time position.
Repeated reports do not restart the transition. Rapid changes continue from the
current blend without accumulating old scenes, and particle/cloud motion stays continuous.

The weather label always changes immediately. Lightning starts immediately on
storm entry and stops immediately on storm exit; it never fades with precipitation.
Indoor/unknown weather or hidden/disabled weather clears effects immediately.
Reduced motion or disabled animation switches layers instantly. Initial weather
is shown immediately; existing sun/moon appearance fades remain separate.
RGB colors blend toward the background while glyphs use stable spatial reveal;
terminal palettes use stepped colors and the same reveal. Preview by switching
weather with Left/Right in the offline demo; Space freezes transitions too.

At sunrise, cloud colors grade toward warm light on the right (east); at sunset,
the gradient reverses toward the left (west). Lower edges catch warmer light,
while rear layers are shaded. Colors reuse `muted`, `danger`, and `warning`:
RGB themes blend smoothly; named/256-color themes use stepped palette colors.
Day, night, and unknown-time cloud palettes keep their existing behavior.

Cloud layers use seeded spacing and different slow drift speeds, so they overlap
at irregular intervals. Foreground clouds hide background outlines and interiors.
This deterministic scene repeats with the existing weather phase cycle; overlap
timing follows `weather_fps`, not a separate real-time timer. Reduced motion or
disabled animation freezes the layers. Preview with `cargo run -- --demo weather`,
Right once for cloudy, then Up/Down to 6 AM or 6 PM.

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
