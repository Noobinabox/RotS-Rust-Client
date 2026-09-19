# Troubleshooting

## Config Changes Do Not Apply

Run:

```text
/reload
```

The active config path is normally `~/.config/mud-client/config.toml`. The repository `config.toml` is only used directly when running from the repository without an installed config.

## Connection Uses the Wrong Host

Use the local override while testing:

```sh
cargo run -- --local
```

Or set:

```toml
[connection]
host = "localhost"
port = 3791
```

## Output Wraps Too Early At Startup

The client waits for the first terminal layout before normal wrapping. If wrapping remains wrong after the UI settles, resize the terminal once and confirm the output pane has enough width for the selected profile.

## Game Spacing Is Missing

MUD output should preserve spacing. If formatting looks collapsed, check whether you are viewing plain/debug output mode with `F2` and verify no trigger or external terminal setting is rewriting whitespace.

## Colors Bleed Between Commands

ANSI state is reset at prompt boundaries. If a color still bleeds, capture the affected raw text in debug mode and check whether the MUD sent an incomplete ANSI reset sequence.

## Map Is Wrong

Use:

```text
/msdp
/map get
```

The mapper must receive the MSDP `ROOM` table and `ROOM_EXITS`. Room description text is not used for links because players can turn that display on or off in the game.

## Lua Hook Does Not Run

Check:

```text
/lua status
/lua reload
```

Confirm `[lua] enabled = true`, the hook function exists in `scripts/init.lua`, and the alias, trigger, or event handler has `lua = "function_name"`.

## Lua Editor Completion Is Missing

Use an editor with Lua Language Server support and open the repository root. `.luarc.json` includes `scripts`, and `scripts/mud-client-api.lua` defines the `client` API annotations.

## Mouse Scrolling Or Resizing Does Not Work

Set:

```toml
[terminal]
mouse = true
```

Some terminal multiplexers also need mouse mode enabled.

