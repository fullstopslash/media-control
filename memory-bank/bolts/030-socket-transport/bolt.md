---
id: 030-socket-transport
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
type: simple-construction-bolt
status: complete
stories:
  - 001-daemon-binds-sock-dgram
  - 002-dgram-listener-replaces-fifo
  - 003-cli-kick-subcommand
  - 004-daemon-fifo-cleanup
  - 005-docs-update
created: 2026-05-03T15:44:51.000Z
started: 2026-05-03T16:06:54.000Z
completed: "2026-05-03T16:44:34Z"
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-05-03T16:09:37.000Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-05-03T16:22:03.000Z
    artifact: implementation-walkthrough.md
requires_bolts: []
enables_bolts:
  - 031-rollout-migration
requires_units: []
blocks: false
complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 030-socket-transport

## Overview

Land the entire in-repo transport replacement in one cohesive bolt. Adds `media-control-lib` helpers (`socket_path()`, `kick()`), binds a `SOCK_DGRAM` socket in the daemon at startup with TOCTOU-safe creation, replaces the `fifo_listener` task with `dgram_listener` (locked wire format per FR-9), adds `media-control kick` CLI subcommand with non-blocking semantics, performs best-effort cleanup of the legacy FIFO, deletes ~118 LOC of FIFO machinery, and updates project docs. End state: the workspace is release-ready; cross-repo activation (Unit 2) is unblocked.

## Objective

Produce a release-ready `media-control` workspace where:

1. The daemon binds and listens on `$XDG_RUNTIME_DIR/media-control-daemon.sock` (`SOCK_DGRAM`, `0o600`).
2. `media-control kick` exists, sends 0 bytes, exits ≤ 100ms p99 in all daemon states.
3. The wire format is locked: 0-byte = canonical kick (forever); non-empty = reserved for v1+ with version-byte prefix (currently ignored with `debug!` log).
4. The legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` is best-effort unlinked at daemon startup; all FIFO-specific code is deleted.
5. Project docs (CLAUDE.md, readme.md, daemon docstring) accurately describe the new transport.
6. `cargo test --workspace --all-features` and `nix build .#default` are green.

## Stories Included

- **001-daemon-binds-sock-dgram** (Must) — TOCTOU-safe socket bind + `socket_path()` lib helper. 4 socket tests replace the 4 FIFO tests using `is_socket()`-based assertions (no inode equality, per `media-control-t8d` lessons).
- **002-dgram-listener-replaces-fifo** (Must) — `dgram_listener` task; idempotent kick coalescing via `try_send`; non-empty datagrams ignored with `debug!` log (FR-9 lock); bounded recv-error backoff (`SOCKET_ERROR_BACKOFF` ~100ms); rename `fifo_listener_handle` → `dgram_listener_handle` keeping AbortOnDrop shape.
- **003-cli-kick-subcommand** (Must) — `Commands::Kick` variant in `crates/media-control/src/main.rs` routed to lib `kick()`. Connectionless `sendto`; ECONNREFUSED/ENOENT silent (exit 0); other errors stderr+exit 1. CLI rejects `--reason` and any payload-shaping flag (FR-9 enforcement). Round-trip + daemon-down + p99-timing integration tests.
- **004-daemon-fifo-cleanup** (Should) — Best-effort `unlink` of legacy FIFO at startup (after socket bind succeeds, for symmetric error recovery); deletes all FIFO-specific functions (`get_fifo_path`, `create_fifo_at`, `create_fifo`, `remove_fifo`, `fifo_listener`) and the 4 FIFO unit tests; net deletion ~118 LOC.
- **005-docs-update** (Must) — CLAUDE.md, readme.md, daemon module docstring updated; all live docs reflect the new transport, FR-9 wire format, and the `kick` CLI subcommand.

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

Rationale: cli-tool project type with no domain modeling needed. Stories are tightly coupled scaffolding-and-replacement work, share a TOCTOU pattern, and mostly reuse the existing `fifo_listener` shape. DDD ceremony would slow this without adding value.

## Stages

- [ ] **1. plan**: Pending → implementation-plan.md
- [ ] **2. implement**: Pending → implementation-walkthrough.md
- [ ] **3. test**: Pending → test-walkthrough.md

## Dependencies

### Requires

- None (first bolt in the intent; no cross-unit dependencies)

### Enables

- 031-rollout-migration (the next bolt; can't run keybind migration or nix-module cleanup until this bolt's release ships)

## Success Criteria

- [ ] All 5 stories implemented; acceptance criteria met
- [ ] `cargo test --workspace --all-features` green
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `nix build .#default` green (preserving existing `doCheck` setting)
- [ ] Net negative LOC delta in `crates/media-control-daemon/src/main.rs` (FIFO machinery deleted exceeds new socket machinery)
- [ ] `grep -i fifo crates/media-control-daemon/src/main.rs` returns only the legacy FIFO cleanup path string and no live FIFO functions
- [ ] All live docs (CLAUDE.md project, readme.md, daemon docstring) reference the new socket transport and the `kick` CLI subcommand

## Notes

### Design-stage decisions (deferred from inception)

- **Lib `kick()` sync vs async**: open question from requirements.md. Recommendation: sync. CLI doesn't otherwise need tokio for this path; tokio runtime startup is non-trivial overhead vs. the < 100ms p99 target (FR-5). Async-symmetric consistency is nice-to-have but not load-bearing.
- **Module placement of `socket_path()` and `kick()` in `media-control-lib`**: extend existing `commands` module or add a new `transport` module? Recommend new `transport` module — these are transport concerns, not commands; bolt 026's `commands-regrouping` precedent supports semantic submodule layout.
- **TOCTOU helper extraction**: the `lstat + reject + unlink + create` pattern is used twice (for the soon-deleted FIFO and the new socket). Story 004 deletes the FIFO version. Recommend a single generic `create_unix_endpoint_at(path, kind)` helper parameterized over file-type predicate, even though only one caller will remain — cleaner and tests can exercise both branches.
- **Wire format log specifics** (story 002): `debug!("Ignoring v{:#04x} datagram ({} bytes, unsupported in this release)", buf[0], n)` or similar. Format details are bikeshed-able at design stage.

### Cross-cutting concerns

- **Daemon-stop hang from intent 017's discovered side issues**: hypothesized to be the FIFO `File::open` mid-await. This bolt deletes the FIFO listener entirely. May resolve incidentally; validated in bolt 031 story 003. If hang persists, file a follow-up intent.
- **Cleanup ordering** (story 004): FIFO cleanup must run *after* successful socket bind so that a partial migration (new bind fails) doesn't also leave the user without their old FIFO — symmetric error recovery preserves a fallback path.
- **Test posture** (per CLAUDE.md memory `media-control-t8d`): socket tests use `is_socket()` and bind-success assertions, NOT inode equality — tmpfs in nix sandbox reuses inodes and breaks inode-based identity tests.

### Atomic rollout (Q7 reminder)

This bolt produces the release. Bolt 031 (cross-repo activation) MUST run in the same release window — no intermediate state where keybinds reference a non-existent CLI subcommand. The two-bolt sequence is: 030 lands → release published → 031 runs against the release.
