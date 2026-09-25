# Cleanup Rules

This directory holds the built-in cleanup rules for `arch-cleaner`.

## Layout

- `mod.rs` wires the built-in rules together.
- Each rule is implemented here instead of inside `src/`, so rule changes stay separate from the engine.

## Contributing a rule

- Add a new rule here, or split an existing rule into its own file if it grows.
- Keep the rule entry self-contained: id, localized title/description, threshold summary, scan function, and dry-run/apply commands all live in the rule's own definition. `--help`, `list-targets`, TUI, and JSON render from the registry, so no other source file needs editing.
- Use explicit command arguments and validate any command output you consume.
- Keep the cleanup scope narrow and avoid broad shell expansion; feed lists to commands via stdin when the tool supports it.
- Update tests and docs when the rule changes visible output or thresholds; `readme_lists_every_target` fails until the README target table includes the new id.

## Notes

- `src/` owns the CLI, TUI, JSON, and execution engine.
- `rules/` owns what can be cleaned and how it is described.
- This directory is the place to review when you want to add or modify cleanup behavior.
