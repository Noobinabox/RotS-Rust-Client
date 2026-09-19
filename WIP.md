# WIP.md

## Active Slice: 48. Repeat Command Prefix

### Implementation Tasks

- [x] Add brace-aware command splitting so semicolons inside repeat bodies stay grouped.
- [x] Parse `#<count> {command}` repeat prefixes.
- [x] Support multiple commands inside the braced repeat body.
- [x] Ensure commands after the closing brace run once unless they have their own repeat prefix.
- [x] Route repeated commands through aliases, local commands, movement, echo, and network sending.
- [x] Update in-client help and markdown documentation.
- [x] Add regression coverage.
- [x] Run validation and inline review.

### Validation Checklist

- [x] `cargo test repeat_command --quiet`
- [x] `cargo test app::local_commands::tests --quiet`
- [x] `cargo test --quiet`
- [x] `cargo fmt --check`
- [x] `git diff --check`
- [x] Inline Thranduil/Magus/Sauron review pass.

Status: `Done`
