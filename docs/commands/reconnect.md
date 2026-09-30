# `/reconnect`

## Purpose

Close the current connection, if any, and open a fresh connection to the same server. This also works after the server disconnects or a connection attempt fails.

## Syntax

```text
/reconnect
```

The client reports `Reconnect requested.` followed by a connection result. You may need to log into the MUD again. Your client remains running, preserving session-only settings.

## Examples

To replace a live connection or recover a dropped connection:

```text
/reconnect
```

To connect to a different host or port, save the active `config.toml`, exit the client, and launch it again:

```text
/quit
```

`/reconnect` reuses the network settings selected at launch, including `--local`; `/reload` does not replace those settings. Reconnection is explicit, not an automatic retry loop. Commands entered while disconnected or connecting are discarded rather than replayed into a new session.
