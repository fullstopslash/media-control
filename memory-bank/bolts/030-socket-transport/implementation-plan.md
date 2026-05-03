---
stage: plan
bolt: 030-socket-transport
created: 2026-05-03T16:06:54Z
---

## Implementation Plan: socket-transport

### Objective

Replace the daemon's FIFO trigger transport (`$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`) with a `SOCK_DGRAM` UNIX socket (`$XDG_RUNTIME_DIR/media-control-daemon.sock`), add a first-class `media-control kick` CLI subcommand, lock the wire format per FR-9 (0-byte = canonical kick; non-empty = reserved with version-byte prefix), best-effort unlink the legacy FIFO at daemon startup, delete ~118 LOC of FIFO machinery from `crates/media-control-daemon/src/main.rs`, and update project docs. End state: release-ready workspace; bolt 031 (cross-repo activation) is unblocked.

---

### Deliverables

**New code**

- New `transport` module in `media-control-lib` exposing two helpers:
  - `socket_path()` — single source of truth for the daemon trigger socket path. Returns `runtime_dir()?.join("media-control-daemon.sock")`.
  - `kick()` — sync helper. Opens an unbound `UnixDatagram`, sends 0 bytes, classifies errors per FR-4 (Ok/ECONNREFUSED/ENOENT → silent; other → return error). Sync (not async) because the CLI's `kick` path doesn't otherwise need tokio and runtime startup would burn the FR-5 latency budget.
- New TOCTOU-safe socket creator in the daemon (`bind_trigger_socket(path) -> io::Result<UnixDatagram>`): lstat → reject-symlink → reject-non-socket → reject-wrong-uid → unlink → bind → explicit `chmod 0o600`. Mirrors `create_fifo_at` shape verbatim, swapping `mkfifo` for `UnixDatagram::bind` and the `is_fifo()` predicate for `is_socket()`. Single helper specialized to sockets — the FIFO version is being deleted in story 004, so a generic helper would have one caller.
- New `dgram_listener(socket, tx)` task in the daemon. Owns the bound `UnixDatagram`; loops on `recv_from` into a 16-byte buffer; classifies datagrams by length (0 → canonical kick → `tx.try_send(())`; ≥ 1 → version-byte log + ignore); handles transient recv errors with `SOCKET_ERROR_BACKOFF` (~100ms) backoff matching the existing `FIFO_ERROR_BACKOFF` posture.
- New `Commands::Kick` variant in `crates/media-control/src/main.rs` Subcommand enum, routed to `media_control_lib::transport::kick()`. No flags exposed (FR-9 enforcement).

**Renames**

- `fifo_listener_handle` → `dgram_listener_handle` in `run_event_loop`'s startup code.
- `fifo_rx` / `fifo_tx` → `dgram_rx` / `dgram_tx`.
- Log line shape: `Received FIFO trigger` → `Received datagram trigger`; `Processing FIFO trigger` → `Processing trigger` (matches inception-time spec).

**Deletions** (story 004)

- `get_fifo_path()` (line 235)
- `create_fifo_at()` (line 394, ~46 LOC)
- `create_fifo()` / `remove_fifo()` (lines 442-459, ~13 LOC)
- `fifo_listener()` (line 475, ~59 LOC) — replaced by `dgram_listener`
- `FIFO_ERROR_BACKOFF` constant — replaced by `SOCKET_ERROR_BACKOFF`
- 4 unit tests targeting `create_fifo_at` (replaced by 4 socket-equivalent tests in story 001)
- The reconnect-path FIFO-recreate block at lines 763-779 — the socket FD persists across reconnect, no recreation needed; the entire `Ok(true)` arm simplifies to "sleep 500ms + reconnect".

**One-time migration** (story 004)

- After successful socket bind, best-effort `std::fs::remove_file($XDG_RUNTIME_DIR/media-avoider-trigger.fifo)`. `NotFound` → silent (clean install path). Other errors → `debug!` log, ignore. Runs after socket bind succeeds for symmetric error recovery (a failed-bind daemon doesn't strip the user's old fallback).

**Tests** (workspace `cargo test --workspace --all-features` must remain green)

- 4 new socket tests in the daemon's test module replacing the 4 deleted FIFO tests:
  - `binds_at_fresh_path` — assert via `is_socket()` and bind success, **NOT** inode equality (per `media-control-t8d` lessons).
  - `rejects_symlink_at_path`
  - `rejects_regular_file_at_path`
  - `rebinds_over_our_own_existing_socket` — replaces the inode-comparison test that was destabilised by tmpfs inode reuse.
- New integration tests for `media-control kick`:
  - Round-trip: spawn test daemon, run `kick`, observe channel signal arrives.
  - Daemon-down silent: bind no socket, run `kick`, assert exit 0 + empty stderr + wall time < 100ms.
  - Socket-non-writable: bind socket, `chmod 000`, run `kick`, assert exit 1 + non-empty stderr.
  - Argument rejection: `kick --reason foo` exits non-zero (FR-9 enforcement at clap layer).
- New listener test: send 100 datagrams in a tight loop; assert ≤ 101 channel sends recorded (coalescing intact).
- New listener test: send `[0x01]` and `[0xFF]` datagrams; assert no channel send + one debug log line each.

**Docs** (story 005)

- `CLAUDE.md` (project root) — replace FIFO references in Architecture / Configuration sections with socket transport; add wire-format reservation table.
- `readme.md` — add `kick` to subcommand list; brief description.
- Daemon module docstring (top of `crates/media-control-daemon/src/main.rs` — currently mentions "manual triggers via FIFO" at line 6) — describe new socket transport, `dgram_listener`, FR-9 wire format, FR-8 cleanup behavior.
- `Commands::Kick` doc comment — short user-facing description.

---

### Dependencies

| Dependency | Why needed | Already present? |
|---|---|---|
| `tokio::net::UnixDatagram` | Daemon-side async recv_from loop | Yes (transitive via tokio's `net` feature; if not, add `net` feature flag) |
| `std::os::unix::net::UnixDatagram` | CLI-side sync sendto path (sync `kick()` choice) | Yes (std lib) |
| `nix::sys::stat::lstat` + `nix::unistd::unlink` + `nix::sys::stat::Mode` | TOCTOU-safe socket creation, mode setting | Yes (already used by `create_fifo_at`) |
| `media_control_lib::commands::shared::runtime_dir()` (line 100) | Resolve `$XDG_RUNTIME_DIR` for socket path | Yes — re-export or use through new `transport` module |
| `clap` derive `Subcommand` | Add `Kick` variant with no flags | Yes |
| `tracing` (debug/warn/error) | Log line shape changes; version-byte ignore log | Yes |

**No new runtime crates required.** Confirms intent's "No new runtime crates" constraint.

---

### Technical Approach

**Construction-stage decisions resolved here:**

1. **Lib `kick()` is sync, not async.** Rationale: FR-5 budget is < 100ms p99 for `media-control kick` invocations. Tokio runtime startup is non-trivial overhead (cold-process measured ~10–30ms in similar Rust CLIs). Sync path is `std::os::unix::net::UnixDatagram::unbound()` + `send_to(&[], path)` + match-on-error — three syscalls, no runtime. CLI doesn't need tokio for any other path on the `kick` codepath, so this avoids dragging the runtime into a hot, latency-sensitive entry point.

2. **New top-level `transport` module in `media-control-lib`**, not extension of `commands`. Rationale: transport is a substrate concern (the path constant + the kick helper are both transport-layer; `commands::*` are user-facing operations). Bolt 026 (commands-regrouping) established the precedent of semantic submodule layout. Add `pub mod transport;` to `lib.rs` next to the existing `pub mod hyprland;`.

3. **TOCTOU helper specialized to sockets**, not generic. Rationale: by end of bolt the FIFO version is deleted (story 004), so a generic file-type-parameterized helper would have exactly one caller. Specialization keeps the call site clear (`bind_trigger_socket(&path)?` reads better than `create_unix_endpoint_at(&path, EndpointKind::Datagram)?`). Generic helper is the right call if a third caller appears later (e.g. a separate diagnostic socket).

4. **Buffer size for `recv_from`**: 16 bytes. Rationale: we ignore non-empty datagrams; we just need the length classification. 16 bytes accommodates the largest plausible reserved short-version-envelope. Larger buffer = wasted heap; smaller = false-truncation classification edge cases.

5. **Wire-format ignore log shape**: `debug!("Ignoring v{:#04x} datagram ({} bytes, unsupported in this release)", buf[0], n)`. Includes both version byte (in hex for clarity at v0x01..0xFF range) and total length (so a 50-byte v1 envelope vs a 1-byte version-only marker are distinguishable in logs).

6. **Reconnect path simplification** (sequencing concern, not in story bodies but emerges from line 763-779 review): the existing FIFO recreate-on-reconnect logic exists because the FIFO would get unlinked between sessions. Sockets bound by the daemon persist as FDs across all `run_event_session` reconnects — no recreation needed. The `Ok(true)` arm collapses to `sleep 500ms; loop`. This is a clean simplification that comes for free with the transport swap.

7. **`SOCKET_ERROR_BACKOFF` constant placement**: co-located with the new `dgram_listener` function, not promoted to module-level. Mirrors how `FIFO_ERROR_BACKOFF` was scoped inside `fifo_listener`'s function body (line 480). Consistency with prior style.

**Order of operations within Stage 2 (Implement):**

1. Add the `transport` module and lib helpers (`socket_path`, `kick`) — no callers yet, just the substrate.
2. Add the daemon's `bind_trigger_socket` helper + `dgram_listener` task, side-by-side with the existing FIFO functions. Both transports temporarily coexist in the source.
3. Wire `dgram_listener` into `run_event_loop` startup. Rename channel + handle. Update log line shapes.
4. Delete FIFO functions (`get_fifo_path`, `create_fifo_at`, `create_fifo`, `remove_fifo`, `fifo_listener`, `FIFO_ERROR_BACKOFF`) and the FIFO-recreate block in the reconnect path.
5. Add `cleanup_legacy_fifo()` call after socket bind. (Could be done earlier; sequenced last among code changes for safe rollback if anything in steps 1-4 has issues.)
6. Add `Commands::Kick` variant in CLI; route to `media_control_lib::transport::kick()`.
7. Replace the 4 FIFO unit tests with 4 socket-equivalent tests. Add the new integration tests for `kick`, listener coalescing, and version-byte ignore.
8. Update CLAUDE.md, readme.md, and the daemon docstring (story 005).

**Symmetric error recovery** (FR-8 ordering): the legacy FIFO cleanup MUST run after the socket bind succeeds. Failed bind → daemon exits → user's old FIFO is preserved as a manual fallback path. This is critical and is captured in story 004's acceptance criteria.

**Test posture**: every new test asserts on observable behaviour (`is_socket()`, `recv_from` returns expected bytes, channel send count, exit code, stderr content). NO inode equality. NO timing assertions tighter than 100ms (per `media-control-t8d` lessons about nix-sandbox tmpfs reuse and timing flakiness).

---

### Acceptance Criteria

**Per-story (rolled up from story files)**

- [ ] Story 001 — Daemon binds `SOCK_DGRAM` at the path with TOCTOU-safe creation; 4 new socket tests using `is_socket()` not inode equality; lib `socket_path()` is the single source of truth.
- [ ] Story 002 — `dgram_listener` replaces `fifo_listener`; 0-byte → kick; non-empty → debug log + ignore (FR-9 lock); recv-error backoff bounded; AbortOnDrop shape preserved.
- [ ] Story 003 — `media-control kick` exists; ECONNREFUSED/ENOENT silent (exit 0); other errors → exit 1 + stderr; CLI rejects payload-shaping flags; p99 < 100ms in all daemon states.
- [ ] Story 004 — Legacy FIFO unlinked on daemon startup (best-effort); ~118 LOC of FIFO machinery deleted; reconnect-path FIFO-recreate block deleted.
- [ ] Story 005 — CLAUDE.md, readme.md, daemon docstring all updated; `grep -ri 'media-avoider-trigger.fifo' crates/ readme.md CLAUDE.md` finds only the cleanup path string.

**Workspace gates**

- [ ] `cargo build --workspace` clean
- [ ] `cargo test --workspace --all-features` green
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `nix build .#default` green (preserving the existing `doCheck = false` if currently set; no change to that knob)
- [ ] Net negative LOC delta in `crates/media-control-daemon/src/main.rs` (sanity check; ~118 LOC deleted, smaller socket replacement added)

**FR coverage validation**

- [ ] FR-1 (TOCTOU-safe bind), FR-2 (0-byte kick coalescing), FR-3 (recv-error backoff), FR-9 (version-byte ignore) demonstrated by daemon-side tests.
- [ ] FR-4 (kick CLI exit codes), FR-5 (< 100ms p99 in all daemon states) demonstrated by CLI integration tests.
- [ ] FR-8 (legacy FIFO cleanup) demonstrated by either an integration test (set up FIFO, start daemon, assert FIFO gone) or manual verification documented in test-walkthrough.md.
- [ ] FR-6, FR-7 are bolt 031's responsibility, not this bolt's — explicitly out of scope.

---

### Open Questions for Implementation

None blocking. The four construction-stage decisions above (sync `kick`, `transport` module placement, specialized TOCTOU helper, log line shape) are decided and recorded here. If implementation surfaces a reason to reverse any, capture in the implementation-walkthrough Deviations section.

---

### Out of Scope (reminder)

- Hyprland keybind migration — bolt 031 story 001 (FR-6).
- NixOS module deletion — bolt 031 story 002 (FR-7).
- End-to-end DoD validation on `malphas` — bolt 031 story 003.
- Designing the v1 envelope structure — future intent. FR-9 reserves the wire format only.
- Investigating the daemon-stop hang from intent 017 — bolt 031 story 003 measures whether deleting the FIFO listener resolves it incidentally; if not, separate intent.
- Wiring `sd_listen_fds()` — explicit non-goal of intent 018.
