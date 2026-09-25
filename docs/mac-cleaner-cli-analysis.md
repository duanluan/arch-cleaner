# mac-cleaner-cli Reference Analysis

Reference: https://github.com/guhcostan/mac-cleaner-cli, read from local clone at `/var/tmp/arch-cleaner-mac-cleaner-cli.5L17j6/repo`.

## Useful Patterns

- Category metadata is explicit: every category has an id, display name, group, description, safety level, and optional safety note. `src/types.ts` is the source for this shape.
- Scanning and cleaning share the same item model. `src/scanners/base-scanner.ts` computes total size from scan items, and `src/commands/clean.ts` cleans only selected scan results.
- Non-interactive scan output has a summary: `src/commands/scan.ts` prints grouped results, total size, total item count, and safety legend. JSON includes total size and total item count.
- Interactive mode hides risky categories by default and only includes them with an explicit flag. This keeps normal runs conservative.
- File selection is reserved for categories where item-level review matters. `src/pickers/file-picker.ts` adds drill-down selection, directory grouping, and size bars without forcing every category into that flow.
- Deletion is centralized behind path checks. `src/utils/fs.ts` validates protected paths, handles symlinks separately, and reports failure counts by error code.
- External tools are invoked with explicit arguments, and some tool paths are discovered from known safe locations. `src/scanners/docker.ts` and `src/scanners/homebrew.ts` avoid shell interpolation and validate tool output.

## Applied To arch-cleaner

- Added target grouping in the core model so CLI, TUI, and JSON can expose the same package/system/user/developer grouping.
- Added structured `estimated_items` to scan reports so summaries no longer need to infer counts from text details.
- Added scan summaries to text output; JSON output is new in this pass and ships with a stable `*.v1` format marker.
- Kept the existing command-oriented cleanup plan, because Arch system cleanup is easier to audit when the exact `pacman`, `journalctl`, and `find` commands remain visible.
- Fixed pacman cache dry-run to use one valid `paccache` operation: `paccache -d -k N`. The previous `-d -r` combination is rejected by `paccache`.
- Pacman scan now asks `paccache` for its own dry-run summary and uses that as the cleanable estimate. The scan reuses the exact argv of the dry-run command shown in the plan (only the locale is pinned), so the estimate always describes the command the user sees.
- Rule metadata is single-source: each rule entry in `rules/mod.rs` owns its id, localized title/description, threshold summary, scan function, and commands. `--help`, `list-targets`, TUI, and JSON all render from the registry; a test keeps the README target table in sync.

## Not Applied In This Pass

- Full file picker: useful, but it requires scan reports to expose typed per-item paths for every target. That is a larger model change.
- Backup/restore: useful for user-file cleanup, but current targets focus on caches and system commands. Adding backups now would add state and failure modes unrelated to this release.
- Risky categories such as downloads, duplicate files, and large files: these need item-level review and should not be added without a dedicated selection UI.
- App uninstall and maintenance tasks: these are separate workflows, not cleanup targets for this Arch-focused tool.
