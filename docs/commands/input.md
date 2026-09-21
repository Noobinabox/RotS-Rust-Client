# Input and keyboard behavior

## Submission

- `Enter` sends the current input; empty input sends a blank line.
- `Enter` on the highlighted last command resends it.
- `;` separates multiple MUD commands.
- `#<count> {command}` repeats a command or braced command group.
- `/quit` exits; `Ctrl-C` clears input and never exits.

## Editing and history

- `Left`, `Right`, `Home`, and `End` move the cursor.
- `Up` and `Down` navigate history; typed input filters history by prefix.
- `Tab` completes the current word from recent MUD output.
- `Shift-Tab` cycles completion backward.
- Typing clears the completion dropdown.

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
