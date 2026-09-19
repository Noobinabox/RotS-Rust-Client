# AGENTS.md

## Project

Build a fully customizable terminal MUD client in Rust.

The client must run entirely in the terminal and support:

- MSDP
- Aliases
- Triggers
- Events
- Highlights
- Configurable panels
- Configurable colors
- Animated maps
- Animated character art
- Animated weather
- Character health, mana, and stamina gauges
- Group member health, mana, and movement gauges
- Opponent name, level, and health gauge
- MUD communication
- MUD output rendering
- Terminal resizing and responsive layouts

The application should feel like a polished, responsive terminal game client rather than a basic telnet wrapper.

---

## Technology Requirements

Use stable Rust and prefer these crates:

- `ratatui` for terminal UI
- `crossterm` for terminal input, colors, raw mode, alternate screen, and resize events
- `tokio` for asynchronous networking and timers
- `serde` and `toml` for configuration
- `unicode-width` for terminal-width calculations
- `thiserror` for typed errors
- `tracing` and `tracing-subscriber` for diagnostics
- `directories` or an equivalent crate for platform-appropriate configuration paths

Use additional crates only when they solve a clear problem and are compatible with the project architecture.

---

## Core Architecture

Use a layered architecture with strict separation between networking, protocol parsing, application state, scripting, layout, and rendering.

Recommended structure:

```text
src/
├── main.rs
├── app.rs
├── config.rs
├── error.rs
├── events.rs
├── commands.rs
├── input.rs
├── terminal.rs
├── network/
│   ├── mod.rs
│   ├── connection.rs
│   ├── telnet.rs
│   ├── msdp.rs
│   └── parser.rs
├── scripting/
│   ├── mod.rs
│   ├── aliases.rs
│   ├── triggers.rs
│   ├── highlights.rs
│   └── events.rs
├── map/
│   ├── mod.rs
│   ├── model.rs
│   ├── renderer.rs
│   └── animation.rs
├── animation/
│   ├── mod.rs
│   ├── scheduler.rs
│   ├── character.rs
│   ├── weather.rs
│   └── effects.rs
├── ui/
│   ├── mod.rs
│   ├── layout.rs
│   ├── theme.rs
│   ├── panels.rs
│   ├── output.rs
│   ├── input.rs
│   ├── map.rs
│   ├── character.rs
│   ├── gauges.rs
│   ├── group.rs
│   ├── opponent.rs
│   └── weather.rs
└── tests/
    ├── msdp.rs
    ├── aliases.rs
    ├── triggers.rs
    ├── map.rs
    └── config.rs
```

Do not place networking logic directly in rendering functions.

Do not place game-state mutation directly inside individual widgets.

The application state must be the source of truth. Widgets should receive read-only state and render it.

---

## Runtime Model

Use an asynchronous event-driven architecture.

Recommended flow:

```text
Terminal input ───────┐
                      ▼
MUD network ───────▶ Application event bus ───────▶ App state
                      ▲                                  │
Timers/animations ────┘                                  ▼
                                                   Ratatui renderer
```

Use typed events:

```rust
enum AppEvent {
    Terminal(TerminalEvent),
    Network(NetworkEvent),
    Timer(TimerEvent),
    Script(ScriptEvent),
    Command(ClientCommand),
}
```

Use typed commands:

```rust
enum ClientCommand {
    SendText(String),
    SendRaw(Vec<u8>),
    Disconnect,
    Reconnect,
    ExecuteAlias(String),
}
```

All event handling should be testable without starting a terminal or network connection.

---

## Terminal and Rendering

The terminal UI must:

- Use raw mode safely.
- Use the alternate screen.
- Restore the terminal on normal exit and errors.
- Handle `Ctrl-C` and quit commands cleanly.
- React to terminal resize events.
- Never assume a fixed terminal width or height.
- Avoid panics caused by small terminal dimensions.
- Redraw at a controlled frame rate.
- Keep input responsive during network activity and animations.

Use `frame.area()` as the authoritative available area during every render.

Never use fixed absolute coordinates for the main UI.

---

## Responsive Layout

The layout must dynamically adapt to the current terminal size.

Define responsive modes similar to:

```rust
enum UiMode {
    Tiny,
    Compact,
    Standard,
    Wide,
}
```

Recommended behavior:

### Tiny

For very small terminals:

- Show command input.
- Show the most recent MUD output.
- Show a compact one-line status bar.
- Hide map, portrait, group, opponent, and weather panels.

### Compact

- Show MUD output.
- Show command input.
- Show compact gauges.
- Show a reduced map if enough space exists.
- Hide large character art.

### Standard

- Show output, input, map, gauges, opponent, and character panel.
- Use compact character art.
- Show group members if vertical space permits.

### Wide

- Show the full detailed character portrait.
- Show animated map.
- Show group panel.
- Show opponent panel.
- Show weather effects.
- Show inventory or additional user-configured panels.

Panels must be dynamically hidden, collapsed, or moved when there is insufficient space.

Never distort large ASCII/ANSI character art. Use alternate portrait assets or hide the portrait instead.

---

## Configurable Panels

Every major UI component must be configurable.

Supported panel properties should include:

- Enabled/disabled
- Position
- Width
- Height
- Minimum width
- Minimum height
- Border style
- Title
- Visibility by responsive mode
- Z-order where relevant
- Refresh rate
- Color theme
- Content alignment

Use a TOML configuration format similar to:

```toml
[connection]
host = "example.org"
port = 4000
username = ""
password = ""
auto_reconnect = true

[terminal]
tick_rate_ms = 100
animation_fps = 15
mouse = false
true_color = true

[layout]
mode = "auto"
sidebar_position = "right"
sidebar_width = 34
show_map = true
show_character = true
show_group = true
show_opponent = true
show_weather = true
show_output = true

[panels.character]
enabled = true
title = "Character"
min_width = 34
min_height = 18
priority = 70

[panels.map]
enabled = true
title = "Map"
priority = 90

[panels.group]
enabled = true
title = "Group"
priority = 60

[panels.opponent]
enabled = true
title = "Opponent"
priority = 80

[panels.output]
enabled = true
title = "MUD Output"
priority = 100

[panels.input]
enabled = true
title = "Command"
priority = 100
```

The configuration must support hot reload if practical. If hot reload is not implemented initially, provide a command or key binding to reload the configuration.

---

## Theme and Colors

All colors must be customizable.

Support:

- Named terminal colors
- ANSI 16-color mode
- 256-color mode
- RGB true color where supported
- Separate colors for normal, warning, danger, success, inactive, selected, and highlighted states
- Per-panel border and title colors
- Per-trigger highlight colors
- Health-based dynamic colors

Example:

```toml
[colors]
background = "#101010"
foreground = "#d0c8b0"
border = "#806f4a"
title = "#d8b365"
accent = "#c49a50"
success = "#65b875"
warning = "#d6ad55"
danger = "#d45c5c"
muted = "#777777"
player = "#e5c07b"
enemy = "#e06c75"
friendly = "#98c379"
weather = "#61afef"
```

Create a central theme system. Widgets must not hardcode colors throughout the codebase.

---

## MUD Networking

Implement asynchronous TCP networking using Tokio.

The network layer must support:

- Connection and disconnection
- Reconnection
- Partial TCP packets
- Telnet negotiation
- Incoming server text
- Outgoing commands
- Prompt handling
- ANSI escape sequences
- MSDP negotiation and data
- Graceful shutdown
- Network errors surfaced as application events

Do not assume that one TCP read equals one complete MUD message.

Create a stream parser that buffers incomplete data and emits complete protocol events.

---

## Telnet and MSDP

Implement proper MSDP support.

The MSDP implementation must support:

- Negotiating MSDP through Telnet options
- Receiving MSDP variables
- Receiving MSDP arrays
- Receiving MSDP tables
- Escaping and unescaping MSDP protocol bytes
- Multiple variables in one message
- Nested arrays and tables
- Unknown variables
- Malformed messages without crashing
- Configurable variable subscriptions

Represent MSDP data using a typed structure:

```rust
enum MsdpValue {
    String(String),
    Array(Vec<MsdpValue>),
    Table(std::collections::HashMap<String, MsdpValue>),
}
```

Maintain raw MSDP data and provide typed accessors where useful.

Support common variables, including but not limited to:

```text
CHARACTER_NAME
HEALTH
HEALTH_MAX
MANA
MANA_MAX
MOVES
MOVES_MAX
LEVEL
EXPERIENCE
OPPONENT
OPPONENT_HEALTH
OPPONENT_HEALTH_MAX
OPPONENT_LEVEL
ROOM
ROOM_NAME
ROOM_VNUM
ROOM_EXITS
WEATHER
GROUP
```

Do not assume every MUD uses the same variable names. Add configurable mappings:

```toml
[msdp.mapping]
health = "HEALTH"
health_max = "HEALTH_MAX"
mana = "MANA"
mana_max = "MANA_MAX"
stamina = "MOVES"
stamina_max = "MOVES_MAX"
opponent_name = "OPPONENT"
opponent_health = "OPPONENT_HEALTH"
opponent_health_max = "OPPONENT_HEALTH_MAX"
opponent_level = "OPPONENT_LEVEL"
```

Create parser unit tests for all supported MSDP value types and malformed input.

---

## MUD Output

The output system must:

- Preserve incoming line order.
- Handle prompts without newlines.
- Handle ANSI color sequences.
- Support wrapped and unwrapped lines.
- Support scrollback.
- Support timestamps optionally.
- Support output categories such as:
  - Normal
  - Combat
  - Communication
  - System
  - Error
  - Prompt
  - Triggered
- Allow highlights and triggers to inspect normalized text.
- Keep raw text available for debugging and scripting.

The output panel must support:

- Scrolling
- Search
- Clear output
- Page up/down
- Following the newest line
- Configurable maximum scrollback
- Copy-friendly plain-text mode if practical

---

## Command Input and Communication

Implement:

- Command editing
- Enter to send
- Command history
- History search
- Cursor movement
- Home/end
- Word movement
- Backspace/delete
- Paste support where available
- Multi-line input if configured
- Local echo
- Communication shortcuts
- Configurable key bindings

Support communication helpers such as:

```text
say hello
tell player hello
group message
auction item
channel gossip hello
```

The exact command transformations must be configurable:

```toml
[communication]
say = "say {message}"
tell = "tell {target} {message}"
group = "gtell {message}"
gossip = "gossip {message}"
```

---

## Aliases

Implement aliases with:

- Exact matches
- Prefix matches
- Arguments
- Positional arguments
- Optional regular expressions if practical
- Multiple commands per alias
- Command substitution
- Alias recursion protection
- Enable/disable state
- Priority/order

Example:

```toml
[[aliases]]
name = "k"
pattern = "^k(?:ill)?\\s+(.+)$"
commands = ["kill {1}"]

[[aliases]]
name = "gs"
pattern = "^gs\\s+(.+)$"
commands = ["gtell {1}"]

[[aliases]]
name = "recall"
pattern = "^rr$"
commands = ["recall", "look"]
```

Aliases must execute through the same command pipeline as manually entered commands.

Prevent infinite alias loops with a maximum expansion depth.

---

## Triggers

Implement triggers that inspect incoming MUD output.

Triggers must support:

- Plain-text matching
- Regular expressions
- Priority
- Enabled/disabled state
- Cooldowns
- One-shot triggers
- Commands to execute
- Events to emit
- Optional capture groups
- Scope by output category

Example:

```toml
[[triggers]]
name = "low_health_warning"
pattern = "You are badly wounded"
priority = 100
commands = ["flee"]
event = "LowHealth"
cooldown_ms = 2000

[[triggers]]
name = "enemy_arrives"
pattern = "^(.+) has entered the room\\.$"
event = "EnemyEntered"
```

Triggers must not block the network reader or UI thread.

All trigger actions should be sent through an event or command queue.

---

## Events

Create a general event system for internal and script-generated events.

Examples:

```rust
enum ScriptEvent {
    LowHealth,
    ManaLow,
    StaminaLow,
    EnemyEntered { name: String },
    EnemyDefeated { name: String },
    RoomChanged,
    WeatherChanged,
    CommunicationReceived,
    Custom(String),
}
```

Events can:

- Change portrait state
- Start animations
- Execute commands
- Change panel visibility
- Add notifications
- Trigger highlights
- Update map state
- Play optional terminal effects

Avoid coupling event names directly to UI widgets.

---

## Highlights

Implement configurable output highlighting.

Highlights must support:

- Plain text
- Regular expressions
- Foreground color
- Background color
- Bold, dim, italic, underline, reverse
- Priority
- Enable/disable state
- Category filtering

Example:

```toml
[[highlights]]
name = "damage_received"
pattern = "You are hit"
foreground = "#ff5555"
bold = true

[[highlights]]
name = "gold_received"
pattern = "You receive \\d+ gold"
foreground = "#e5c07b"
bold = true

[[highlights]]
name = "group_member"
pattern = "^\\[Group\\]"
foreground = "#98c379"
```

Highlight processing should produce styled spans rather than modifying raw text destructively.

---

## Character State and Gauges

Display gauges for the local character:

- Health
- Mana
- Stamina/moves
- Optional experience
- Optional armor
- Optional morale
- Level
- Name
- Status effects

Gauges must:

- Show current and maximum values
- Handle missing maximum values
- Handle zero maximum values safely
- Use dynamic colors based on percentage
- Work in horizontal and compact modes
- Support Unicode and ASCII rendering modes
- Never panic on invalid values

Example display:

```text
Health  ████████████░░░  82 / 100
Mana    ████████░░░░░░░  54 / 100
Moves   █████████░░░░░░  61 / 100
```

Provide compact rendering:

```text
HP 82%  MP 54%  MV 61%
```

---

## Group Panel

Support group members with:

- Name
- Level if available
- Health and maximum health
- Mana and maximum mana
- Moves/stamina and maximum moves
- Status effects
- Online/dead/unavailable state
- Dynamic health-based colors
- Compact and expanded layouts

Example:

```text
GROUP
Aragorn   HP ████████░░ 82%  MP ██████░░░░ 61%  MV 74%
Legolas   HP █████████░ 91%  MP ████████░░ 80%  MV 88%
Gimli     HP █████░░░░░ 52%  MP ███░░░░░░░ 31%  MV 64%
```

The group panel must collapse gracefully when vertical space is limited.

---

## Opponent Panel

Display the current opponent:

- Name
- Level
- Health
- Maximum health if available
- Health percentage
- Status effects if available
- Targeting state
- Combat state

Example:

```text
OPPONENT
Uruk-hai Captain
Level 27
Health ███████░░░ 68%
```

If no opponent is present, show a neutral state:

```text
OPPONENT
No current target
```

Use a distinct opponent color theme.

---

## Map

Implement a map model that supports:

- Rooms
- Room coordinates
- Exits
- Doors
- Room names
- Room IDs/vnums
- Terrain
- Known/unknown rooms
- Current player location
- Other entities where available
- Room discovery
- Panning
- Zoom levels if practical
- Follow-player mode
- Map persistence
- Map animation

The map must be decoupled from the server protocol. MSDP and triggers can update the map model, but the renderer should only consume the map model.

Recommended structures:

```rust
struct Room {
    id: String,
    name: String,
    x: i32,
    y: i32,
    terrain: Terrain,
    exits: Vec<Exit>,
    discovered: bool,
}

struct MapState {
    rooms: std::collections::HashMap<String, Room>,
    current_room: Option<String>,
    camera: Camera,
    effects: Vec<MapEffect>,
}
```

Support animated map elements such as:

- Pulsing player marker
- Moving entities
- Animated exits
- Combat flashes
- Spell effects
- Room-change transitions
- Path-following indicators

Do not redraw the entire map unnecessarily if a smaller update is sufficient.

---

## Character Art

Support large ANSI/ASCII character portraits inspired by classic terminal game art.

Character art requirements:

- Load assets from files.
- Support multiple portrait states.
- Support animation frames.
- Preserve whitespace.
- Avoid wrapping.
- Calculate terminal display width correctly.
- Support ANSI color art.
- Provide plain monochrome fallback.
- Support compact, medium, and full variants.
- Hide or replace art when the panel is too small.

Recommended asset structure:

```text
assets/
└── portraits/
    └── ranger/
        ├── full/
        │   ├── neutral.ascii
        │   ├── hurt.ascii
        │   ├── attack-1.ascii
        │   ├── attack-2.ascii
        │   ├── low-health.ascii
        │   └── dead.ascii
        ├── medium/
        └── compact/
```

Portrait states should include:

```rust
enum PortraitState {
    Neutral,
    Hurt,
    Attacking,
    LowHealth,
    Poisoned,
    Healed,
    Dead,
}
```

Portrait animations should be event-driven. Do not continuously animate expensive large art unless configured.

---

## Weather Animation

Implement a weather system capable of rendering:

- Rain
- Snow
- Fog
- Wind
- Lightning
- Ash
- Dust
- Clear weather
- Custom weather effects

Weather may appear:

- Inside the map panel
- As a transparent overlay where possible
- In a dedicated weather panel
- As a status indicator
- In the character panel background

Weather should not obscure critical MUD output or command input.

Example weather states:

```rust
enum Weather {
    Clear,
    Rain,
    Storm,
    Snow,
    Fog,
    Wind,
    Ash,
}
```

Weather animation must use deterministic or seeded randomness where possible so that tests remain reliable.

---

## Animation System

Create a shared animation scheduler.

Animations must support:

- Start time
- Duration
- Frame rate
- Looping
- One-shot effects
- Pause/resume
- Cancellation
- Event-triggered activation
- Low-performance mode
- Reduced-motion mode

Use elapsed time for animation progress rather than assuming one update equals one frame.

Animations should degrade gracefully when:

- The terminal is small
- The terminal is slow
- The user enables reduced motion
- The frame rate is limited
- The application is processing heavy network activity

---

## Configuration

All of the following must be configurable:

- Connection settings
- MSDP mappings
- Key bindings
- Aliases
- Triggers
- Highlights
- Communication shortcuts
- Panel visibility
- Panel layout
- Panel priorities
- Colors
- Gauge styles
- Animation speed
- Reduced motion
- Weather visibility
- Character art
- Map behavior
- Scrollback size
- Logging
- Reconnection behavior

Provide sensible defaults so the application can start without a configuration file.

Validate configuration at startup and provide useful error messages.

---

## Testing Requirements

Write tests for:

- MSDP negotiation
- MSDP string parsing
- MSDP arrays
- MSDP tables
- Malformed MSDP packets
- Telnet byte escaping
- Partial network reads
- ANSI parsing
- Alias expansion
- Alias recursion protection
- Trigger matching
- Trigger cooldowns
- Highlight matching
- Gauge percentage calculations
- Responsive layout decisions
- Map movement and room updates
- Animation frame selection
- Configuration loading
- Configuration validation

Prefer deterministic tests. Do not require a real terminal or MUD server for unit tests.

Add integration tests using mocked network streams.

---

## Error Handling

Do not use `unwrap()` or `expect()` in runtime paths unless failure is provably impossible.

Errors should be:

- Typed
- Contextual
- Displayed to the user when actionable
- Logged with `tracing`
- Non-fatal where possible

A malformed server message must not crash the client.

A failed connection should produce a visible system message and allow reconnection.

---

## Performance

The client must remain responsive while:

- Receiving rapid MUD output
- Running triggers
- Rendering large character art
- Animating weather
- Updating the map
- Scrolling output
- Processing MSDP data

Requirements:

- Do not block the UI thread.
- Do not block the network reader with rendering.
- Bound all queues or provide backpressure.
- Limit scrollback memory.
- Avoid recompiling regular expressions for every line.
- Cache parsed portrait assets.
- Avoid unnecessary allocations in the render loop where practical.
- Provide a configurable animation frame rate.

---

## Logging and Debugging

Provide optional diagnostic logging.

Useful debug information includes:

- Connection state
- Telnet negotiation
- Raw MSDP packets
- Parsed MSDP values
- Trigger matches
- Alias expansions
- Event dispatch
- Layout mode changes
- Resize events
- Animation state changes

Raw protocol logging must be disabled by default and configurable.

---

## Implementation Order

Implement in this order:

1. Project setup and terminal lifecycle.
2. Basic Ratatui layout with output and input.
3. Async TCP connection.
4. Telnet parsing and ANSI output.
5. MSDP negotiation and parsing.
6. Application state and typed event bus.
7. Character gauges.
8. Responsive panel layout.
9. Command history and communication helpers.
10. Aliases.
11. Triggers.
12. Highlights.
13. Opponent panel.
14. Group panel.
15. Character art loading and animation.
16. Map model and renderer.
17. Map animation.
18. Weather animation.
19. Configuration loading and validation.
20. Hot reload and polish.
21. Automated tests and documentation.

At every step, keep the application compiling and runnable.

---

## Acceptance Criteria

The implementation is complete when:

- The client connects to a MUD asynchronously.
- Telnet negotiation works.
- MSDP data is parsed and displayed.
- Character health, mana, and stamina are visible.
- Opponent name, level, and health are visible.
- Group members and their gauges are visible.
- MUD output and prompts render correctly.
- Communication commands work.
- Aliases, triggers, events, and highlights work.
- The map updates and animates.
- Character art supports multiple animated states.
- Weather can animate without blocking input.
- Panels dynamically adapt to terminal width and height.
- Panels can be enabled, disabled, rearranged, and styled.
- Colors are configurable.
- Terminal resizing does not crash or corrupt the layout.
- Small terminals remain usable.
- Malformed network data does not crash the application.
- Unit and integration tests cover protocol parsing and core behavior.
- The terminal is always restored correctly when the program exits.

A useful first implementation target is a vertical slice containing: asynchronous connection, MSDP health values, MUD output, command input, one responsive character panel, and configurable gauges. Once that works, aliases, triggers, map animation, portrait animation, and the remaining panels can be added without redesigning the foundation.
