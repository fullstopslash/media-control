---
stage: implement
bolt: 030-socket-transport
created: 2026-05-03T16:19:32Z
---

## Implementation Walkthrough: socket-transport

### Summary

Replaced the daemon's FIFO trigger transport with a `SOCK_DGRAM` UNIX
socket bound at `$XDG_RUNTIME_DIR/media-control-daemon.sock`. Added a new
`media-control-lib::transport` module that owns the canonical socket
filename and a sync `kick()` helper used by both the new
`media-control kick` CLI subcommand and (via the same path constant) the
daemon's own bind. Locked the FR-9 wire format at the listener: 0-byte =
canonical kick, non-empty = reserved/ignored with debug log. Deleted all
FIFO machinery (~160 LOC) and added a best-effort legacy-FIFO unlink at
daemon startup. Build, clippy, and 413 tests across the workspace are
green.

### Structure Overview

Three crates, three concentric responsibilities:

- `media-control-lib`: new `transport` substrate module owning the socket
  filename constant, the path helper, and the sync `kick()` function with
  classification of `DaemonDown` (ECONNREFUSED/ENOENT silent) vs. real
  errors.
- `media-control-daemon`: `bind_trigger_socket()` mirroring the prior
  `create_fifo_at` TOCTOU posture (lstat → reject-symlink → reject-non-
  socket → reject-wrong-uid → unlink → bind → chmod 0o600), the
  `dgram_listener` task replacing `fifo_listener`, and a one-time
  `cleanup_legacy_fifo()` that runs after successful bind.
- `media-control`: new `Commands::Kick` variant routed through an
  early-return pre-config branch matching the existing `Status` /
  `Completions` pattern.

### Completed Work

**Library (`media-control-lib`)**

- [x] `crates/media-control-lib/src/transport.rs` (new) — owns the
  `SOCKET_FILENAME` constant, `socket_path()` resolver, `KickOutcome`
  enum, and `kick()`/`kick_to()` send helpers. 4 co-located unit tests.
- [x] `crates/media-control-lib/src/lib.rs` — added `pub mod transport;`
  alongside the existing top-level modules.

**Daemon (`media-control-daemon`)**

- [x] `crates/media-control-daemon/src/main.rs` — module docstring
  updated to describe the new socket transport, FR-9 wire format, and
  the `kick` CLI subcommand.
- [x] `crates/media-control-daemon/src/main.rs` — added
  `bind_trigger_socket_at()`, `bind_trigger_socket()`,
  `remove_trigger_socket()`, `cleanup_legacy_fifo()`, and
  `dgram_listener()`. Renamed all `fifo_*` identifiers to `dgram_*` in
  `run_event_loop` / `run_event_session`. Updated trigger-arm log line
  (`Processing FIFO trigger` → `Processing trigger`) and listener log
  line shape (`Received FIFO trigger` → `Received datagram trigger`).
- [x] `crates/media-control-daemon/src/main.rs` — deleted
  `get_fifo_path`, `create_fifo_at`, `create_fifo`, `remove_fifo`,
  `fifo_listener`, the inline `FIFO_ERROR_BACKOFF` constant, the
  reconnect-path FIFO-recreate block (~28 LOC), and the now-dead
  `AbortOnDrop::take()` helper.
- [x] `crates/media-control-daemon/src/main.rs` — replaced 4 FIFO unit
  tests with 4 socket-equivalent tests (`bind_trigger_socket_at_*`)
  using `is_socket()` assertions instead of inode equality, per
  `media-control-t8d` lessons. Added 2 new listener tests
  (`dgram_listener_forwards_zero_byte_kick`,
  `dgram_listener_ignores_non_empty_datagrams`) covering FR-2 and FR-9.
- [x] `crates/media-control-daemon/src/main.rs` — updated
  `run_foreground` cleanup to call `remove_trigger_socket()` instead of
  `remove_fifo()`. Updated `AbortOnDrop` docstring to remove the
  FIFO-specific motivation paragraph.

**CLI (`media-control`)**

- [x] `crates/media-control/src/main.rs` — added `Commands::Kick` variant
  with a doc comment describing the connectionless send semantics.
  Added an early-return pre-config branch that calls
  `transport::kick()`, classifies `Delivered`/`DaemonDown` as exit-0,
  and surfaces other errors to stderr with exit-1. Added a usage line
  in the top-of-file docstring under a new "Daemon control" section.

**Documentation (story 005)**

- [x] `CLAUDE.md` — replaced the FIFO line with a paragraph describing
  the new socket transport, the wire format reservation (FR-9), the
  `media-control kick` entry point, and the FR-8 legacy-FIFO unlink
  behavior. (Note: the surrounding CLAUDE.md still describes the
  pre-Rust bash project — that's pre-existing doc rot outside this
  intent's scope.)
- [x] `readme.md` — added a `media-control kick` entry to the usage
  block with a brief description of when to use it (Hyprland keybinds
  for layoutmsg etc.).

### Key Decisions

- **Lib `kick()` is sync, not async.** Honored the construction-stage
  decision from the plan: tokio runtime startup would burn the FR-5
  budget for a 3-syscall operation. Used `std::os::unix::net::UnixDatagram`
  for the CLI sender; `tokio::net::UnixDatagram` only on the daemon side
  where we're already inside the runtime.
- **New top-level `transport` module**, not extension of `commands`.
  Transport is a substrate concern; `commands::*` are user-facing
  operations. Mirrors the bolt-026 commands-regrouping precedent.
- **TOCTOU helper specialized to sockets**, not generic. The FIFO
  version is being deleted in this same bolt, so a generic file-type-
  parameterized helper would have one caller. `bind_trigger_socket_at`
  reads cleanly without a kind enum.
- **`SOCKET_ERROR_BACKOFF` co-located with `dgram_listener`**, mirroring
  how `FIFO_ERROR_BACKOFF` was scoped inside `fifo_listener`'s body.
  Consistency with prior style.
- **Reconnect path simplified for free.** The trigger socket FD is owned
  by the listener task for the daemon's lifetime, so no recreation is
  needed across `run_event_session` reconnects. The pre-018 `Ok(true)`
  arm collapsed from ~28 LOC of FIFO-recreate-with-listener-stop logic
  to a 2-line "sleep + loop" comment.
- **`kick_to` private helper for testability.** `kick()` itself takes no
  arguments (it resolves the path through `socket_path()`); tests
  exercise `kick_to(&Path)` directly to avoid env-mutation races
  through the lib's process-wide async test mutex. Pattern matches the
  existing daemon-side `bind_trigger_socket_at(&Path)` split.

### Deviations from Plan

- **Net LOC delta in `main.rs` is positive (+39), not negative.** The
  unit-brief listed "net negative LOC delta" as a sanity check, not a
  hard target. The growth comes from two new dgram_listener integration
  tests (~85 LOC) that have no FIFO equivalent — the prior FIFO listener
  had no automated wire-contract tests. The substrate code itself
  shrunk by ~50 LOC (FIFO machinery deleted ~160 LOC, socket
  replacement added ~110 LOC including the more comprehensive
  docstrings). Trading code LOC for new test coverage on the wire
  contract is a deliberate gain on the FR-9 lock.
- **Did not add a "socket-non-writable → exit 1" CLI integration test.**
  Construction-stage decision: testing `chmod 000` on a self-owned
  socket from inside the test process doesn't reliably surface
  `PermissionDenied` on every filesystem (especially tmpfs in the nix
  sandbox). The error path is exercised by inspection: the `kick_to`
  function returns `MediaControlError::from(io::Error)` for any error
  outside ECONNREFUSED/ENOENT, and the CLI's early-return matches
  `Err(_)` to the exit-1 path. Story 003 acceptance criterion for this
  case is folded into bolt 031's DoD validation matrix (manual test
  on `malphas`).

### Dependencies Added

None. `tokio::net::UnixDatagram` (daemon) and `std::os::unix::net::UnixDatagram`
(CLI/lib tests) were already transitively available; no Cargo.toml
changes were required.

### Developer Notes

- Verification commands all green at end of stage:
  - `cargo build --workspace` — clean
  - `cargo clippy --workspace --all-features -- -D warnings` — clean
  - `cargo test --workspace --all-features` — 413/413 pass (lib 372,
    daemon 20, lib boundary 2, lib config-integration 3, lib doctests
    16; CLI binary unit-test target ran 0 tests as before)
- The boundary test `daemon_source_contains_no_forbidden_imports`
  continues to pass after the source restructuring.
- `grep -ri media-avoider-trigger.fifo crates readme.md CLAUDE.md`
  surfaces only the expected references: the FR-8 cleanup path string
  in `crates/media-control-daemon/src/main.rs` (twice — once in the
  function docstring, once in the path constant) and one historical
  note in CLAUDE.md ("Pre-018 daemons used a FIFO at..."). All other
  references are under `memory-bank/` or `intents/` (intent-history
  artifacts).
- The daemon-stop hang from intent 017's discovered side issues — the
  FIFO `File::open` mid-await hypothesis — is now untestable here
  because the FIFO listener is gone. Bolt 031 story 003 measures
  whether the hang persists post-018 (resolved-incidentally vs.
  separate-cause-needs-its-own-intent).
