# Install mud-client

These instructions build the client from source. You do not need to know Rust, but the first build downloads dependencies and can take several minutes. Internet access, Git, the latest stable Rust toolchain, and a C compiler/linker are required. Lua is bundled: no separate Lua installation is needed.

Choose your computer: [Linux](#linux), [macOS](#macos), or [Windows](#windows). Run commands one block at a time; stop if a command reports an error. Do not run Cargo or the client as administrator/root.

## Linux

Open your terminal. On Ubuntu, Debian, or Linux Mint:

```sh
sudo apt update
sudo apt install build-essential git curl ca-certificates
```

On Fedora:

```sh
sudo dnf install gcc gcc-c++ make git curl ca-certificates
```

On Arch Linux:

```sh
sudo pacman -Syu --needed base-devel git curl ca-certificates
```

Install Rust using the [official Rust installer](https://rust-lang.org/tools/install/). This downloads and executes the official rustup script; review the installer page first if you prefer to inspect it before running:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the standard installation, close and reopen your terminal, then continue to [download and build](#download-and-build).

## macOS

Open **Terminal** from Applications → Utilities. Install Apple's command-line developer tools (includes Git, a compiler, and a linker):

```sh
xcode-select --install
```

Complete the installer dialog before continuing. If the tools are already installed, you can skip this step. You do not need the full Xcode application or Homebrew. These prerequisites follow the [Rust installation guide](https://doc.rust-lang.org/book/ch01-01-installation.html).

Install Rust using the official installer:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept the standard installation and reopen Terminal. Continue to [download and build](#download-and-build). Use a native terminal/toolchain for your Mac's architecture (Apple Silicon or Intel).

## Windows

### Option A: WSL with Ubuntu

This runs the Linux client inside Windows. Open **PowerShell as administrator** for this one system-setup step:

```powershell
wsl --install -d Ubuntu
```

Restart if prompted. Open **Ubuntu** from the Start menu and create your Linux username/password. Password typing is invisible; this is normal. The command requires Windows 11 or Windows 10 version 2004/build 19041 or newer; see [Microsoft's WSL installation guide](https://learn.microsoft.com/en-us/windows/wsl/install) for requirements and installation errors.

Now follow the [Linux Ubuntu steps](#linux) **inside Ubuntu**, including Rust installation and download/build. Keep your checkout in your Linux home directory, not under `/mnt/c`. Install Windows Terminal from the Microsoft Store if needed, then use its Ubuntu profile for playing. On later launches, open that profile and run `mud-client`.

WSL has its own Rust installation and config files; a Windows Rust installation does not replace the Ubuntu setup.

### Option B: Native Windows

1. Install [Git for Windows](https://git-scm.com/download/win), allowing Git on your command-line PATH.
2. Follow the [official Windows Rust setup](https://rust-lang.github.io/rustup/installation/windows-msvc.html). Install Visual Studio Build Tools with **Desktop development with C++**, the MSVC compiler tools, and a Windows SDK. Rust's Windows installer can guide you through prerequisites. Use the default MSVC Rust toolchain.
3. Install Rust through [rustup-init.exe](https://rust-lang.org/tools/install/).
4. Close and reopen **PowerShell in Windows Terminal** as a normal user. Continue below; the Git/Cargo commands work in PowerShell too. If the compiler cannot find `cl.exe` or `link.exe`, try a Developer PowerShell supplied by Visual Studio Build Tools.

Native Windows and macOS instructions have not been end-to-end tested in this Linux development environment. Native Windows also has keyboard-reporting limitations: dedicated numpad identity and paste behavior may differ. See [macro limitations](commands/macro.md) and [input help](commands/input.md).

## Download and build

Check that the tools are available:

```sh
git --version
rustc --version
cargo --version
rustup update stable
```

From a folder where you want to keep the source (your home folder is fine):

```sh
git clone https://github.com/Noobinabox/RotS-Rust-Client.git
cd RotS-Rust-Client
cargo +stable install --path . --locked
mud-client --help
mud-client
```

If GitHub reports that the repository is unavailable, ask the maintainer for access or the current download URL. The HTTPS URL does not require setting up SSH keys.

`cargo install` builds an optimized executable and puts it in Cargo's user binary folder. It does **not** install configuration or Lua scripts. `make install` is an equivalent convenience on computers with Make; Make is not required for the commands above.

After installation, run `mud-client` from any terminal folder. Keep the source checkout for updates. To try the source without installing, run `cargo +stable run --locked --release` from the checkout.

## First launch and configuration

The client connects to `rotsmud.org:3791` by default. A missing config file is fine for a first connection. Enter your MUD login in the command input; type `/help` for commands and `/quit` to exit. Use your normal terminal, not an editor's output/debug console.

The repository's `config.toml` is an example, **not automatically loaded from your current folder**. Optional configuration belongs here:

| Platform | Configuration file |
|---|---|
| Linux / Ubuntu under WSL | `~/.config/mud-client/config.toml` (or `$XDG_CONFIG_HOME/mud-client/config.toml` when set to an absolute path) |
| macOS | `~/Library/Application Support/org.mud-client.mud-client/config.toml` |
| Native Windows | `%APPDATA%\mud-client\mud-client\config\config.toml` |

For the bundled panels, aliases, and targeting, copy the example config **and** the `scripts` folder. The sample configuration enables automation, including an example arrival hook that sends a target command; review its aliases/triggers before playing. Do not overwrite an existing setup: back it up and merge settings instead.

Fresh Linux/WSL setup, from the repository directory:

```sh
case "${XDG_CONFIG_HOME:-}" in
  /*) client_config_dir="$XDG_CONFIG_HOME/mud-client" ;;
  *) client_config_dir="$HOME/.config/mud-client" ;;
esac
mkdir -p "$client_config_dir/scripts"
cp -i config.toml "$client_config_dir/config.toml"
cp -i scripts/*.lua "$client_config_dir/scripts/"
```

Fresh macOS setup:

```sh
client_config_dir="$HOME/Library/Application Support/org.mud-client.mud-client"
mkdir -p "$client_config_dir/scripts"
cp -i config.toml "$client_config_dir/config.toml"
cp -i scripts/*.lua "$client_config_dir/scripts/"
```

Fresh native Windows setup, in PowerShell:

```powershell
$clientConfigDir = Join-Path $env:APPDATA 'mud-client\mud-client\config'
New-Item -ItemType Directory -Force -Path (Join-Path $clientConfigDir 'scripts')
Copy-Item .\config.toml (Join-Path $clientConfigDir 'config.toml') -Confirm
Copy-Item .\scripts\*.lua (Join-Path $clientConfigDir 'scripts') -Confirm
```

Restart after first creating the config. For subsequent edits, use `/reload` in the client. See [numbered targeting](targeting.md) for the required server colors and shortcuts, and [configuration](configuration.md) for customization. Use `/save` for runtime aliases/variables; it does not overwrite the main TOML file.

## Updating

Exit the client, return to the source checkout, and run:

```sh
git pull --ff-only
rustup update stable
cargo +stable install --path . --locked
```

If Git reports local changes or diverged history, stop and preserve your edits; do not reset them to force an update. Relaunch `mud-client` after a successful install. Installed config and scripts are separate copies: review and merge relevant upstream changes, including updated Lua scripts, then `/reload`. Keep backups of your config directory, including runtime sidecars, character profiles and map data.

## Common setup problems

- **`cargo` or `mud-client` not found:** reopen your terminal. Linux/macOS executables normally live in `~/.cargo/bin`; native Windows uses `%USERPROFILE%\.cargo\bin`. Ensure the appropriate folder is on PATH. On Linux/macOS, `. "$HOME/.cargo/env"` loads rustup's environment in the current shell.
- **Compiler/linker missing:** finish the C/C++ prerequisites for your platform, then retry installation. Vendored Lua still needs a C compiler.
- **Rust version or dependency errors:** run `rustup update stable` and use the `cargo +stable` commands above; old distribution-packaged Rust may be too old.
- **Missing Lua script:** copy the scripts alongside the active config, not just into the checkout. Relative script paths resolve beside the config file.
- **Small layout or missing panels:** maximize the terminal or reduce its font size. Layout depends on character columns/rows; a larger window exposes more panels. Use a Unicode-capable monospace font; no special icon font is required.
- **Connection fails:** check internet access and whether your firewall permits outbound TCP to `rotsmud.org:3791`. Normal use does not require opening an inbound port or forwarding your router.
- **Keypad/paste behaves differently:** support depends on the terminal and input backend; try ordinary commands first and consult [input help](commands/input.md).

For gameplay configuration and further help, continue with [Getting Started](getting-started.md) and [Troubleshooting](troubleshooting.md).
