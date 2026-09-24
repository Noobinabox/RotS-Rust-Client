# Input and keyboard behavior

Optional [Vim-style editing](vim.md) adds Insert/Normal modes, operators, visual selection, registers, undo, and repeat. Enable `[terminal] input_mode = "vim"`; `/help vim` provides examples. The standard-mode shortcuts below remain the default.

## Submission

- `Enter` sends the current input; empty input sends a blank line.
- `Enter` on the highlighted last command resends it.
- Submitting input returns scrolled output to the latest lines and resumes following new output. Editing a draft does not move the output view; commands such as `/help` can then select their own view.
- `;` separates multiple MUD commands.
- `#<count> {command}` repeats a command or braced command group.
- `/quit` exits; `Ctrl-C` clears input and never exits.

## Editing and history

- `Left`, `Right`, `Home`, and `End` move the cursor.
- `Ctrl-Left` / `Alt-B` move back one word; `Ctrl-Right` / `Alt-F` move forward one word.
- `Ctrl-Backspace`, `Alt-Backspace`, or `Ctrl-W` delete the previous word; `Ctrl-Delete` or `Alt-D` delete the next word.
- `Up` and `Down` navigate history; typed input filters history by prefix.
- `Tab` completes the current word from recent MUD output.
- `Shift-Tab` cycles completion backward.
- Typing clears the completion dropdown.

Words are non-whitespace runs (punctuation stays part of a word). Movement skips adjacent whitespace and then the word in that direction; deletion removes the same range. Unicode whitespace and multiline boundaries count as separators. These shortcuts also work in output search without changing the command draft. Movement restores a highlighted last submission for editing; deletion clears it, like ordinary Backspace. Word deletion ends history navigation; movement preserves it. Completion is dismissed when editing a command.

Your terminal must report the chord distinctly. Some terminals send ordinary Backspace for Ctrl-Backspace or intercept Alt shortcuts; use Ctrl-W or another listed alternative. Existing macros on these shortcuts now require `override_builtin = true` (or `/macro --override`); search editing still takes priority over macros.

For example, type `say hello world`, press Ctrl-Left to reach `world`, then Ctrl-W to remove `hello `, leaving `say world`. To override word deletion with a macro, use `/macro --override {Alt+D} {look}`.

## Paste and multiline editing

On supported terminals, bracketed paste inserts text without triggering macros, shortcuts, completion, or commands—even when printable-key macros are enabled. Pasting replaces a highlighted previous submission, or inserts at the cursor in a draft. In search mode it edits the search text without submitting it.

By default, pasted newlines become spaces. For example, pasting two lines containing `say hello` and `world` produces `say hello world`; nothing is sent until you press Enter. CRLF and CR line endings are normalized, tabs become spaces, and other control characters are removed.

To preserve newlines, add this to shared configuration or a character profile and run `/reload`:

```toml
[terminal]
multiline_input = true
```

With multiline input enabled:

- `Alt+Enter` adds a newline instead of sending. This editing chord takes priority over macros.
- Newlines appear as `↵` markers in the compact, horizontally scrolling input pane; the pane does not grow vertically.
- Left/Right, Home/End, Backspace/Delete edit the entire draft, including line boundaries. Up/Down still navigate command history.
- Enter explicitly submits nonblank lines in order through the normal command/alias pipeline. `/quit` stops subsequent lines. The draft is one history entry; resending submits it again.

For example, paste `look` and `score` on separate lines, inspect `look↵score`, then press Enter to execute both. Semicolons, aliases, variables, and repeat syntax keep their normal meaning **after submission**. Multiline drafts are sequences of commands, not multiline Lua scripts or multiline alias definitions.

Paste payloads and resulting pasted drafts are limited to 64 KiB. The normalized draft is limited to 128 logical lines; in single-line mode incoming newlines become spaces before this line limit is checked. Oversized pastes are rejected as a whole without changing the draft. Crossterm collects the payload before this application limit is checked. Manually assembled multiline submissions are also limited to 128 lines. A draft consisting only of blank multiline rows sends nothing; Enter on a genuinely empty input still sends a blank command.

Safety depends on terminal support: terminals that do not report bracketed paste—including the current native Windows input backend—deliver ordinary key events, which cannot reliably be distinguished from typing. Such pastes may trigger macros or execute newline-separated commands; do not paste untrusted or multiline text there. Alt+Enter also requires the terminal to report its modifier distinctly; otherwise use bracketed paste for multiline editing. Bracketed-paste mode is restored on exit.

## Output navigation

- `PageUp` and `PageDown` scroll by page.
- `Ctrl-Up` and `Ctrl-Down` scroll one line.
- `Ctrl-E` follows newest output.
- `Ctrl-L` clears output.
- `Ctrl-F` starts search; `Ctrl-N` and `Ctrl-P` navigate matches.
- Mouse wheel scrolls output when the pointer is over the output pane.
- `F2` cycles styled, plain, and debug output views.

## Repeat examples

```text
#10 {kill orc}
#3 {look;score}
#2 {look;score};rest
```

In the last example, only `look;score` repeats; `rest` runs once.
