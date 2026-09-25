#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

cargo build >/dev/null
cargo run --quiet -- -l en --help >/dev/null
cargo run --quiet -- -l en list-targets >/dev/null
cargo run --quiet -- -l zh scan --targets thumbnail-cache >/dev/null
cargo run --quiet -- -l en scan --targets systemd-journal >/dev/null
cargo run --quiet -- -l en scan --targets systemd-journal --json | python3 -m json.tool >/dev/null
cargo run --quiet -- -l en clean --targets pacman-cache >/dev/null
cargo run --quiet -- -l en clean --targets ai-agent-caches --ai-agent-days 14 >/dev/null
cargo run --quiet -- -l en list-targets --json | python3 -m json.tool >/dev/null
cargo run --quiet -- -l zh scan --targets thumbnail-cache --json | python3 -m json.tool >/dev/null
cargo run --quiet -- -l en scan --targets ai-agent-caches --ai-agent-days 14 --json | python3 -m json.tool >/dev/null
cargo run --quiet -- -l en clean --targets pacman-cache --run-readonly-checks --json | python3 -m json.tool >/dev/null
cargo run --quiet -- -l en clean --targets pacman-cache --json | python3 -m json.tool >/dev/null

printf '%s\n' 'smoke test passed'
