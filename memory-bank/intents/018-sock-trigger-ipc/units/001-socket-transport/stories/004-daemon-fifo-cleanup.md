---
id: 004-daemon-fifo-cleanup
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
status: complete
priority: should
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 030-socket-transport
implemented: true
---

# Story: 004-daemon-fifo-cleanup

## User Story

**As a** user upgrading from a pre-018 daemon (which created a FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`) to the post-018 daemon
**I want** the new daemon to best-effort remove the legacy FIFO at startup
**So that** my runtime directory isn't left polluted with a stale FIFO file that nothing reads, and `find /run/user -name '*media*'` shows only the new socket after one upgrade cycle

## Acceptance Criteria

- [ ] **Given** a host with a stale FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` (left by a pre-018 daemon), **When** the new daemon starts and successfully binds the new socket, **Then** the legacy FIFO is removed before the daemon's "ready" log line
- [ ] **Given** the legacy FIFO does not exist (clean install), **When** the new daemon starts, **Then** no error and no warn-level log entry is produced (cleanup is silent on the clean-install path)
- [ ] **Given** the legacy FIFO exists but `unlink` fails (e.g. permission, racing with another process), **When** the daemon attempts cleanup, **Then** the failure is logged at `debug` level and ignored — daemon continues startup successfully
- [ ] **Given** the new socket bind fails, **When** the daemon would otherwise have run cleanup, **Then** cleanup does NOT run (symmetric error recovery: don't remove the legacy FIFO when the new transport isn't actually up)
- [ ] **Given** all FIFO-specific code is removed, **When** I run `grep -i fifo crates/media-control-daemon/src/main.rs`, **Then** the only remaining hit is the cleanup call's path string `media-avoider-trigger.fifo` (and any code that references it through it) — no dead `create_fifo`, `remove_fifo`, `fifo_listener`, `get_fifo_path`, `create_fifo_at`, `FIFO_ERROR_BACKOFF` symbols remain
- [ ] **Given** the 4 pre-existing FIFO unit tests (`fresh-path`, `symlink`, `regular-file`, `replace-our-own`), **When** the test module is read post-change, **Then** they are deleted (replaced by the 4 socket equivalents in story 001)

## Technical Notes

- Cleanup runs in `main()` *after* the new socket is successfully bound (story 001), *before* the dgram_listener task is spawned (story 002). This ordering is deliberate: failed bind → no cleanup → user can manually fall back to the old daemon if needed.
- Implementation:
  ```rust
  let legacy_fifo = runtime_dir().join("media-avoider-trigger.fifo");
  if let Err(e) = std::fs::remove_file(&legacy_fifo) {
      if e.kind() != std::io::ErrorKind::NotFound {
          debug!("legacy FIFO cleanup failed at {}: {}", legacy_fifo.display(), e);
      }
  }
  ```
  - `NotFound` is the clean-install path → silent.
  - Other errors → `debug!` log, continue.
- Deletions in `crates/media-control-daemon/src/main.rs`:
  - `get_fifo_path()` (line 235)
  - `create_fifo_at()` (line 394, ~46 LOC)
  - `create_fifo()` / `remove_fifo()` wrappers (~13 LOC)
  - `fifo_listener()` (line 475, ~59 LOC) — **already replaced** by `dgram_listener` in story 002; this story confirms deletion.
  - `FIFO_ERROR_BACKOFF` constant — replaced by `SOCKET_ERROR_BACKOFF` in story 002.
  - `fifo_rx` field — renamed to `dgram_rx` in story 002.
  - 4 unit tests targeting `create_fifo_at` — replaced by the 4 socket tests in story 001.
- Net deletion: ~118 LOC of FIFO machinery from `main.rs`. Sanity-check the unit-brief NFR (net negative LOC delta in `main.rs`).
- This story is sequenced last among code stories so deletions don't precede the replacements that need to compile.

## Dependencies

### Requires

- 001-daemon-binds-sock-dgram (cleanup runs after this succeeds; symmetric error recovery)
- 002-dgram-listener-replaces-fifo (the `fifo_listener` deletion happens once `dgram_listener` is in place)

### Enables

- 005-docs-update (docs can confidently say "the FIFO is gone" only after this story lands)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Two daemons racing on startup (shouldn't happen but defensive) | First to `unlink` wins; second sees `NotFound` and is silent |
| Legacy FIFO is actually a file with the same name (someone's `touch`'d it) | `remove_file` succeeds regardless of file type — cleanup intent is "remove whatever is at the legacy path"; no type check needed for cleanup (the *new* socket's bind path has TOCTOU defense in story 001) |
| Legacy FIFO is a directory at the path (degenerate) | `remove_file` fails with `IsADirectory` → `debug!` log, ignored; user can clean manually |
| Cleanup runs on a pre-018-FIFO host where daemon was never installed (no legacy FIFO ever existed) | Silent (the `NotFound` clean-install path) |

## Out of Scope

- TOCTOU defense at the legacy FIFO path — the cleanup is best-effort and we don't bind anything there afterward. The TOCTOU surface is at the *new* socket path (story 001).
- Migrating any state from FIFO to socket — the FIFO carried no state; it was a write-only signal.
- CLI-side cleanup (Q2 resolved daemon-side): the CLI does not attempt FIFO cleanup. Means a host where the new daemon never gets installed/restarted retains the legacy FIFO. Acceptable: the FIFO file is harmless to leave behind; cleanup is a hygiene nice-to-have.
- Investigating why the daemon-stop hang exists (intent 017's discovered side issue) — unit 2 story 003 validates whether deleting the FIFO listener resolves it incidentally.
