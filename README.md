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
| Packages | Pacman package cache | `pacman-cache` | `/var/cache/pacman/pkg` | `paccache -r -k 3` plus `paccache -ruk0` for caches of uninstalled packages; scan sums both dry-runs |
| Packages | Orphan packages | `orphan-packages` | `pacman -Qtdq` | Remove with `pacman -Rns` after re-querying |
| Packages | Large packages | `large-packages` | `pacman -Qi` | List the top 30 installed packages by size; picker-only; picked entries are uninstalled together via `pacman -Rns` (package names fed over stdin) |
| Developer | Docker unused images | `docker-images` | docker images without containers | Official `docker image prune -a -f`; scan reads reclaimable bytes from `docker system df`; stopped containers and volumes untouched |
| System | Systemd journal | `systemd-journal` | systemd journal files | Vacuum to 14 days and 1G |
| User | User cache | `user-cache` | `$HOME/.cache` excluding thumbnails, JetBrains, and known AI agent cache directories | Top-level directories older than 30 days; caches of running apps (Edge/Chrome/Firefox/QQ) are skipped |
| Developer | AI agent caches | `ai-agent-caches` | known agent cache/log/artifact paths plus whitelisted temp-scratch prefixes | Entries older than 30 days |
| Developer | pnpm store | `pnpm-store` | pnpm content-addressable store | Official `pnpm store prune`; unreferenced packages only |
| Developer | Conda package cache | `conda-cache` | conda `pkgs` cache (miniforge/miniconda/anaconda) | Official `conda clean --all -y` |
| Developer | Gradle cache | `gradle-cache` | `$HOME/.gradle/caches` | Picker-only; no candidate while a Gradle process runs |
| Developer | JetBrains stale version data | `jetbrains-stale` | `$HOME/.cache/JetBrains`, `$HOME/.local/share/JetBrains` | Directories whose version no longer matches an installed IDE; picker-only; settings directories untouched |
| User | Temporary files | `temp-files` | `/var/tmp`, `/tmp` excluding known AI agent scratch prefixes and common runtime directories | Top-level entries older than 7 days |
| User | Thumbnail cache | `thumbnail-cache` | `$HOME/.cache/thumbnails` | Clear generated thumbnails |
| User | Trash | `trash` | `$HOME/.local/share/Trash` | Empty the trash |
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

The TUI accepts Ctrl+L to switch between Chinese and English. Press Tab to open the settings page, where you can edit thresholds such as pacman package versions to keep, journal age/size, AI agent cache age, temp file age, and user cache age. Use arrow keys to move and Space to toggle targets.

Scanning (`s`) runs on a background thread with a live progress line, so long scans (large files, duplicates) no longer freeze the interface; press Esc to stop after the current target finishes. When the scan completes, a grouped results page opens: each target is a section header showing its risk level, item count, total size, and how many entries are picked. Entries of low/medium-risk targets (caches) are picked by default, so the common flow is scan → review → `c`; high-risk groups (old downloads, large files, duplicates) start unpicked and are picked one by one or with `t`. Each cache entry carries a plain-language note (what this directory is and what happens after removal) shown on its row; the full path of the focused entry is shown in the detail line at the bottom. Move with arrows or `j`/`k`, page with PgUp/PgDn, toggle one entry with Space, a whole group with `t` (or Space on the header), and use `a`/`n` for all/none — `a` deliberately skips high-risk groups. The footer always shows the picked count and total size. Press `c` to clean only the picked entries — each removal is an explicit `rm -rf -- <path>` command shown for confirmation before anything runs.

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

System targets use `sudo` only when executing the apply plan. The interactive menu still prints the plan (with a summary of what will be removed) and requires typing `y` to confirm before it runs cleanup commands.

Caches of running applications are protected in two places. The `user-cache` rule skips the cache directories of running browsers and QQ both in the scan estimate and in the apply script (a `pgrep` guard re-checks at execution time); the `jetbrains-stale` and `gradle-cache` rules never offer entries while the corresponding IDE or a Gradle process is running. Developer tool stores are cleaned only through their official commands (`pnpm store prune`, `conda clean --all`), never by removing the store directories directly.

## Development

```sh
cargo fmt
cargo test
cargo clippy -- -D warnings
scripts/smoke-test.sh
```

The TUI uses `crossterm` for raw key handling.
