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
