#!/usr/bin/env bash
# Run from an extracted checkout; never pipe this script from the internet.
set -euo pipefail

source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
check_only=false
skip_prerequisites=false
for argument in "$@"; do
    case "$argument" in
        --check) check_only=true ;;
        --skip-prerequisites) skip_prerequisites=true ;;
        --help) printf '%s\n' 'Usage: bash install.sh [--check] [--skip-prerequisites]' 'Run as your normal user. Existing configuration and scripts are preserved.'; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$argument" >&2; exit 1 ;;
    esac
done

platform=$(uname -s)
case "$platform" in
    Linux)
        case "${XDG_CONFIG_HOME:-}" in
            /*) config_dir="$XDG_CONFIG_HOME/mud-client" ;;
            *) config_dir="$HOME/.config/mud-client" ;;
        esac ;;
    Darwin) config_dir="$HOME/Library/Application Support/org.mud-client.mud-client" ;;
    *) printf 'Unsupported platform: %s. On Windows use Install-Windows.cmd.\n' "$platform" >&2; exit 1 ;;
esac

cargo_dir=${CARGO_HOME:-$HOME/.cargo}
case "$cargo_dir" in
    /*) ;;
    *) printf '%s\n' 'CARGO_HOME must be an absolute path for this installer.' >&2; exit 1 ;;
esac
export PATH="$cargo_dir/bin:$PATH"
printf 'mud-client source installer\nSource: %s\nBinary: %s/bin/mud-client\nConfig: %s\n' "$source_dir" "$cargo_dir" "$config_dir"
printf '%s\n' 'Licensed under MIT; see LICENSE for permissions, conditions and warranty disclaimer.'

for required in Cargo.toml Cargo.lock LICENSE install/default-config.toml; do
    if [[ ! -f "$source_dir/$required" ]]; then
        printf 'Missing %s. Download and extract the complete repository first.\n' "$required" >&2
        exit 1
    fi
done

if $check_only; then
    missing=false
    for tool in cc make curl rustup; do
        if command -v "$tool" >/dev/null 2>&1; then
            printf 'Found: %s\n' "$tool"
        else
            printf 'Missing: %s\n' "$tool"
            missing=true
        fi
    done
    if ! command -v rustup >/dev/null 2>&1 || ! rustup run stable cargo --version; then
        missing=true
    fi
    if $missing; then exit 1; fi
    exit 0
fi

if [[ $(id -u) == 0 ]]; then
    printf '%s\n' 'Run this installer as your normal user, not root/sudo. Only prerequisite installation uses sudo.' >&2
    exit 1
fi
printf '%s\n' 'This builds and installs mud-client for your user. A previous binary may be upgraded.' \
    'Missing compiler/Rust tools may be downloaded; system tools may require sudo.' \
    'Existing config, scripts, profiles, maps and saved settings will not be overwritten.' \
    'No connection to the MUD will be made. Building can take several minutes.'
read -r -p 'Continue? [y/N] ' answer
case "$answer" in y|Y|yes|YES) ;; *) printf '%s\n' 'Cancelled.'; exit 1 ;; esac

if ! $skip_prerequisites; then
    if [[ "$platform" == Darwin ]]; then
        if ! xcode-select -p >/dev/null 2>&1 || ! xcrun --find clang >/dev/null 2>&1; then
            xcode-select --install
            printf '%s\n' 'Finish the Apple developer-tools installation, then run this installer again.'
            exit 1
        fi
    elif ! command -v cc >/dev/null 2>&1 || ! command -v make >/dev/null 2>&1 || ! command -v curl >/dev/null 2>&1; then
        if ! command -v sudo >/dev/null 2>&1; then
            printf '%s\n' 'sudo is unavailable. Ask your administrator to install a C compiler, make, curl and CA certificates.' >&2
            exit 1
        fi
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get update
            sudo apt-get install build-essential curl ca-certificates
        elif command -v dnf >/dev/null 2>&1; then
            sudo dnf install gcc gcc-c++ make curl ca-certificates
        elif command -v pacman >/dev/null 2>&1; then
            sudo pacman -S --needed base-devel curl ca-certificates
        else
            printf '%s\n' 'Unsupported package manager. Install a C compiler, make, curl and CA certificates, then rerun.' >&2
            exit 1
        fi
    fi
    if ! command -v rustup >/dev/null 2>&1; then
        rustup_download=$(mktemp -d)
        trap 'rm -f -- "$rustup_download/rustup-init.sh"; rmdir -- "$rustup_download"' EXIT
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$rustup_download/rustup-init.sh"
        sh "$rustup_download/rustup-init.sh" -y --profile minimal --default-toolchain stable
    fi
    rustup toolchain install stable --profile minimal
fi

for tool in cc make rustup; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'Required tool missing: %s. Rerun without --skip-prerequisites or see docs/installation.md.\n' "$tool" >&2
        exit 1
    fi
done
rustup run stable cargo install --path "$source_dir" --locked --root "$cargo_dir"
"$cargo_dir/bin/mud-client" --help

mkdir -p "$config_dir/scripts"
copy_missing() (
    local source_file=$1 destination=$2
    if [[ -e "$destination" || -L "$destination" ]]; then
        printf 'Preserved: %s\n' "$destination"
    else
        local staged_file
        staged_file=$(mktemp "$destination.install.XXXXXX")
        trap 'rm -f -- "$staged_file"' EXIT
        cp -- "$source_file" "$staged_file"
        # Same-filesystem hard-link publication is atomic and refuses overwrite.
        if ! ln -- "$staged_file" "$destination"; then
            if [[ -e "$destination" || -L "$destination" ]]; then
                printf 'Preserved concurrent creation: %s\n' "$destination"
                return
            fi
            printf 'Unable to install: %s\n' "$destination" >&2
            return 1
        fi
        printf 'Installed: %s\n' "$destination"
    fi
)
copy_missing "$source_dir/install/default-config.toml" "$config_dir/config.toml"
copy_missing "$source_dir/LICENSE" "$config_dir/LICENSE"
for script in "$source_dir"/scripts/*.lua; do
    copy_missing "$script" "$config_dir/scripts/$(basename "$script")"
done
printf '\nInstalled. Open a new terminal and run: mud-client\nOr run: "%s/bin/mud-client"\n' "$cargo_dir"
printf '%s\n' 'Lua examples were copied but are disabled in new configurations. See docs/targeting.md to opt in.'
