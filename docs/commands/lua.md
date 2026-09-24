# `/lua`

## Purpose

Inspect, reload, or manually call the configured sandboxed Lua entrypoint.

## Syntax

```text
/lua
/lua status
/lua reload
/lua call <function>
```

Lua is controlled by `[lua]` configuration. Hooks use the safe `client` API; filesystem, process, OS, package, and debug globals are unavailable.

## Examples

Check whether Lua is enabled and whether the configured entrypoint loaded successfully:

```text
/lua status
```

For a harmless first callback, add this function to your configured Lua entrypoint (with `[lua] enabled = true`). If you have not configured an entrypoint yet, follow [Lua setup](../lua-api.md) first:

```lua
function smoke_test(ctx)
  client.echo("Lua is ready")
end
```

Save the script, then reload it and call the function one command at a time:

```text
/lua reload
/lua call smoke_test
```

You should see `Lua is ready` in local output. `smoke_test` is an example function, not a built-in command; calling it before defining and loading it fails.

See [Lua API](../lua-api.md) and [Lua examples](../lua-scripting-examples.md).

The opt-in bot controller is documented in [Lua Botting](../botting.md).
