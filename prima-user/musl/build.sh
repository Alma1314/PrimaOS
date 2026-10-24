#!/bin/sh
# Keep the existing xtask entry; Python 3.11+ supplies TOML parsing and locking.
set -eu
exec python3 "$(dirname -- "$0")/build.py" "$@"
