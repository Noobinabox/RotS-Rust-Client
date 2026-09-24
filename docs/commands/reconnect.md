# `/reconnect`

## Purpose

Send a reconnect request to the client network task. **Currently incomplete:** the network task does not act on this request, so this command does not establish a new connection.

## Syntax

```text
/reconnect
```

The client reports `Reconnect requested.`, but that message is not evidence of a new connection. Restart the client to recover from a dropped connection or use a changed host/port.

## Examples

This demonstrates the request command only; do not rely on it to recover a connection:

```text
/reconnect
```

For a working recovery, exit the client:

```text
/quit
```

Then launch it again using your usual terminal command. If you changed the host or port, save the active `config.toml` before relaunching. `/reload` does not replace the existing connection, and `--local` still forces the local test endpoint. Restarting may require logging into the MUD again and loses session-only settings.
