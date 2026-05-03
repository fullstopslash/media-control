---
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
phase: inception
status: complete
created: 2026-05-03T15:44:51.000Z
updated: 2026-05-03T15:44:51.000Z
---

# Unit Brief: socket-transport

## Purpose

Replace the daemon's FIFO trigger transport with a `SOCK_DGRAM` UNIX socket and add a first-class `media-control kick` CLI subcommand. All in-repo Rust changes; produces a release-ready `media-control` workspace where the new transport is live, the legacy FIFO is auto-cleaned at daemon startup, and the wire format (FR-9) is locked for future extension.

## Scope

### In Scope

- New lib helper `media-control-lib::socket_path()` (or equivalent) — single source of truth for the socket path; used by both daemon `bind()` and CLI `kick()`. One constant for the filename.
- New lib helper `media-control-lib::kick()` — opens `UnixDatagram`, sends 0 bytes, handles errors per FR-4.
- Daemon: bind `UnixDatagram` at `$XDG_RUNTIME_DIR/media-control-daemon.sock` mode `0o600` at startup with TOCTOU-safe creation (lstat → reject-symlink → reject-non-socket → reject-wrong-uid → unlink → bind). Mirrors `create_fifo_at`.
- Daemon: `dgram_listener` task replaces `fifo_listener`. `recv_from` loop pushes idempotent kicks into the existing `mpsc<()>` channel via `try_send`. Length-0 datagrams trigger an avoid pass; length ≥ 1 datagrams are ignored with `debug!` log.
- Daemon: bounded backoff (`SOCKET_ERROR_BACKOFF` ~100ms) on `recv_from` errors other than `WouldBlock`.
- Daemon: rename `fifo_listener_handle` → `dgram_listener_handle`; keep AbortOnDrop spawn shape (Q3).
- Daemon: best-effort `unlink` of legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` at startup. Failure → `debug!` log.
- CLI: new `Commands::Kick` variant in `crates/media-control/src/main.rs`, routed to lib `kick()`.
- Tests: 4 socket tests replacing the 4 FIFO tests (rebind-on-restart, reject-symlink, reject-regular-file, reject-wrong-uid), using `is_socket()` not inode equality (per `media-control-t8d` lessons).
- Tests: `media-control kick` integration test exercising daemon-down (silent exit 0), socket-non-writable (exit 1), and round-trip cases.
- Documentation: `CLAUDE.md` (project), `readme.md`, daemon module docstring updated to reflect the new transport, FR-9 wire format reservation, and CLI subcommand.
- Deletion: all FIFO-specific functions (`get_fifo_path`, `create_fifo_at`, `create_fifo`, `remove_fifo`, `fifo_listener`) and their unit tests.

### Out of Scope

- Hyprland keybind migration — Unit 2 (FR-6).
- NixOS module deletion — Unit 2 (FR-7).
- End-to-end DoD validation on `malphas` — Unit 2.
- Designing the v1 envelope structure (FR-9 reserves the wire format; the envelope itself is a future intent).
- Investigating the daemon-stop hang from intent 017's discovered side issues (separate intent if it persists post-unit-1).
- Wiring `sd_listen_fds()` (explicit non-goal of the intent).
- Adding `--reason` or any payload-shaping flag to `kick` (FR-9 reserves it; not exposed today).

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Daemon binds a single SOCK_DGRAM socket at startup | Must |
| FR-2 | Daemon accepts trigger datagrams without parsing payload | Must |
| FR-3 | Daemon recovers from socket errors with bounded backoff | Must |
| FR-4 | `media-control kick` CLI subcommand | Must |
| FR-5 | Daemon-down kick must not block the keybind shell | Must |
| FR-8 | Migration safety — daemon-side FIFO cleanup | Should |
| FR-9 | Reserved version-byte wire format for future extensibility | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| Trigger socket | The `UnixDatagram` bound at `$XDG_RUNTIME_DIR/media-control-daemon.sock` | Mode `0o600`, owner uid == daemon uid, file type `is_socket()`, lifetime == daemon process lifetime |
| Kick datagram | A datagram delivered to the trigger socket | Length 0 = canonical kick; length ≥ 1 = reserved (byte 0 is version) |
| `mpsc<()>` trigger channel | Existing in-process channel from listener task to avoider | Bounded; coalesces under load via `try_send` (drop-on-full) |
| Legacy FIFO | The pre-018 transport at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` | Best-effort unlinked at daemon startup; existence after one daemon restart cycle is the migration bug signal |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| `socket_path()` (lib) | Resolve `runtime_dir().join("media-control-daemon.sock")` | none | `PathBuf` |
| `kick()` (lib) | Open `UnixDatagram`, sendto 0 bytes, classify errors | none | `Result<(), KickError>` where errors map to exit codes per FR-4 |
| `bind_trigger_socket()` (daemon) | TOCTOU-safe bind; returns `UnixDatagram` ready to recv | path | `Result<UnixDatagram, _>` |
| `dgram_listener()` (daemon) | Async task; recv_from loop; coalesce kicks into channel | socket, sender | runs forever; aborts on AbortOnDrop |
| `cleanup_legacy_fifo()` (daemon) | Best-effort unlink of legacy FIFO path | path | `()` (failures debug-logged) |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 5 |
| Must Have | 4 |
| Should Have | 1 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-daemon-binds-sock-dgram | TOCTOU-safe socket bind + `socket_path()` lib helper | Must | Planned |
| 002-dgram-listener-replaces-fifo | `dgram_listener` task + version-byte ignore + bounded backoff | Must | Planned |
| 003-cli-kick-subcommand | `media-control kick` + lib `kick()` helper + non-blocking semantics | Must | Planned |
| 004-daemon-fifo-cleanup | Best-effort legacy FIFO unlink + delete FIFO machinery | Should | Planned |
| 005-docs-update | CLAUDE.md / readme.md / daemon docstring updates | Must | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| (none) | Pure in-repo substrate addition |

### Depended By

| Unit | Reason |
|------|--------|
| 002-rollout-migration | Keybind migration calls `media-control kick`; nix module cleanup assumes daemon binds its own socket |

### External Dependencies

| System | Purpose | Risk |
|--------|---------|------|
| `tokio::net::UnixDatagram` | Socket bind + recv_from + send_to | Low — already transitively present; well-trodden API |
| `nix` crate (or libc) for `lstat` + `unlink` | TOCTOU-safe socket creation | Low — same primitives the existing `create_fifo_at` uses |
| Filesystem at `$XDG_RUNTIME_DIR` | Socket bind path | Low — already exists; CLI/daemon both have write access |

---

## Technical Context

### Suggested Technology

- Reuse `tokio::net::UnixDatagram` (already available transitively).
- Reuse the existing `runtime_dir()` helper for `$XDG_RUNTIME_DIR` resolution.
- For TOCTOU-safe socket creation, follow `create_fifo_at`'s structure step-for-step: separate the lstat-and-validate phase from the bind phase, with explicit unlink between them when the existing entry is "ours".
- For the listener task: keep the same `tokio::spawn` + `AbortOnDrop` shape that `fifo_listener` uses; just rename and swap the recv primitive.
- For `kick()` CLI helper: connectionless `UnixDatagram::unbound()` + `send_to(&[], path)` is the simplest and never-blocks path. Async-vs-sync is a construction-stage choice (see Open Questions in requirements.md).

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `media-control-lib::socket_path()` ↔ daemon `bind()` | Function call | Synchronous path resolution |
| `media-control-lib::kick()` ↔ daemon socket | UNIX domain | `SOCK_DGRAM`, 0-byte payload |
| `dgram_listener` ↔ avoider | In-process `mpsc<()>` | Same channel `fifo_listener` feeds today |

### Data Storage

None. Socket is ephemeral (process-lifetime). No on-disk state introduced.

---

## Constraints

- No new runtime crates (`tokio::net::UnixDatagram` already transitively available).
- No `libsystemd` / `sd_listen_fds()` (explicit non-goal).
- TOCTOU-safe creation MUST mirror `create_fifo_at` posture exactly (the security-relevant invariants are identical).
- Tests MUST NOT assert on inode equality (per CLAUDE.md memory: tmpfs inode reuse in nix sandbox).
- `kick` CLI MUST exit ≤ 100ms p99 in all daemon states (FR-5).
- Legacy FIFO cleanup is best-effort; failure MUST NOT prevent daemon startup (FR-8).
- The lib `socket_path()` helper MUST be the single source of truth — daemon and CLI both call it; the filename is one constant.

---

## Success Criteria

### Functional

- [ ] Daemon binds `SOCK_DGRAM` at expected path with `0o600`; pre-positioned symlink/wrong-uid/non-socket all rejected (FR-1)
- [ ] Empty datagram triggers exactly one avoid pass (FR-2); 100 rapid kicks → ≤ 101 trigger evaluations (channel coalescing intact)
- [ ] Non-empty datagram (`[0x01]`, `[0xFF]`) ignored with single `debug!` log line each (FR-9)
- [ ] Recv error storm bounded ≤ 10% CPU during burst (FR-3); bind failure at startup propagates non-zero exit
- [ ] `media-control kick` exits 0 silently when daemon down or socket file missing (FR-4, FR-5)
- [ ] `media-control kick` exits 1 with stderr message when socket exists but is non-writable (FR-4)
- [ ] `media-control kick --reason foo` exits non-zero with "unrecognized option" (FR-9 enforcement at CLI layer)
- [ ] Legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` is gone after daemon reaches "ready" log line; daemon doesn't error if FIFO never existed (FR-8)
- [ ] All FIFO-specific functions and tests removed from `crates/media-control-daemon/src/main.rs`

### Non-Functional

- [ ] Kick latency end-to-end < 50ms p95 (keybind shell to daemon `Processing trigger`)
- [ ] `media-control kick` invocation < 100ms p99 across all daemon states
- [ ] `cargo test --workspace --all-features` green
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `nix build .#default` green (preserving the existing `doCheck` setting)
- [ ] Net negative LOC delta in `crates/media-control-daemon/src/main.rs` (sanity check; FIFO machinery is ~118 LOC)

### Quality

- [ ] 4 new socket tests cover the same TOCTOU surface the 4 FIFO tests covered (rebind, reject-symlink, reject-non-socket, reject-wrong-uid) using `is_socket()` not inode equality
- [ ] `media-control kick` has integration test for round-trip + daemon-down silent path
- [ ] Documentation (CLAUDE.md, readme.md, daemon docstring) reflects new transport, FR-9 wire format, and CLI subcommand

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 030-socket-transport | simple-construction-bolt | 001, 002, 003, 004, 005 | Land the entire in-repo transport replacement as one cohesive bolt — stories share scaffolding (lib helpers, TOCTOU patterns, test infrastructure) and Q7 mandates a single coordinated rollout |

---

## Notes

- **Construction-stage decisions to make**:
  - Lib `kick()` sync vs async: async-symmetric with the rest of `media-control-lib` is consistent; sync is one less dep for the CLI process. Pick at design stage.
  - Module placement: extend `media-control-lib::commands` or add a new top-level `media-control-lib::transport` module? Recommend new module since `socket_path()` and `kick()` are transport concerns, not commands.
  - TOCTOU helper extraction: the `lstat + reject + unlink + create` pattern is now used twice (FIFO is being deleted, but the socket version mirrors it exactly). One generic helper or two parallel call sites? Probably one helper parameterized over file-type predicate.
- **Cross-cutting concern**: The 5+ second daemon-stop hang from intent 017's discovered side issues was hypothesised to be the FIFO `File::open` mid-await. This unit deletes the FIFO listener entirely, so the hang **may resolve incidentally**. Validate during construction; if hang persists, file a follow-up intent.
- **The bolt's design stage should also confirm** that `cleanup_legacy_fifo()` runs *after* the new socket is bound (so a partial migration where the new socket fails to bind doesn't also leave the old FIFO gone — symmetric error recovery).
