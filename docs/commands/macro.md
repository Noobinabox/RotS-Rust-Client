# Macro Help

Bind a key to a command that runs immediately, without Enter. The current input draft, cursor, and history are preserved.

## Commands

- `/macro` — list configured and runtime bindings, flags, and printable-key mode.
- `/macro [--override] [--repeat] {key} {command}` — add or replace a session binding.
- `/macro remove {key}` — remove a session binding and reveal any configured binding.
- `/macro clear` — remove all session bindings.
- `/macro mode on|off` — enable or disable printable-key bindings for this session (off at startup).

## Examples

### Try a function-key binding

Enter this command, then press F5 (no Enter needed for the key press):

```text
/macro {F5} {look}
```

The client sends `look` through the normal command pipeline without replacing your input draft. Inspect bindings and remove the test binding when finished:

```text
/macro
/macro remove {F5}
```

Removal reveals any configured F5 binding underneath the session override. Runtime changes last for this session unless you run `/save`; use that command or the TOML example below to keep a binding across restarts.

### More binding examples

Choose the bindings you want; these are independent examples:

- `/macro {F5} {cast 'heal' self}`
- `/macro {F6} {get bread bag;eat bread}`
- `/macro {Ctrl+G} {/lua call start_bot}`
- `/macro {F7} {kill ${target}}`
- `/macro --override {F2} {look}`
- `/macro {n} {north}` then `/macro mode on`

Ctrl+C always leaves printable-key macro mode and retains normal input handling. Search editing takes priority over macros. Bound printable keys consume their characters in macro mode; unbound keys still edit input. Function keys and Ctrl/Alt combinations work without macro mode.

Existing editing/navigation shortcuts require `--override` (or `override_builtin = true` in TOML). Ctrl+C cannot be rebound. Key names are case-insensitive except printable characters: `a` and `A` are different. `Shift+a` means `A`. Supported names: F1–F24, Enter, Esc/Escape, Tab, BackTab/Shift+Tab, Backspace, Delete, Insert, Home, End, Up, Down, Left, Right, PageUp, PageDown, Space, Plus, and single printable characters. Modifiers are Ctrl/Control, Alt, and Shift, joined with `+`.

Release events never execute commands. Reported repeat events are consumed without execution unless `--repeat` / `allow_repeat = true` is set. Some terminals report held keys as repeated presses, so this cannot prevent every held-key repetition. Terminals can also intercept shortcuts or report different chords identically (for example Tab and Ctrl+I); bind the key the terminal actually delivers.

Commands use the normal command pipeline: aliases, variables, semicolon groups, repeats, movement preparation, local commands, and Lua calls all work. `${name}` templates are stored at definition time and expanded when pressed. Missing variables produce an error. Macro commands themselves can deliberately modify client state, including the input via a Lua hook. `/quit` works as a standalone macro command, just as direct input.

Existing slash-command boundaries still apply: a command beginning with `/` consumes the remaining text, so `/macro {F5} {/echo ready;look}` echoes `ready;look` without sending `look`. Use an alias with separate command actions, or a Lua function, for multi-step actions beginning with local commands.

## Dedicated numpad keys

- `/macro {Numpad8} {north}`
- `/macro {Numpad2} {south}`
- `/macro {Numpad4} {west}`
- `/macro {Numpad6} {east}`
- `/macro {Numpad5} {look}`
- `/macro {NumpadEnter} {score}`
- `/macro {Ctrl+Numpad8} {scan north}`

Dedicated numpad bindings work without `/macro mode on` or `--override`. They leave top-row digits, regular arrows, and main Enter available for normal input. Ctrl/Alt/Shift modifiers, repeat options, search priority, runtime replacement, and reload work as for other macros.

Supported names: `Numpad0`–`Numpad9`, `NumpadDecimal`, `NumpadAdd`, `NumpadSubtract`, `NumpadMultiply`, `NumpadDivide`, `NumpadEnter`, `NumpadEqual`, and `NumpadSeparator` (comma). Names are case-insensitive. Keypad navigation reports with Num Lock off map to the same physical positions: Insert→0, End→1, Down→2, PageDown→3, Left→4, Begin→5, Right→6, Home→7, Up→8, PageUp→9, Delete→Decimal. These names also work as aliases, for example `NumpadUp` is the same binding as `Numpad8`.

An explicit numpad binding takes priority over a generic binding such as `8` or `Up`. Without a dedicated binding, existing generic macros and input editing continue to handle the reported key. A disabled dedicated binding returns to normal editing rather than running a generic macro.

On Unix (including WSL), startup requests the [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) for distinct key identity, shifted characters, and press/repeat/release reporting, without waiting for a capability query. The terminal's previous keyboard mode is restored on exit. Your terminal and any multiplexer must preserve keypad identity. Unsupported terminals ignore the request and continue working normally, but dedicated numpad bindings cannot fire when keypad identity is absent; ordinary `8` is never guessed to mean `Numpad8`. Native Windows builds currently lack keypad identity in the installed Crossterm backend. Use an ordinary binding as a fallback there. Num Lock behavior ultimately depends on what the terminal reports. Shifted non-letter Ctrl chords may arrive as their shifted symbols through Crossterm; bind the delivered symbol rather than relying on a physical keyboard layout.

## Persistent configuration

```toml
[[macros.rules]]
key = "F5"
command = "cast 'heal' self"
enabled = true
override_builtin = false
allow_repeat = false
```

Definitions default to enabled, with override and repeat disabled. Invalid keys, blank commands, protected-key bindings, shortcut conflicts, and duplicate normalized keys are rejected. Runtime definitions replace the effective configured binding for that key. `/reload` refreshes configured bindings while preserving runtime definitions and printable-key mode. Edit TOML to remove configured macros.

Alternatively, save an in-game binding without editing TOML:

```text
/macro {F5} {look}
/save
```

Wait for `Saved runtime settings`. This writes the runtime snapshot beside your main configuration, not into it. Run `/save` again after removing or changing saved bindings. Printable-key mode is never persisted. See [`/save`](save.md) for conflicts, file location, and limits.
