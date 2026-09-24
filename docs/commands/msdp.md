# `/msdp`

## Purpose

Display every MSDP variable currently stored by the client, including nested arrays and tables. Values are sorted by variable name.

## Syntax

```text
/msdp
```

This is a local diagnostic snapshot. It does not request new values or change subscriptions.

## Examples

After connecting, inspect the values the server has sent:

```text
/msdp
```

Look for variables such as `HEALTH`, `HEALTH_MAX`, or `ROOM` when diagnosing a missing gauge or map update. Only received variables are shown; names depend on the MUD.
