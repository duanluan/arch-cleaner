# arch-cleaner

`arch-cleaner` is an interactive and scriptable cleaner for Arch Linux systems.
It is built as a Rust library plus a thin CLI so a later desktop client can reuse the same cleanup target model, scan reports, and execution plan.

The first release is intentionally conservative. It focuses on common Arch/system cleanup tasks and avoids browser profiles, container volumes, project build caches, and application-private data.

## Goals

- Default to an interactive terminal menu.
- Keep script-friendly subcommands for automation and future clients.
- Show a cleanup plan before running destructive commands.
- Require explicit confirmation for system changes.
- Use `sudo` only for targets that need system privileges.

## Rule Set

Built-in cleanup rules live in [rules/README.md](rules/README.md). The engine in `src/` keeps the CLI, TUI, JSON, and execution flow separate from the rule definitions, so new cleanup behavior can be reviewed in one place.


| Group | Target | ID | Scope | Default policy |
| --- | --- | --- | --- | --- |
| Packages | Pacman package cache | `pacman-cache` | `/var/cache/pacman/pkg` | `paccache -r -k 3`; scan uses `paccache -d -k 3` |
| Packages | Orphan packages | `orphan-packages` | `pacman -Qtdq` | Remove with `pacman -Rns` after re-querying |
| System | Systemd journal | `systemd-journal` | systemd journal files | Vacuum to 14 days and 1G |
| User | User cache | `user-cache` | `$HOME/.cache` excluding thumbnails and known AI agent cache directories | Top-level directories older than 30 days |
| Developer | AI agent caches | `ai-agent-caches` | known agent cache/log/artifact paths plus whitelisted temp-scratch prefixes | Entries older than 30 days |
| User | Temporary files | `temp-files` | `/var/tmp`, `/tmp` excluding known AI agent scratch prefixes and common runtime directories | Top-level entries older than 7 days |
| User | Thumbnail cache | `thumbnail-cache` | `$HOME/.cache/thumbnails` | Clear generated thumbnails |
| System | System crash dumps | `crash-dumps` | `/var/lib/systemd/coredump` | Remove stored coredump files |
| User | Old downloads | `old-downloads` | `$HOME/Downloads` | Remove old top-level entries (high risk; default 90 days) |
| User | Large files | `large-files` | `$HOME` | List files over the size threshold (default 500M); picker-only |
| User | Duplicate files | `duplicate-files` | `$HOME` | Same-content files at or above 1M; one copy per group is kept; picker-only |

## Build

```sh
cargo build --release
```

## Run

Start the interactive menu:

```sh
arch-cleaner
```

Start the interactive menu in English:

```sh
arch-cleaner -l en
```

The TUI accepts Ctrl+L to switch between Chinese and English. Press Tab to open the settings page, where you can edit thresholds such as pacman package versions to keep, journal age/size, AI agent cache age, temp file age, and user cache age. Use arrow keys to move and Space to toggle targets. After scanning (`s`), targets that report per-item paths (user cache, temp files, AI agent caches) open a results page: use arrow keys to move, Space to toggle individual entries, `a`/`n` to select all/none, and `c` to clean only the selected entries — each removal is an explicit `rm -rf -- <path>` command shown for confirmation before anything runs.

List supported targets:

```sh
arch-cleaner list-targets
```

List supported targets as JSON:

```sh
arch-cleaner list-targets --json
```

Scan all targets:

```sh
arch-cleaner scan
```

Scan selected targets:

```sh
arch-cleaner scan --targets pacman-cache,systemd-journal
```

Scan selected targets as JSON:

```sh
arch-cleaner scan --targets pacman-cache,systemd-journal --json
```

Show the cleanup plan without changing anything:

```sh
arch-cleaner clean --targets pacman-cache,temp-files
```

Show the cleanup plan as JSON:

```sh
arch-cleaner clean --targets pacman-cache,temp-files --json
```

Run read-only dry-run checks and return results as JSON:

```sh
arch-cleaner clean --targets pacman-cache --run-readonly-checks --json
```

Execute a cleanup plan:

```sh
arch-cleaner clean --targets pacman-cache,temp-files --apply
```

Skip the confirmation prompt for automation:

```sh
arch-cleaner clean --targets pacman-cache --apply --yes
```

Use JSON while executing from automation:

```sh
arch-cleaner clean --targets pacman-cache --apply --yes --json
```

`clean --json --apply` requires `--yes` so stdout remains a single JSON document.

## JSON Output

JSON output is available for `list-targets`, `scan`, and `clean`.

- `list-targets --json` emits `arch-cleaner.targets.v1` with target metadata, group ids, and threshold summaries.
- `scan --json` emits `arch-cleaner.scan.v1` with a top-level summary, scan reports, group ids, threshold summaries, estimated bytes, and estimated item counts when known.
- `clean --json` emits `arch-cleaner.clean.v1` with a command status summary, the planned commands, and any execution results.
- Text fields in JSON follow the selected `--lang` value.

See [docs/mac-cleaner-cli-analysis.md](docs/mac-cleaner-cli-analysis.md) for the reference-project analysis behind the grouping and scan-summary design.

## Options

```text
--lang, -l <zh|en>       UI language, default zh
--targets <ids>          Comma-separated target ids, or all
--apply                  Execute cleanup commands
--yes, -y                Skip confirmation prompts for --apply
--run-readonly-checks    In dry-run mode, run read-only commands
--json                   Print machine-readable JSON
--keep-packages <n>      Pacman package versions to keep, default 3
--journal-days <n>       Journal age vacuum threshold, default 14
--journal-size <size>    Journal size vacuum threshold, default 1G
--temp-days <n>          Temp file age threshold, default 7
--user-cache-days <n>    User cache age threshold, default 30
--ai-agent-days <n>      AI agent cache age threshold, default 30
--downloads-days <n>     Downloads age threshold, default 90
--large-file-size <size> Large file threshold, default 500M
--duplicate-min-size <size> Duplicate scanning minimum size, default 1M
```

## Install Locally

```sh
scripts/install.sh
```

This runs `cargo install --path . --locked --force --root "$HOME/.local"` from the repository root and installs the binary into `~/.local/bin`.

## Safety Notes

`scan` is read-only. Its size estimates are scoped to entries that match the current cleanup rule, including the configured age thresholds and target-specific exclusions. `clean` without `--apply` prints the commands that would run and makes no changes.

The `ai-agent-caches` target is deliberately whitelist-based. It covers old cache, log, attachment, generated artifact, and temp-scratch entries for tools such as Codex, Claude, Cursor, Windsurf, Gemini, OpenCode, Pi, and rtk command logs. That includes selected `/var/tmp` and `/tmp` paths whose names match known agent prefixes. The generic `temp-files` target excludes those prefixes, common runtime directories such as AppImage mount points, and only handles top-level old entries, so the two rules do not overlap. It does not remove configuration files, secrets, session history, plugin installs, skills, Electron `Local Storage`/`IndexedDB`, or VS Code-style workspace state.

System targets use `sudo` only when executing the apply plan. The interactive menu still prints the plan and requires typing `APPLY` before it runs cleanup commands.

## Development

```sh
cargo fmt
cargo test
cargo clippy -- -D warnings
scripts/smoke-test.sh
```

The TUI uses `crossterm` for raw key handling.
