# Getting Started

Start with [Installation for Linux, macOS, and Windows](installation.md) if this is your first setup. It covers Rust/compiler installation, downloading the client, platform-specific configuration paths, and copying bundled Lua scripts.

## Requirements

- Stable Rust toolchain.
- A terminal with Unicode support. True color is optional but recommended.
- A RoTS-compatible MUD endpoint. The checked-in default connects to `rotsmud.org:3791`.

## Run From Source

```sh
cargo run
```

Install the release binary into Cargo's user binary directory:

```sh
cargo +stable install --path . --locked
```

This installs `mud-client`; it does not copy or overwrite configuration or scripts. `make install` is also available when Make is installed.

Force the local test endpoint even when an installed config points elsewhere:

```sh
cargo run -- --local
```

Show CLI help without entering the terminal UI:

```sh
cargo run -- --help
```

Common make targets:

```sh
make help
make local
make install
make ci
```

See [Development and Validation](development.md) for every `make` target, its plain Cargo equivalent, and the full pre-change validation sequence.

## Configuration File

The repository includes `config.toml` as a complete default example. It is not automatically loaded from the current directory. On Linux/WSL the default location is:

```text
~/.config/mud-client/config.toml
```

If no config file exists, built-in defaults are used. The default repository config connects to `rotsmud.org` on port `3791`.

See [platform configuration paths and copy commands](installation.md#first-launch-and-configuration) for macOS, Windows, and Linux setup. Copy the Lua scripts too when using the bundled example configuration.

Reload the active config without restarting:

```text
/reload
```

Validation errors are printed in the output pane and the existing runtime session continues.

## First Login

Type login text directly into the command input. Local commands start with `/`; everything else is sent to the MUD after alias, variable, semicolon, and echo processing.

The default connection config does not store credentials:

```toml
[connection]
host = "rotsmud.org"
port = 3791
username = ""
password = ""
```

## Next Steps

- Configure connection, panels, colors, MSDP mappings, and scripts in [Configuration Reference](configuration.md).
- Learn local slash commands in [Command Reference](commands.md).
- Learn mapper behavior in [Mapper](mapping.md).
- Add aliases, triggers, events, and Lua hooks with [Scripting](scripting.md).
