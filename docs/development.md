# Development and Validation

This page is for people building, testing, or contributing to `mud-client` from source. Players who just want to run the client should use [Installation](installation.md) and [Getting Started](getting-started.md) instead.

## Make targets

`make` wraps common Cargo workflows. Run `make help` at any time to see this list from the source checkout.

| Target | What it does |
|---|---|
| `make run` | Runs the client with your configured connection settings. |
| `make local` | Runs the client against `localhost:3791`, the same as `cargo run -- --local`. |
| `make install` | Builds and installs the release binary with `cargo install --path . --locked`, the same install method used by the guided installers. |
| `make build` | Builds the project with `cargo build`. |
| `make check` | Runs `cargo check` for a fast compile-only check. |
| `make fmt` | Formats Rust source with `cargo fmt`. |
| `make fmt-check` | Verifies formatting without changing files (`cargo fmt --check`). |
| `make lint` | Runs Clippy with warnings denied (`cargo clippy --all-targets --all-features -- -D warnings`). |
| `make test` | Runs the automated test suite (`cargo test`). |
| `make ci` | Runs `fmt-check`, `lint`, `test`, and a whitespace-only `git diff --check`; this mirrors what CI runs. |
| `make clean` | Removes build artifacts (`cargo clean`). |

Make is optional. Every target above has an equivalent plain Cargo (or Git) command shown in parentheses, so contributors without Make installed can run the same checks directly.

## Full validation before a change

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
git diff --check
```

This is the same sequence as `make ci` and is the minimum bar for a change to be considered ready.

## GitHub Actions builds

Pushes to the `main` branch run `make test` first. Linux, macOS, and Windows
release builds start only after that test job succeeds. The workflow publishes
two workflow artifacts for each platform:

- a standalone optimized executable;
- a ZIP bundle containing the executable, example configuration files, Lua
  scripts, documentation, license, and available platform installers.

The workflow is defined in `.github/workflows/make-build.yml` and uses the
native GitHub-hosted runner for each platform, so Windows produces a native
`.exe` and macOS produces an Apple Silicon ARM64 binary.

Every successful push to `main` also receives the next patch version tag. The
first release is `v0.1.0`; later releases increment the patch component. The
workflow creates the GitHub Release automatically and attaches the three
standalone executables with generated release notes. The ZIP bundles remain
available as downloadable artifacts on the workflow run.

## Runtime diagnostics

### Responsiveness and thread ownership

Terminal input is read on its own OS thread. A separate terminal output thread
owns stdout, Ratatui's terminal diff, and terminal restoration. The application
renders complete screens into memory and publishes only the latest screen; slow
terminal writes cannot build an unbounded frame queue or block command editing.
Intermediate screens may be skipped, but MUD output remains in scrollback.

Networking runs on Tokio worker threads with independently polled read and write
halves. Bounded queues provide backpressure without making incoming output and
outgoing commands wait for each other. Keyboard and network events have separate
queues, and the app selects one event at a time so sustained output cannot trap it
in a queue-draining loop. Screen updates coalesce at approximately 60 FPS;
gameplay ticks skip missed ticks instead of replaying a backlog.
Connection controls use a separate latest-request channel so disconnect,
reconnect, and quit can interrupt a stalled socket write even when commands are
queued. Rapid consecutive controls coalesce to the most recent request.

State updates, scripts, and widget composition remain ordered on the application
thread to preserve trigger ordering and avoid shared mutable state. Lua hooks
retain their instruction budget. These operations can still take time, as can
explicit configuration/map loading. This architecture does not guarantee a
maximum latency for arbitrary scripts or a terminal that has stopped responding.

If the outgoing command queue fills, the app reports that a command was not sent
and remains responsive. Retry it after the connection catches up. A rejected
movement command does not update the local map or recorded path. Commands already
accepted into the queue retain their order. Use a release build (`make install`)
when assessing interactive performance.

These in-client commands and settings are useful while developing or debugging behavior, not just for players:

- `/msdp`: show the current stored MSDP values.
- `/reload`: reload configuration and scripts, and display validation errors in the output pane.
- `/reconnect`: request a reconnect through the network command channel.
- `logging.level`: tracing filter, for example `mud_client=debug`. See [Configuration Reference](configuration.md#logging).
- `logging.raw_protocol`: reserved low-level protocol diagnostics flag; keep disabled unless actively troubleshooting the network layer.

## See also

- [Installation](installation.md) for setting up prerequisites and building the client for normal play.
- [Configuration Reference](configuration.md) for every `config.toml` option, including `[logging]`.
- [Troubleshooting](troubleshooting.md) for common player-facing setup and runtime issues.
