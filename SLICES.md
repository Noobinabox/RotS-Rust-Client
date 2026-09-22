# SLICES.md

High-level implementation slices for the Rust MUD client. Keep this file focused on scope and ordering. When a slice starts, mark it as `Active` here and replace `WIP.md` with that slice's detailed task list. When the slice finishes, mark it `Done` and clear completed work out of `WIP.md`.

## Status Legend

- `Done`: implemented, reviewed, and validated.
- `Active`: currently being worked in `WIP.md`.
- `Next`: ready to start.
- `Planned`: later work.

## Slices

### 1. RoTS Vertical Slice

Status: `Done`

Async RoTS connection, terminal lifecycle, Ratatui layout, MUD output rendering, command input, Telnet/MSDP negotiation, character gauges, opponent/group MSDP parsing, local `localhost:3791` test launch, command history basics, and output rendering stability fixes.

### 2. Output Controls and Text UX

Status: `Done`

Make the MUD output panel usable as a real client scrollback surface.

Scope:

- Scrollback navigation.
- Follow-newest mode.
- Clear output command/key.
- Page up/down and line scroll.
- Plain-text copy/debug view if practical.
- Search over normalized output.
- Output category groundwork for later triggers/highlights.

### 3. Alias and Command Pipeline

Status: `Done`

Add configurable aliases and route all submitted commands through a single expansion pipeline.

Scope:

- TOML alias definitions.
- Exact and regex alias matching.
- Positional substitutions.
- Multi-command expansion.
- Recursion depth protection.
- Tests for expansion, disabled aliases, and loop prevention.

### 4. Triggers and Highlights

Status: `Done`

Add output inspection and reaction without coupling scripts directly to rendering.

Scope:

- Trigger definitions.
- Plain-text and regex matching.
- Cooldowns, one-shot triggers, and priority.
- Internal script-event record groundwork.
- Highlight rules producing styled spans.
- Tests for matching, cooldowns, captures, and category filters.

### 5. Panel Configuration and Hot Reload

Status: `Done`

Expand configuration from startup defaults into user-customizable UI behavior.

Scope:

- Per-panel enabled state, title, size, priority, and responsive visibility.
- Configurable gauge styles and colors.
- Runtime config reload command/key.
- Validation errors surfaced in the UI.
- README examples for common layouts.

### 6. Opponent and Group Panels

Status: `Done`

Turn parsed combat and group state into dedicated responsive panels.

Scope:

- Opponent name, level, health, and neutral state.
- Group member health, mana, and movement gauges.
- Compact and expanded panel modes.
- Graceful collapse on small terminals.
- Tests for missing and partial MSDP values.

### 7. Map Model and Renderer

Status: `Done`

Build a protocol-independent room graph and terminal renderer.

Scope:

- Room, exit, terrain, camera, and discovery models.
- Current-room updates from MSDP/triggers.
- Basic map renderer.
- MUD-output-pane-sized map snapshots via `/map map`.
- Pan/follow-player behavior.
- Persistence format.
- Tests for movement and room updates.

### 8. Animation Framework

Status: `Done`

Add deterministic animation scheduling for map effects and weather.

Scope:

- Shared animation scheduler.
- Frame timing by elapsed time.
- Reduced-motion and low-performance options.
- Event-triggered one-shot effects.
- Deterministic tests.

### 9. Character Art and Weather

Status: `Done`

Add weather effects on top of the animation framework.

Scope:

- Weather states and overlays.
- Fallback behavior for small panels.

### 10. Diagnostics and Polish

Status: `Done`

Prepare the client for regular use and deeper troubleshooting.

Scope:

- Configurable diagnostic logging.
- Optional raw protocol logging.
- Reconnect workflow.
- Better connection/error status messages.
- Performance pass on output bursts and large scrollback.
- Documentation for setup, config, and troubleshooting.

### 11. Responsive Display Profiles

Status: `Done`

Replace generic responsive behavior with terminal-dimension profiles while keeping the map, MUD output, and command input visible everywhere.

Scope:

- Mobile, Tablet, 1080p, and Ultrawide terminal-cell breakpoints.
- Top-stacked map layouts for Mobile and Tablet.
- Configurable top, left, and right panes for 1080p and Ultrawide.
- Shared layout geometry for rendering, mouse interaction, and Telnet NAWS.
- Independent left and right pane resizing.
- Tests and documentation for profile boundaries and required panels.

### 12. Runtime Trigger Commands

Status: `Done`

Expose configured and session-only triggers through a local command matching the alias workflow.

Scope:

- `/trigger` listing for all currently loaded triggers.
- `/trigger {pattern} {command} [{command}...]` runtime additions.
- Regex capture substitution for runtime trigger commands.
- Replacement of same-pattern runtime triggers without restarting.
- Help, README, and deterministic command/engine tests.

### 13. Configurable Display Layouts

Status: `Done`

Restore the classic 1080p sidebar while making terminal profile selection and geometry configurable.

Scope:

- TOML-configurable Mobile, Tablet, 1080p, and Ultrawide breakpoints.
- Configurable stacked map height and compact status behavior.
- Classic 1080p sidebar with configurable left/right position and width.
- Ultrawide top, left, and right pane roles and dimensions.
- Shared rendering, mouse interaction, cursor, and Telnet NAWS geometry.
- Responsive optional-panel collapse with mandatory Map, MUD Output, and Command surfaces.

### 14. Script Event Dispatch

Status: `Done`

Turn internal trigger event records into a user-visible, configurable event system.

Scope:

- Shared typed script-event dispatcher with an `AppEvent::Script` integration point.
- Configurable event handlers and command actions.
- Handler commands that can drive existing map and panel commands, direct notifications, and event-animation scheduling groundwork.
- Recursion and command-rate protection.
- Local event inspection and diagnostics.
- Deterministic dispatch, handler, and failure tests.

### 15. Script Variables

Status: `Done`

Add shared variables that can be configured or created for the current session and substituted throughout the scripting pipeline.

Scope:

- TOML-defined variables loaded at startup and rebuilt on configuration reload.
- `/variable` listing for all active variables with configured/runtime source labels.
- `/variable {name} {value}` creation and replacement of session-only in-memory variables.
- `/variable unset {name}` removal of runtime variables without mutating configured values.
- Variable substitution in alias patterns/actions, trigger patterns/actions, and event handler matches/actions.
- One documented interpolation syntax with escaping, missing-variable diagnostics, deterministic expansion order, and recursion limits.
- Validation for variable names, duplicate definitions, recursive references, and expanded command limits.
- Runtime/configuration precedence rules, help text, README examples, and deterministic tests across aliases, triggers, events, and reload behavior.

### 16. Styled Echo and Input Variables

Status: `Done`

Add local output messages and make script variables available from the command input line.

Scope:

- `/echo <text>` output in the MUD output pane without sending text to the server.
- Optional `/echo --fg <color>` and `/echo --bg <color>` foreground/background styling.
- Named terminal colors and `#RRGGBB` color validation through the shared theme parser.
- `${name}` interpolation and `$${name}` escaping in directly entered commands.
- Interpolation before direct local-command, alias, movement, echo, or network dispatch.
- Preservation of variable templates entered through `/alias`, `/trigger`, and `/variable`.
- Help text, README examples, and deterministic command, styling, interpolation, escaping, and error tests.

### 17. Runtime Highlight Commands

Status: `Done`

Expose configured and session-only highlights through a local command matching the alias and trigger workflows.

Scope:

- `/highlights` listing for all currently loaded highlight rules and their configured/runtime source.
- `/highlights [plain|regex] {pattern} {foreground} [background] [styles]` runtime additions.
- `none` color placeholders and optional bold, dim, italic, underline, and reverse styles.
- Shared named and `#RRGGBB` color validation.
- Replacement of same-pattern runtime highlights without restarting.
- Runtime highlight persistence across `/reload`.
- Help, README, and deterministic command, engine, styling, replacement, validation, and reload tests.

### 18. Color-Aware Trigger Commands

Status: `Done`

Allow configured and runtime triggers to require ANSI foreground or background colors from MUD output.

Scope:

- `/triggers` plural alias for listing and creating runtime triggers.
- Optional `--fg <color>` and `--bg <color>` filters before runtime trigger fields.
- Foreground/background filters on persistent `[[triggers.rules]]`.
- Stateful ANSI SGR color tracking across output lines, including named, bright, indexed, and RGB colors.
- Combined text, category, cooldown, one-shot, foreground, and background matching.
- Runtime trigger listing and `/reload` retention of color filters.
- Help, README, sample configuration, and deterministic parser, engine, command, ANSI inheritance, reset, and validation tests.

### 19. Runtime Event Handler Commands

Status: `Done`

Expose complete event handler definitions and support session-only handlers from the command input.

Scope:

- Detailed `/handler` listing with source, match type, state, priority, commands, notifications, and emitted events.
- `/handler [plain|regex] {event} {command} [{command}...]` runtime handler additions.
- Keep `/event {name}` dedicated to manual event emission while `/event` retains its compact handler summary and recent event history.
- Replacement of same-event runtime handlers without restarting.
- Runtime handler persistence across `/reload`.
- Variable interpolation and regex capture substitution using existing event safety limits.
- Help, README, and deterministic engine, command, replacement, validation, and reload tests.

### 20. Lua Runtime Foundation

Status: `Done`

Add a sandboxed Lua runtime as the procedural scripting layer for advanced client logic.

Scope:

- Vendored Lua 5.4 runtime through `mlua`.
- `[lua]` configuration with script directory, entrypoint, action limits, and runtime error display.
- `scripts/init.lua` loading and reload behavior.
- `/lua status`, `/lua reload`, and `/lua call <function>`.
- Safe client API registration with no filesystem, OS, process, package, or debug globals.
- Compile and deterministic tests for load, reload, manual calls, syntax/runtime errors, and disabled Lua.

### 21. Lua Client API

Status: `Done`

Expose the full safe client API to Lua while routing all mutations through existing client systems.

Scope:

- Command, output, logging, variable, MSDP, character, opponent, group, room, scrollback, event, map, UI, and time APIs.
- Action queue limits per hook.
- Read-only state snapshots for hook execution.
- API-level tests for expected actions and read-only table contents.

### 22. Lua Hooks for Aliases, Triggers, and Events

Status: `Done`

Allow aliases, triggers, and event handlers to call named Lua functions.

Scope:

- `lua = "function_name"` on alias rules, trigger rules, and event handlers.
- Alias context with input and captures.
- Trigger context with normalized/raw line, category, captures, and ANSI colors.
- Event context with event name, source, and captures.
- Additive behavior alongside existing command, emit, and notification actions.
- Budget-aware hook execution through the existing command/event pipeline.

### 23. Lua Safety, Help, and Documentation

Status: `Done`

Polish the Lua scripting surface for regular use.

Scope:

- Runtime diagnostics and actionable error messages.
- Help topics for `/help lua`, `/help alias lua`, `/help trigger lua`, and `/help event lua`.
- README and default `config.toml` examples.
- Review pass for sandbox escape, runaway action, and reload edge cases.

### 24. Character Portrait Removal

Status: `Done`

Remove character portrait and face animation from the client so the character panel remains a static gauge and sheet display.

Scope:

- Remove portrait animation module and tests.
- Remove portrait asset files and generated portrait artifact.
- Remove character-art config fields and default TOML entries.
- Remove portrait references from end-user documentation.
- Keep map and weather animation support intact.

### 25. Map Command Documentation Completion

Status: `Done`

Close gaps between the implemented `/map` parser and user-facing map documentation.

Scope:

- Document every parsed `/map` subcommand in `docs/commands.md` and `docs/mapping.md`.
- Document map metadata options, mapper flags, room flags, exit flags, door aliases, and persistence commands.
- Expand README examples to include missing edit and flag commands.
- Expand in-client `/help map` and bare `/map` output so users can discover the full command surface.
- Validate documentation changes against the Rust parser and test suite.

### 26. Lua IntelliSense API Completion

Status: `Done`

Complete `scripts/mud-client-api.lua` so Lua Language Server users get useful completions and hover documentation for the exposed client API.

Scope:

- Add aliases for colors, output categories, directions, door states, panels, and toggle states.
- Document hook context tables for aliases, triggers, events, and manual calls.
- Document snapshot tables for character, opponent, group members, rooms, MSDP values, and events.
- Document every exposed `client.*` API table and method with parameter and return types.
- Validate Lua syntax and Rust test suite.

### 27. Full-Width Character Gauges

Status: `Done`

Make the character panel resource gauges use the full available row width.

Scope:

- Expand full health, mana, movement, and TNL gauge bars to fill the character panel row.
- Keep compact/sidebar status gauges on the configured fixed width.
- Preserve fixed-width numeric suffixes so values do not shift the layout.
- Add regression tests for resource and TNL full-width rendering.

### 28. Ultrawide Pane Placement

Status: `Done`

Swap the ultrawide default side panes so character details sit on the left and the map sits on the right.

Scope:

- Change built-in ultrawide defaults to `left = "details"` and `right = "map"`.
- Update the repository default `config.toml`, README, and UI/config docs.
- Update layout/config validation tests that assumed the old map-left default.
- Verify the installed config symlink points at the repository default config.

### 29. Ultrawide Nearby Map Dashboard

Status: `Done`

Replace duplicated ultrawide top dashboard detail panes with world context and a compressed nearby map.

Scope:

- Rename the default Info panel title to World while keeping the compatible `panels.info` config key.
- Make the ultrawide status dashboard render World plus Nearby Map only.
- Add a compressed linkless nearby map renderer centered on the current room.
- Butt route symbols together in Nearby Map and overlay up/down exit indicators.
- Let route glyphs consider incoming links as well as outgoing links.
- Keep the full linked map in the right ultrawide side pane.
- Add regression tests for the dashboard composition and nearby map rendering.

### 30. Map Persistence Configuration

Status: `Done`

Allow `config.toml` to load and save map files automatically.

Scope:

- Add `[map.persistence]` with `load_on_startup`, `path`, and `save_on_exit`.
- Resolve relative persistence paths beside the active `config.toml`.
- Load configured map files during app startup without aborting on errors.
- Save the current map during graceful shutdown when enabled.
- Document persistence options and add regression tests.

### 31. Social Communication Panel

Status: `Done`

Capture RoTS social output into a dedicated scrollable ultrawide dashboard panel.

Scope:

- Match the communication output patterns from the RoTS C++ communication source.
- Store timestamped social scrollback independently from MUD output scrollback.
- Extract the text inside RoTS single quotes and display `[HH:MM](channel) Prefix Text`.
- Include tell direction/player prefixes, yells, sings, and Social panel word wrapping.
- Render Social between World and Nearby Map in the ultrawide top dashboard.
- Add mouse-wheel scrolling for the Social panel.
- Add `layout.show_social`, `[social]`, and `panels.social` configuration.
- Update in-client help, README, and docs.
- Add regression tests for capture, scrolling, rendering, and config defaults.

### 32. File and Module Extraction Pass

Status: `Done`

Reduce the size and mixed responsibilities in `app.rs` without changing behavior.

Scope:

- Extract cohesive helper modules from `app.rs` where the boundary already exists.
- Keep the existing `App` runtime shape and public APIs intact.
- Move tests with the logic they cover when practical.
- Avoid new traits, frameworks, or abstractions that do not remove current complexity.
- Validate with formatting, full tests, and review.

### 33. Output Scrollback Clamp

Status: `Done`

Prevent MUD output scrolling from moving past the oldest full viewport of retained lines.

Scope:

- Clamp output rendering bounds using the current output pane height.
- Clamp the stored output scroll offset during scroll input so scrolling down is immediately responsive after reaching the oldest page.
- Preserve existing scroll controls and follow mode behavior.
- Add regression coverage for oversized scroll offsets.
- Update user-facing scrollback documentation.

### 34. Blank Enter Sends MUD Line

Status: `Done`

Allow an empty command buffer to send a blank line to the MUD so RoTS casting and concentration actions can be cancelled from the keyboard.

Scope:

- Convert empty input submission into a network text command.
- Preserve line-ending handling in the network writer.
- Avoid adding blank submissions to history or local command echo.
- Add regression coverage and update command input documentation.

### 35. Compact Full-Width Opponent Gauge

Status: `Done`

Make the opponent panel consume less sidebar height while rendering target health with the same full-width gauge style used by the character vitals.

Scope:

- Render opponent name and level on one compact line.
- Render opponent health with the full-width gauge renderer.
- Keep the opponent sidebar allocation fixed-height so it does not consume the same vertical space as map, group, or character.
- Update defaults, docs, and regression tests.

### 36. Prefix-Filtered Command History

Status: `Done`

Make command-history navigation behave like shell prefix search: when the input buffer has text, Up and Down cycle only through previous commands that start with that text.

Scope:

- Preserve full-history navigation when the input buffer is empty.
- Use the original typed draft as the prefix while cycling.
- Restore the draft when cycling forward past the newest match.
- Leave the input unchanged when no command matches the prefix.
- Update help/docs and add regression tests.

### 37. TinTin-Style Overlap Map Visibility

Status: `Done`

Match TinTin map display behavior when multiple rooms resolve to the same display coordinate by keeping the first visible room and hiding later overlapping branches.

Scope:

- Track occupied display positions during visible map traversal.
- Hide rooms that would overlap an already visible room.
- Do not traverse through hidden overlapping rooms.
- Preserve current-room priority and same-layer visibility.
- Add regression tests and document the behavior.

### 38. Compact Sidebar World Pane

Status: `Done`

Keep the classic sidebar World pane fixed to its compact content height so the map can consume more vertical space.

Scope:

- Keep the default World pane minimum height at 5 rows so weather can wrap to two lines.
- Allocate the classic sidebar World pane with a fixed-height constraint.
- Preserve the mandatory map panel and existing optional-panel priority behavior.
- Update user-facing layout documentation.
- Add regression coverage for the sidebar constraint.

### 39. Group Panel Default Off

Status: `Done`

Keep the optional Group panel disabled on startup unless the user enables it in config or toggles it in memory.

Scope:

- Change Rust layout defaults to start with the Group panel hidden.
- Change the repository default `config.toml` to match.
- Update user-facing configuration and toggle documentation.
- Preserve `/toggle group [on|off]` behavior.

### 40. Compact Character Base Stats

Status: `Done`

Render the six base character stats on one row to reduce vertical space in the Character pane.

Scope:

- Combine `Str`, `Int`, `Wil`, `Dex`, `Con`, and `Lea` into one metric row.
- Preserve combat and magic/stat resource rows.
- Update user-facing UI documentation.
- Add regression coverage for the combined base-stat row.

### 41. Character Sheet Gauge Spacer

Status: `Done`

Add a blank row between the Movement gauge and character stat sheet for clearer visual separation.

Scope:

- Insert one fixed spacer row after Health, Mana, and Movement gauges.
- Keep compact character rendering unchanged.
- Preserve TNL at the bottom of the Character pane.
- Update user-facing UI documentation.
- Add regression coverage for the spacer row.

### 42. Class-Specific Character Stats

Status: `Done`

Show a class-specific character sheet section below the base stats when Mage or Mystic is the highest class level.

Scope:

- Store RoTS MSDP class levels and regeneration values.
- Request and map class-level and regeneration MSDP variables by default.
- Show Mage details when Mage level is higher than every other class.
- Show Mystic details when Mystic level is higher than every other class.
- Add a blank row between base statistics and class-specific information.
- Update docs and tests.

### 43. Single-Line Class Stat Labels

Status: `Done`

Render class-specific Character pane details on one row with full labels.

Scope:

- Keep Mage class details on one row using `Mana Regen`, `Spell Power`, and `Spell Pen`.
- Keep Mystic class details on one row using `Willpower`, `Spirits`, `Health Regen`, and `Movement Regen`.
- Update docs and regression coverage.

### 44. Teleport Exit Marker

Status: `Done`

Add a configurable teleport marker for mapped directional exits that transport the player to non-adjacent rooms.

Scope:

- Add a `teleport` exit flag accepted by `/map exitflag`.
- Add `[map.teleport]` config with default `◇` glyph and cyan color.
- Validate the teleport glyph as one terminal cell.
- Render teleport markers on flagged exit links.
- Update mapping/config docs and regression coverage.

### 45. Door-Aware Movement Commands

Status: `Done`

Use directional door names stored in map data to prepare doors before movement commands.

Scope:

- Extend exit door data with an optional directional door name.
- Allow `/map door <dir> [state|none] [name]`.
- Keep door names directional and avoid copying them to reverse exits.
- Expand movement through closed, pickable, and locked named doors into open, pick, or unlock/open command sequences.
- Apply the behavior to typed movement, aliases, Lua sends, and `/map run`.
- Update in-app help and end-user docs.
- Add regression coverage.

### 46. Door State Movement Action Correction

Status: `Done`

Constrain door movement behavior to the supported RoTS door states and document the exact command expansion.

Scope:

- Accept only `trigger`, `unknown`, `open`, `closed`, `pickable`, `locked`, and `none`/`clear`/`off`.
- Keep `trigger`, `unknown`, and `open` as visual map data with no automatic movement command.
- Expand named `closed`, `pickable`, and `locked` doors into the correct movement preparation commands.
- Update in-game help, markdown docs, and Lua intellisense definitions.
- Add regression coverage.

### 47. Lua and Command Documentation Completeness

Status: `Done`

Make Lua scripting and local command documentation complete and example-driven for end users.

Scope:

- Document every Lua hook context and exposed `client` API in markdown.
- Add copy-ready Lua examples for aliases, triggers, events, mapper helpers, UI helpers, variables, MSDP, output, and snapshots.
- Add examples for local slash commands in the command reference.
- Update in-client Lua help with examples and documentation pointers.
- Validate markdown links and run formatting/test checks.

### 48. Repeat Command Prefix

Status: `Done`

Add TinTin-style repeat input using `#<count> {command}` while preserving semicolon command boundaries.

Scope:

- Parse repeat commands that start with `#` followed by a number and a braced command body.
- Repeat every command inside the braces, including semicolon-separated groups.
- Keep commands after the closing brace separate unless they have their own repeat prefix.
- Run repeated commands through aliases, local commands, door-aware movement, command echo, and network sending.
- Update in-client help, markdown docs, and regression coverage.

### 49. Rendering Performance

Status: `Done`

Reduce scrollback and map rendering work without changing displayed content.

Scope:

- Borrow scrollback and render only visible text spans.
- Preserve inherited ANSI styles by replaying from the latest reset boundary (or the start of retained history when no boundary exists).
- Use binary search for sorted output search matches.
- Lazily index incoming route links once per map pane, only when needed.
- Add regression coverage and a repeatable output timing probe.

Validation: 381 tests passed; the optional timing probe passed separately. Formatting and diff checks passed. Thranduil, Magus, and Sauron completed review; resolved unnecessary eager map indexing and redundant output bounds checks. Strict Clippy remains blocked by four pre-existing warnings: a derivable default in `src/map.rs`, a collapsible conditional and redundant closure in `src/scripting/lua.rs`, and the existing eight-argument marker helper in `src/ui/map.rs`.

### 50. Key Macros

Status: `Done`

- Bind single key presses and modifier chords to the existing command pipeline.
- Preserve command drafts and history, protect search editing and Ctrl+C, and require explicit overrides for built-in shortcuts.
- Provide opt-in printable-key mode and reported-repeat handling.
- Add persistent TOML definitions, runtime management, reload preservation, docs, and deterministic tests.
- Complete Thranduil/Magus/Sauron subagent reviews.

Validation: 393 tests passed (including 12 macro tests); one optional performance probe remains ignored. Formatting and diff checks passed. Thranduil, Magus, and Sauron completed separate subagent reviews. Fixed shifted Alt-key normalization and clarified inherited slash-command semicolon behavior. Strict Clippy still reports the four pre-existing warnings documented in slice 49. Terminal-dependent chord reporting and held-key behavior are documented in the macro reference.

### 51. Dedicated Numpad Bindings

Status: `Done`

- Distinguish numpad digits, operators, Enter, and Num Lock navigation aliases from ordinary keys.
- Preserve generic binding fallback while allowing dedicated numpad macros outside printable-key mode.
- Enable enhanced keyboard reporting on supported terminals and restore it on exit.
- Cover key identity, modifiers, repeats, reload, draft preservation, and terminal lifecycle.
- Document terminal/backend limitations and complete subagent review.

Validation: 399 tests passed; one optional timing probe ignored. PTY checks passed with enhanced keypad sequences and legacy input, including top-row/main-Enter separation, modifiers, preserved drafts, and terminal restoration. Formatting/diff checks passed; the four existing Clippy warnings remain. Thranduil, Magus, and Sauron completed reviews; fixed the blocking capability probe and lost Shift on enhanced Ctrl-letter events. Dedicated bindings require terminal-reported keypad identity; native Windows support remains limited by the current input backend.

### 52. First-Use Help and Wrapped Scrollback

Status: `Done`

- Open `/help` at the requested heading instead of showing only its tail.
- Track logical-line anchors plus wrapped-row offsets, allowing every displayed row to be reached even with little retained history.
- Keep rendering and scroll calculations bounded to relevant logical lines (with existing ANSI-prefix replay as needed).
- Cover first/repeated macro help, bidirectional scrolling, display modes, narrow/empty geometry, resizing, snapshots, and ordinary appended output.

Validation: 403 tests passed; optional timing probe passed separately. Formatting and diff checks passed. Thranduil, Magus, and Sauron completed separate reviews; removed unnecessary anchor rendering for zero row offsets and repaired the scrolling-controls list. Strict Clippy still reports the four existing warnings from slice 49. Sauron noted pre-existing paused-anchor drift during snapshot batches, prompt removal, and scrollback eviction; broader anchor lifecycle changes and dedicated regression coverage are deferred outside this help/wrapping fix.
