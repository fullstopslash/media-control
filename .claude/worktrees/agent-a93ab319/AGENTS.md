# Agent Instructions

## System Environment

This is a Nix-based system. The project should have a `flake.nix` for development dependencies and build tooling. Use `nix develop` to enter the development shell. If no flake exists, create one appropriate for the project's language/framework.

## Version Control

Use `jj` for all git operations. Never use raw git commands.

## Version Management

Versions are bumped automatically. Use the global justfile recipes:

- `just -g bump` — bump patch version (auto-detects project type)
- `just -g bump minor` — bump minor version
- `just -g bump major` — bump major version
- `just -g commit "message"` — auto-bumps patch, then commits via jj

Supported formats: Cargo.toml, pyproject.toml, package.json, setup.py, flake.nix.
Supports semver (X.Y.Z) and datever (YYYY.MM.DD[.N]). Projects without version files are handled silently.

## Task Tracking

Use beads for task management. Initialize with `bd init` if `.beads/` doesn't exist.

## Documentation

Keep `readme.md` up to date with description, installation, and usage. Be concise — cover all known usage without padding. Update it when features, CLI interfaces, or setup steps change.

## Automation

Use `just` for project automation. Each project should have a local `justfile` with recipes for common tasks appropriate for the language/framework.

### Required Recipes

Every project justfile MUST have these recipes:

- **`just lint`** — Run linters and formatters. Always run before committing.
- **`just build`** — Compile/bundle the project. Should produce a usable artifact.
- **`just install`** — Install the built artifact to the system (e.g. `~/.local/bin`).

### Recommended Recipes

Add these when applicable:

- **`just test`** — Run the test suite.
- **`just check`** — Run `lint` + `test` together (quality gate).
- **`just clean`** — Remove build artifacts and caches.
- **`just dev`** — Start development mode (watch, serve, REPL, etc.).
- **`just fmt`** — Auto-format code (if separate from lint).
- **`just release`** — Build optimized + tag version.

### Committing

To commit, use the global recipe: `just -g commit "message"` — this describes, bookmarks, and pushes via jj in one step.

## Project Management

Keep `todo.md` in the repo root up to date — the-desk reads it for project tracking.
