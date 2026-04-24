# Agent Instructions

## Version Management

Versions are bumped automatically. Use the global justfile recipes:

- `just -g bump` — bump patch version (auto-detects project type)
- `just -g bump minor` — bump minor version
- `just -g bump major` — bump major version
- `just -g commit "message"` — auto-bumps patch, then commits via jj

Supported formats: Cargo.toml, pyproject.toml, package.json, setup.py, flake.nix.
Supports semver (X.Y.Z) and datever (YYYY.MM.DD[.N]). Projects without version files are handled silently.

## Code Intelligence (narsil-mcp + forgemax)

**narsil-mcp** is available as an MCP server providing tree-sitter-based symbol lookup, call graphs, git integration, and security scanning across repos.

Key usage rules:
- Always start a session with `list_repos` or `get_index_status` to confirm which repos are indexed
- Parameter names: `repo` (not `repo_path`), `symbol` (not `symbol_name`), `path` (not `file_path`)
- Server is launched with `--preset balanced` (51 tools) — omit `--preset full` unless you need the extra tools

**forgemax** collapses narsil-mcp (and any other configured backends) into two tools: `search` and `execute`. Use it when you want a single MCP entry point with constant ~1,100 token context cost. Config lives at `~/.config/forgemax/forge.toml`.

## Boundary contract: daemon ↔ workflow

`media-control-daemon` cannot import workflow / Jellyfin code from `media-control-lib`. The daemon's allowed surface is the substrate (`commands::shared`, `hyprland`, `window`, `config`, `error`, `test_helpers`) plus `commands::window`. Importing from `commands::workflow` or `jellyfin` is forbidden, including via the `commands::*` shim re-exports.

Enforced by two mechanisms (see `memory-bank/standards/decision-index.md` → ADR-001):

1. **Cargo feature `cli`** on `media-control-lib` (non-default). The CLI activates it; the daemon doesn't. `cargo build -p media-control-daemon` strips workflow code and `reqwest` from the daemon's lib build.
2. **`crates/media-control-daemon/tests/boundary.rs`** — a grep test that scans daemon source for forbidden import patterns. Runs as part of `cargo test --workspace`. **This is what catches the cargo workspace feature-unification escape**: under `cargo build --workspace`, the CLI's `cli` activation propagates to the daemon's lib build via unification, so the structural feature alone wouldn't catch a forbidden import in workspace builds.

**If the boundary test fires:** don't try to make the import work. Either (a) the import was a mistake — remove it, or (b) you genuinely need a daemon → workflow bridge, in which case it's a scope question for a new intent, not a fix. If you do need to update the contract, edit `FORBIDDEN_PATTERNS` in `tests/boundary.rs` AND document the justification in the PR description.
