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

```text
/lua status
/lua reload
/lua call smoke_test
```

See [Lua API](../lua-api.md) and [Lua examples](../lua-scripting-examples.md).
