# Configurable Shortcuts

Configure built-in client actions in shared `config.toml` or a character profile, then use `/reload`. No bindings are installed by default: existing shortcuts continue to work. These bindings change client actions, not commands sent to the MUD; use `/macro` for command sequences.

## Examples

Assign F6 to output search, Alt-J to history navigation, and F8 to reconnect:

```toml
[[keybindings.bindings]]
key = "F6"
action = "search_output"

[[keybindings.bindings]]
key = "Alt+j"
action = "history_next"

[[keybindings.bindings]]
key = "F8"
action = "reconnect"
```

Bindings overlay the existing shortcuts by key. Adding F6 does not remove Ctrl-F. To disable an existing shortcut explicitly:

```toml
[[keybindings.bindings]]
key = "Ctrl+L"
action = "none"
```

Remove a binding from the file and `/reload` to restore that key's default behavior. Use `bindings = []` inside `[keybindings]` to clear all custom bindings. Character profiles replace the shared bindings array, rather than merging individual entries. `/save` does not modify keybindings; they remain file-managed.

If you disable or replace Enter, bind another key to `submit` first so you can still enter `/reload` and `/quit`. Ctrl-C and Escape do not reset your configuration.

## Actions

| Action | Behavior |
|---|---|
| `none` | Consume the key without doing anything. |
| `submit` | Submit the command draft through the normal command pipeline. |
| `backspace`, `delete` | Delete the character before/at the cursor. |
| `cursor_left`, `cursor_right` | Move one character. |
| `line_start`, `line_end` | Move to the start/end of the entire draft. |
| `word_left`, `word_right` | Move across whitespace-delimited words. |
| `delete_word_left`, `delete_word_right` | Delete a word before/after the cursor. |
| `history_previous`, `history_next` | Navigate prefix-filtered command history. |
| `complete_next`, `complete_previous` | Cycle command completions. |
| `scroll_page_up`, `scroll_page_down` | Scroll output by ten rows. |
| `scroll_line_up`, `scroll_line_down` | Scroll output by one row. |
| `follow_output` | Return to the latest output. |
| `clear_output` | Clear retained output. |
| `search_output` | Open output search. |
| `search_next`, `search_previous` | Navigate output matches. |
| `toggle_output_display_mode` | Cycle styled, plain, and debug output. |
| `reconnect` | Replace the connection with a fresh connection to the same server. |
| `reload` | Reload configuration. |
| `quit` | Exit the client. |

## Precedence and safety

- Ctrl-C and Escape are protected and cannot be rebound, including by macros. Ctrl-C clears input/leaves printable macro mode; Escape retains search/Vim recovery behavior. Existing Escape macros must be moved to another key.
- Output search and Vim history search retain their own keys; custom shortcuts and macros do not intercept them.
- Vim owns its editing keys before either custom shortcuts or macros. Editing actions (including `submit`) assigned to other keys are ignored in Vim mode to preserve its undo/register behavior. Global actions on function, page, or dedicated numpad keys remain usable.
- With multiline input enabled, Alt-Enter remains the newline insertion shortcut and takes precedence.
- An enabled macro on a custom shortcut needs `override_builtin = true`, or `/macro --override {F6} {look}` at runtime. An explicit macro override wins over the custom shortcut. This also applies to `none` bindings. Invalid reloads retain the previous bindings.
- Printable keys without Ctrl/Alt are rejected so ordinary typing stays available. Key names and terminal/numpad reporting follow [macro key syntax](macro.md). Duplicate normalized keys, unknown actions, and more than 1,024 bindings are rejected.
- Key releases are ignored. Editing/navigation actions can repeat; submit, reconnect, reload, quit, clear, search opening, and display-mode toggling do not repeat.

`/help keybindings` and `/help shortcuts` show this guide. `/help input` describes the default shortcuts.
