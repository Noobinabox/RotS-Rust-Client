# `/history`

Inspect command history with `/history` or `/history status`. `/history clear` clears the active character's history (or temporary login history before identification). It also replaces that character's saved history on the next save tick or graceful exit when persistence is enabled.

To restore history after restarting, add this to shared configuration or a character profile, then `/reload`:

```toml
[history]
persist = true
max_entries = 1000
ignore_prefixes = ["password ", "passwd "]
```

Persistence defaults to `false`. History is automatically selected by the server's MSDP character name, even if that character has no profile file. Before the first identification on a connection, login history is temporary and never saved. `--character` selects settings but does not authorize saving login input. History returns when the server identifies the character. Different characters have separate history even when sharing automation settings.

The default limit is 1000 entries; `max_entries` accepts 0..=10000, with 0 disabling history. History also has a 1 MiB total text limit and a 64 KiB per-entry limit. The oldest entries are dropped first and consecutive duplicate submissions are suppressed. Existing Up/Down prefix navigation and Vim history search work with restored history. Multiline submissions remain single entries.

A command beginning with a space is excluded from history, as is an entire multiline submission containing a leading-space line or a line matching an `ignore_prefixes` entry (case-sensitive). These rules affect both memory and disk. This does not suppress local echo or change command execution. The server’s Telnet ECHO signal also protects password entry, including account-menu password changes on an existing connection: input is hidden, excluded from history/repeat, and sent directly without aliases, expansion, or local echo. Recording into character history stays suspended until a fresh MSDP identity arrives. When echo returns, the client renews the identity report so RoTS also identifies an unchanged character on reentry. Previously recorded history can still save in the background. During password entry, Ctrl+] exits cleanly, and explicitly configured `quit`/`reconnect` shortcuts remain available. `/quit` typed into the password field is literal password text. Partial password drafts are discarded when echo returns or the connection ends. Servers that omit this signal cannot reliably identify bare passwords; use the leading-space/prefix exclusions for other sensitive commands. History files are plain text.

Files live in `history/<lowercase-character>.toml` beside shared `config.toml`, separately from `/save` automation snapshots. New files use owner-only permissions on Unix. Disk loading/saving runs on background workers. Modified histories save automatically on gameplay ticks, at most once per second, and graceful exit flushes pending writes. Abrupt termination can lose recent commands. `/reload` applies limits and exclusions without rereading already-loaded history; disabling persistence leaves the existing file on disk.

Invalid or oversized files are left intact and disable saving for that character until restart. Save failures are shown in the output pane; failures during the final exit flush are returned after terminal restoration. `/history status` reports whether persistence is enabled, pending, waiting for identity, or unavailable after an error. Concurrent clients use a lock and detect external changes instead of overwriting them; restart to reload after a conflict. After a crash, remove a stale `.toml.lock` only after confirming no client is saving. Atomic replacement protects against partial writes; power-loss durability is not guaranteed. History persistence requires a configuration path.
