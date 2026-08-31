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

## Cleanup Targets

| Target | ID | Scope | Default policy |
| --- | --- | --- | --- |
| Pacman package cache | `pacman-cache` | `/var/cache/pacman/pkg` | `paccache -r -k 3` |
| Orphan packages | `orphan-packages` | `pacman -Qtdq` | Remove with `pacman -Rns` after re-querying |
| Systemd journal | `systemd-journal` | systemd journal files | Vacuum to 14 days and 1G |
| User cache | `user-cache` | `$HOME/.cache` | Top-level directories older than 30 days |
| Temporary files | `temp-files` | `/var/tmp`, `/tmp` | Entries older than 7 days |
| Thumbnail cache | `thumbnail-cache` | `$HOME/.cache/thumbnails` | Clear generated thumbnails |
| System crash dumps | `crash-dumps` | `/var/lib/systemd/coredump` | Remove stored coredump files |

## Build

```sh
cargo build --release
```

## Run

Start the interactive menu:

```sh
arch-cleaner
```

List supported targets:

```sh
arch-cleaner list-targets
```

Scan all targets:

```sh
arch-cleaner scan
```

Scan selected targets:

```sh
arch-cleaner scan --targets pacman-cache,systemd-journal
```

Show the cleanup plan without changing anything:

```sh
arch-cleaner clean --targets pacman-cache,temp-files
```

Execute a cleanup plan:

```sh
arch-cleaner clean --targets pacman-cache,temp-files --apply
```

Skip the confirmation prompt for automation:

```sh
arch-cleaner clean --targets pacman-cache --apply --yes
```

## Options

```text
--targets <ids>          Comma-separated target ids, or all
--apply                  Execute cleanup commands
--yes, -y                Skip confirmation prompts for --apply
--run-readonly-checks    In dry-run mode, run read-only commands
--keep-packages <n>      Pacman package versions to keep, default 3
--journal-days <n>       Journal age vacuum threshold, default 14
--journal-size <size>    Journal size vacuum threshold, default 1G
--temp-days <n>          Temp file age threshold, default 7
--user-cache-days <n>    User cache age threshold, default 30
```

## Install Locally

```sh
scripts/install.sh
```

This runs `cargo install --path . --locked` from the repository root.

## Safety Notes

`scan` is read-only. `clean` without `--apply` prints the commands that would run and makes no changes.

System targets use `sudo` only when executing the apply plan. The interactive menu still prints the plan and requires typing `APPLY` before it runs cleanup commands.

## Development

```sh
cargo fmt
cargo test
scripts/smoke-test.sh
```

The project currently has no third-party Rust dependencies.
