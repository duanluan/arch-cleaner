#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if ! command -v cargo >/dev/null 2>&1; then
  printf '%s\n' 'cargo is required to install arch-cleaner.' >&2
  exit 1
fi

cargo install --path . --locked
