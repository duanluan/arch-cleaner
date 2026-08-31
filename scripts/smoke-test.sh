#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

cargo build >/dev/null
cargo run -- --help >/dev/null
cargo run -- list-targets >/dev/null
cargo run -- scan --targets thumbnail-cache >/dev/null
cargo run -- clean --targets pacman-cache >/dev/null

printf '%s\n' 'smoke test passed'
