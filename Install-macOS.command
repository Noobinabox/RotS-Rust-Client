#!/usr/bin/env bash
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
bash "$source_dir/install.sh" "$@"
result=$?
read -r -p 'Press Enter to close this installer.' ignored
exit "$result"
