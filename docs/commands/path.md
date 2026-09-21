# `/path`

## Purpose

Record and replay a manual movement path, following the core behavior of TinTin++’s `#path` command. Path state is session-only and separate from the persisted room map.

## Syntax

```text
/path
/path <subcommand> [arguments]
```

## Record a path

```text
/path create
n
e
/path stop
/path describe
```

`create` clears the current path and begins recording. `start` resumes recording without clearing. Normal cardinal movement commands are captured with reverse directions automatically. `stop` stops recording; `destroy` clears the path and stops recording.

## Edit and navigate

```text
/path insert {open north} {close south}
/path delete
/path undo
/path get length
/path get position
/path goto start
/path goto end
/path goto 2
/path move backward 1
/path walk backward
/path walk forward
```

The position is the cursor between path steps. `move` changes the cursor without sending anything. `walk` sends exactly one forward or backward step and advances the cursor.

## Run and transform

```text
/path goto start
/path run
/path swap
/path zip
/path unzip 3n2e
```

`run` sends remaining forward steps. `swap` reverses step order and exchanges forward/backward commands. `zip` compresses repeated directions into speedwalk notation; `unzip` expands notation into individual steps.

`delete` and `undo` both remove the final stored step; they are provided as the matching TinTin++ editing names.

## Variables

```text
/path save forward {route}
/path save backward {return_route}
/path load {route}
/variable
```

Path save/load uses the client runtime variable system. Use braces for variable names or commands containing spaces.

## Difference from `/map run`

`/path run` replays a manually recorded path. [`/map run`](map.md) calculates a new lowest-cost route to a room or landmark while respecting room and exit flags.
