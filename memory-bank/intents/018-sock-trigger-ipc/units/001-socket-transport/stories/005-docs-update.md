---
id: 005-docs-update
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 030-socket-transport
implemented: true
---

# Story: 005-docs-update

## User Story

**As a** new contributor reading the project docs (or future-me 6 months from now)
**I want** the project's CLAUDE.md, readme.md, and the daemon module docstring to accurately describe the IPC transport, the wire format reservation, and the `media-control kick` CLI subcommand
**So that** nobody has to grep daemon source to discover how the trigger transport works (the original "bifurcation is misleading" motivator from the intent), and the FR-9 wire format reservation is documented before users start scripting against it

## Acceptance Criteria

- [ ] **Given** I read `CLAUDE.md` (project) under "Architecture" or equivalent section, **When** I look for the trigger IPC mechanism, **Then** I find a clear description of the `SOCK_DGRAM` socket at `$XDG_RUNTIME_DIR/media-control-daemon.sock`, the `media-control kick` subcommand, and the FR-9 wire format reservation
- [ ] **Given** I read `readme.md`, **When** I look for usage examples, **Then** `media-control kick` appears in the subcommand list with a brief description ("send a re-evaluate trigger to the daemon")
- [ ] **Given** I read the daemon's module docstring (top of `crates/media-control-daemon/src/main.rs` or equivalent), **When** I look for the IPC description, **Then** it describes the new socket transport and explicitly notes that the FIFO at `media-avoider-trigger.fifo` is removed (with the FR-8 cleanup behavior on startup)
- [ ] **Given** I `grep -ri 'media-avoider-trigger.fifo' crates/ readme.md CLAUDE.md`, **When** I look at the results, **Then** the only matches are the FR-8 cleanup path string in `crates/media-control-daemon/src/main.rs` and historical references in `memory-bank/` (intent files) — no live documentation references the FIFO as the active transport
- [ ] **Given** I `grep -ri 'systemd.user.sockets.media-control' CLAUDE.md readme.md`, **When** I look at the results, **Then** there are no matches (the dead `.socket` unit is removed in unit 2 and any docs that referenced it are updated)
- [ ] **Given** the FR-9 wire format is documented somewhere user-facing, **When** I search the docs for "0-byte" or "version byte", **Then** I find the contract: 0-byte = canonical kick (forever); non-empty = reserved with first byte as version (currently ignored with debug log)

## Technical Notes

- Files in scope:
  - `CLAUDE.md` (project root) — the "Architecture" / "Key Patterns" / "Configuration" sections; replace any FIFO references with socket references; add a brief paragraph or table on the wire format reservation.
  - `readme.md` — subcommand list (or equivalent usage section); add `kick`. Possibly a note on the daemon's IPC surface if the readme covers daemon at all.
  - Daemon module docstring (`//!` comment block at top of `crates/media-control-daemon/src/main.rs`) — describe the trigger socket, the dgram_listener, the version-byte ignore, and the FR-8 cleanup.
  - The CLI's `Kick` variant doc comment in `crates/media-control/src/main.rs` — short user-facing description.
- The intent draft at `intents/sock-trigger-ipc.md` and the formal `memory-bank/intents/018-sock-trigger-ipc/` artifacts are *historical* — they don't need updating to the past tense (the inception-log captures the rollout).
- Global `~/.claude/CLAUDE.md` does not need updating — it's user-level guidance, not project docs. Project `CLAUDE.md` does (per global guidance: "Don't repeat global guidance in per-project AGENTS.md — it just costs tokens twice").
- Wire format documentation tip: a small table is the clearest format. Mirror the table at the bottom of `memory-bank/intents/018-sock-trigger-ipc/system-context.md` (Wire Protocol Reservation section). One source of truth in code-adjacent docs (CLAUDE.md), and the inception-time spec lives in memory-bank.

## Dependencies

### Requires

- 001-daemon-binds-sock-dgram (the socket exists)
- 002-dgram-listener-replaces-fifo (the listener and wire format are in place)
- 003-cli-kick-subcommand (the `kick` CLI exists to document)
- 004-daemon-fifo-cleanup (FIFO is gone, so docs can confidently describe the new state)

### Enables

- Unit 2 (rollout migration) — once docs are accurate, the cross-repo activation can proceed against truthful docs

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `CLAUDE.md` has a Mermaid diagram of the IPC flow (it doesn't today, but might) | Update the diagram to show the new socket; remove the FIFO box |
| Daemon docstring includes pre-018 example commands (`echo > $fifo` style) | Replace with `media-control kick` examples |
| Some external tutorial / blog post / external repo references the FIFO | Out of scope — we don't update third-party docs. The FIFO cleanup (story 004) is gentle (silent on `NotFound`), so the breakage mode for any such caller is "their `echo > $fifo` no-ops because the FIFO doesn't exist", which is the same fail-silent mode as `media-control kick` against a down daemon. |

## Out of Scope

- Updating `~/.claude/CLAUDE.md` (global) — not in this repo, not appropriate scope.
- Updating any `~/.config/hypr` documentation — that's for unit 2's keybind work to handle if relevant.
- Updating `~/nix/modules/apps/media/media-control.nix` comments — handled in unit 2 story 002.
- Generating CHANGELOG.md or release notes — separate concern handled by the release workflow (the `commit` recipe doesn't currently maintain a changelog).
