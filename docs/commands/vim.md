# Vim-style command editing

## Enable

Add this to shared configuration or a character profile, then run `/reload`:

```toml
[terminal]
input_mode = "vim"
```

The default is `"standard"`. This is a bounded command editor, not embedded Vim. The right edge of the input pane displays INSERT, NORMAL, VISUAL, or V-LINE (a single-letter indicator in narrow panes), leaving the input text's starting position unchanged. SEARCH indicates the separate, nonmodal output-search editor.

## Modes and submission

- Start in Insert mode and type normally. Esc enters Normal mode.
- Enter sends the draft from either mode and returns to Insert; multiline drafts retain their existing ordered submission behavior.
- `i` inserts before the cursor; `a` after it; `I` at the first nonblank character of the current line; `A` at its end.
- Ctrl-C clears the draft, cancels pending operations, disables printable macro mode, and returns to Insert.
- Normal-mode `j`/`k` and Up/Down navigate command history using the existing prefix filter. They do not move vertically through multiline drafts. Editing never rewrites stored history entries.
- With `multiline_input = true`, Alt-Enter inserts a newline in Insert mode only. Bracketed paste is literal text, never commands; it replaces a visual selection and is one undo action.
- Output search keeps its existing editor. Esc closes search before affecting Vim mode.

Submitting resumes live output. A local command such as `/help` can subsequently select its own view. Reload and character changes preserve the draft but reset pending operations, undo, and repeat state to Insert mode. Registers remain session-local.

## Command-history search

In Normal mode, `/` opens a command-history search prompt toward older entries; `?` searches toward newer entries. From a fresh draft these start at the newest and oldest entry respectively. This searches all in-memory commands, independently of the `j`/`k` prefix filter; it does not search MUD output.

- Type a pattern, then Enter to recall a matching command in Normal mode. **This Enter does not send it**; press Enter again to execute the recalled command.
- `n` finds the next matching history entry in the original direction; `N` finds one in the opposite direction. Searches wrap around. Counts work: `3n` advances three matching entries, and `2/pattern` selects the second match after Enter.
- Empty `/` or `?` followed by Enter reuses the last accepted pattern with the requested direction. The pattern survives command submission but is never saved to disk.
- Esc cancels the prompt without changing your draft or its cursor. Left/Right, Home/End, Backspace/Delete, word-editing shortcuts, and Ctrl-U edit the query. Pasting edits only the query; pasted newlines become spaces.
- The right-hand mode indicator reads H-SEARCH (H when narrow). While the prompt is open, command macros—including function keys and numpad bindings—are suppressed.
- A valid search with no match reports that result and leaves the draft unchanged. Invalid patterns stay open for correction. Ctrl-C retains its recovery behavior: cancel search and clear the draft.

Patterns use the client's Rust regular-expression syntax, case-sensitive by default: `^say ` matches commands beginning with `say`, `heal|cure` matches either word, and `(?i)orc` ignores case. This is not Vim's full regex dialect: Vim-specific escapes such as `\c` are not supported. Literal punctuation may need escaping, such as `\.`. Matching selects one result per history entry, including duplicate commands.

For example, press Esc, type `/^cast `, then Enter to recall a casting command. Use `n`/`N` to browse matching commands; edit with `i`, or press Enter again to send. To type a MUD-client slash command such as `/help`, enter Insert mode first.

Queries are limited to 4 KiB; regex compilation and history scanning have bounded work limits. A limit error leaves the command draft unchanged. Reported Enter repeats after accepting a result are suppressed, including numpad Enter; terminals that report held keys as fresh presses cannot provide this protection. This is session history only, not persistent history or incremental preview while typing. Ctrl-F still searches MUD output separately.

## Motions and edits

| Keys | Behavior |
|---|---|
| `h`, `l`, Left, Right | Move one Unicode character within the current logical line. |
| `w`, `b`, `e` | Next word, previous word, end of word; punctuation forms separate runs. |
| `W`, `B`, `E` | The same motions using whitespace-delimited words. |
| `0`, `^`, `$`, Home, End | Start, first nonblank, or end of the current logical line. |
| `f{char}`, `F{char}` | Find a character forward/backward within the current line. |
| `t{char}`, `T{char}` | Move just before/after the found character. |
| `;`, `,` | Repeat the last character search in the same/opposite direction. |
| `d{motion}`, `c{motion}`, `y{motion}` | Delete, change, or yank the motion range. Change enters Insert mode. |
| `dd`, `cc`, `yy` | Delete, change, or yank logical lines. |
| `x`, `X`, `D`, `C` | Delete at/before cursor, delete to line end, or change to line end. |
| `r{char}` | Replace characters without entering Insert mode. |
| `u`, Ctrl-R | Undo and redo draft edits. Insert sessions are grouped; cursor movement or paste starts a separate transaction. |
| `.` | Repeat the last completed text change, including the actual inserted/completed text—not submission, history, or MUD commands. |

Counts apply to motions, operators, character replacement/deletion, history, register puts, undo/redo, and dot-repeat. For example, `3w`, `2dw`, `d2w`, and `2d3w`. Counts before mode switches do not repeat insertion. Unsupported commands and incomplete operations do not send anything; Esc cancels a pending count/operator.

## Text objects, selection, and registers

- Use `iw`/`aw` or `iW`/`aW` after an operator or in Visual mode for inner/around words.
- Use inner/around parentheses, brackets, braces, single quotes, and double quotes: `di(`, `ca[`, `yi{`, `ci'`, `da"`. Matching bracket aliases and `b`/`B` objects are accepted. Escaped delimiters are ignored; missing enclosing delimiters leave the draft unchanged. Quote objects stay within one line; delimiter counts choose enclosing pairs from inside out.
- `v` starts characterwise selection; `V` starts linewise selection. Motions extend it; `d`, `c`, `y`, `r`, or `p`/`P` operate on it. Esc cancels selection. There is no blockwise selection.
- Yanks/deletions populate the unnamed register. `p`/`P` put after/before the cursor, or replace a selection; linewise values put on adjacent lines.
- Prefix with `"a` through `"z` to select an internal register: `"ayiw`, then `"ap`. Puts retain register contents. These registers are not the system clipboard and are never written by `/save`.

## Examples

1. Type `say hello world`, press Esc, then `0wcwfriend`, Esc, Enter: sends `say friend world`.
2. Type `get 'iron sword' bag`, press Esc, then `0f'ci'steel axe`, Esc, Enter: sends `get 'steel axe' bag`.
3. Type `one two three`, press Esc, then `0dw`: draft becomes `two three`. Press `.` to get `three`; `u` restores the preceding edit.
4. Type `say hello`, press Esc, then `0wviwy0P`: copies `hello` to the front without sending anything.

## Safety and compatibility

Vim owns printable keys and editing chords even when a command macro has `--override`. Nonconflicting function-key and dedicated numpad macros remain available; disabling Vim mode restores existing macro behavior. Ctrl-F/E/L/N/P retain output/search controls. Normal mode ignores other Ctrl/Alt editing chords; use the listed Vim commands. Existing word-editing chords remain available in Insert mode.

Draft edits are limited to 64 KiB and 128 logical lines. Undo and redo each retain at most 100 snapshots and 1 MiB of text. Counts saturate at 10,000, repeat recipes are limited to 4,096 events, and a bounded work budget cancels oversized operations or rolls back excessive dot replay. Repeated dot counts also stop at the recipe-event budget. These limits favor responsiveness over unrestricted Vim compatibility.

Movement preserves UTF-8 boundaries, not grapheme-cluster editing: combining marks and joined emoji may be edited as separate Unicode characters. Rendering retains grapheme-aware viewport behavior.

Ex commands (`:`), Vimscript, recorded Vim macros (`q`/`@`), block selection, uppercase/numbered/clipboard registers, and full Vim compatibility are not supported. Terminal reporting still matters: Ctrl-R, Alt-Enter, keypad identity, and bracketed paste must reach the client distinctly. On terminals without bracketed-paste reporting, pasted text may be interpreted as keys; do not paste untrusted text there.
