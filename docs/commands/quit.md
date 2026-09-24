# `/quit`

## Purpose

Exit through the normal shutdown path, restoring the terminal and disconnecting the network task cleanly.

## Syntax

```text
/quit
```

`Ctrl-C` clears input and deliberately never quits.

## Examples

When you are ready to close the client, enter:

```text
/quit
```

This disconnects and exits immediately; it does not send the MUD's own logout command. Follow your MUD's logout procedure first if needed. Session-only settings are not automatically saved.
