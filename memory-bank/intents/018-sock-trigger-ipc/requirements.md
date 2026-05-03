---
intent: 018-sock-trigger-ipc
phase: inception
status: inception-complete
created: 2026-05-03T15:44:51Z
updated: 2026-05-03T16:03:16Z
---

# Requirements: Replace FIFO trigger IPC with `SOCK_DGRAM` UNIX socket

## Intent Overview

Consolidate the daemon's external-trigger IPC onto a single UNIX domain socket
(`SOCK_DGRAM`) at `$XDG_RUNTIME_DIR/media-control-daemon.sock`, retire the FIFO
at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`, delete the dead
`systemd.user.sockets.media-control-daemon` unit, and add a first-class
`media-control kick` CLI subcommand so keybinds and scripts no longer have to
know the transport's path.

Today the daemon binds the FIFO for manual avoid kicks while the systemd
`.socket` unit is dead code (the daemon never accepts on it). The FIFO has a
real footgun: `echo > $fifo` from a Hyprland keybind blocks indefinitely if
no reader is open. `SOCK_DGRAM` is one syscall, never blocks the writer, and
drops cleanly when the daemon is down. Failure mode becomes "the kick was
silently lost" instead of "the keybind is now wedged."

## Type

Refactor + hardening (transport replacement; user-visible only via fixed
keybind reliability).

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Eliminate the keybind-shell-hangs-when-daemon-down hazard | `pkill -KILL media-control-daemon` followed by any layoutmsg keybind exits 0 silently within 100ms; no shell hang, no dunst spam | Must |
| Reduce daemon's external IPC surface to one well-documented entry point | After this lands: one socket, one CLI subcommand, no orphaned systemd unit; new readers of the systemd config don't have to grep daemon source to find the live transport | Must |
| `media-control kick` is the only blessed way to wake the avoider from external code | Hyprland keybinds, scripts, and ad-hoc shell calls all use `media-control kick` — no caller embeds the socket path | Must |
| Wire format is forward-compatible without ever breaking the canonical kick | A v0.2.x release can add structured payloads (e.g. `--reason "togglesplit"`) without changing how a 0-byte kick is interpreted | Should |

---

## Functional Requirements

### FR-1: Daemon binds a single SOCK_DGRAM socket at startup
- **Description**: At startup the daemon binds a `UnixDatagram` at
  `$XDG_RUNTIME_DIR/media-control-daemon.sock` with mode `0o600`. Stale-socket
  handling mirrors the existing `create_fifo_at` safety posture: lstat the
  path → reject if symlink → reject if present and not a socket → reject if
  uid != ours → unlink → bind. **MUST not** follow symlinks.
- **Acceptance Criteria**: After daemon start, `ss -lx` shows a `u_dgr`
  listener at the expected path with mode `0o600`. With a pre-existing
  symlink at the path, daemon refuses to start and logs error. With a
  pre-existing regular file or wrong-owner socket, daemon refuses to start.
  With a pre-existing socket owned by us, daemon unlinks and rebinds.
- **Priority**: Must
- **Related Stories**: TBD

### FR-2: Daemon accepts trigger datagrams without parsing payload
- **Description**: A 0-byte datagram is the canonical "re-evaluate placement"
  kick and MUST always remain valid. The daemon coalesces kicks into the
  existing `mpsc<()>` channel that `fifo_listener` feeds today (try_send +
  drop on full channel). In this release the daemon ignores all non-empty
  datagrams — see FR-9 for the reserved wire format.
- **Acceptance Criteria**: Sending an empty datagram to the socket triggers
  exactly one `Processing trigger` log line and one avoid pass within ~50ms.
  Sending 100 datagrams in a tight loop produces ≤ N+1 trigger evaluations
  (channel coalescing intact).
- **Priority**: Must
- **Related Stories**: TBD

### FR-3: Daemon recovers from socket errors with bounded backoff
- **Description**: On `recv_from` error other than `WouldBlock`: log at
  `warn`, sleep `SOCKET_ERROR_BACKOFF` (~100ms, mirroring
  `FIFO_ERROR_BACKOFF`), continue. The socket FD is held for the daemon's
  lifetime — no per-message rebind. On bind failure at startup: log `error`,
  exit non-zero (same severity as the current `create_fifo` failure path).
- **Acceptance Criteria**: Inducing transient recv errors (mock fault) does
  not crash the daemon and does not exceed 10% CPU during a sustained error
  burst. Bind failure at startup propagates a non-zero exit code.
- **Priority**: Must
- **Related Stories**: TBD

### FR-4: `media-control kick` CLI subcommand
- **Description**: Connectionless `sendto` of a 0-byte datagram to the
  daemon socket. Resolves the socket path via the same
  `runtime_dir().join("media-control-daemon.sock")` helper the daemon uses
  (single source of truth). Exit code semantics:
  - `0` on `Ok`
  - `0` on `ECONNREFUSED` / `ENOENT` (daemon-not-running is silent for
    keybind ergonomics — not a user error in that context)
  - `1` with stderr message on any other error (permission denied, path
    validation failure, etc.) so script callers can still debug.
- **Acceptance Criteria**: With daemon running: `media-control kick && echo
  ok` prints `ok` and the daemon journal shows `Processing trigger`. With
  daemon stopped: same command prints `ok` and exits 0 with no stderr
  output. With socket path made unwritable (chmod 000): exits 1 with a
  helpful stderr message.
- **Priority**: Must
- **Related Stories**: TBD

### FR-5: Daemon-down kick must not block the keybind shell
- **Description**: `media-control kick` MUST NOT block longer than 100ms
  total per invocation across all error paths. Use connectionless `sendto`
  (datagram sockets are non-blocking for delivery semantics by nature; no
  `SOCK_STREAM` connect handshake to wait on).
- **Acceptance Criteria**: With daemon stopped, `time media-control kick`
  reports wall time < 100ms (p99 over 1000 invocations). With socket file
  removed entirely, same bound holds.
- **Priority**: Must
- **Related Stories**: TBD

### FR-6: Replace 9 Hyprland keybinds in `~/.config/hypr/conf.d/common.conf`
- **Description**: Swap `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"`
  for `&& media-control kick` in every `layoutmsg`-prefixed bind. The 9
  binds were added 2026-05-01 in this session; their text body is grepable
  by `media-avoider-trigger.fifo`.
- **Acceptance Criteria**: After change, `grep -c media-avoider-trigger.fifo
  ~/.config/hypr/conf.d/common.conf` returns 0. After `hyprctl reload`,
  pressing each of the 9 keybinds produces a `Processing trigger` log line
  in the daemon journal within ~50ms.
- **Priority**: Must
- **Related Stories**: TBD

### FR-7: Delete dead `.socket` unit from NixOS module
- **Description**: Remove the
  `systemd.user.sockets.media-control-daemon` block from
  `~/nix/modules/apps/media/media-control.nix:48-53`. The `.service` unit
  remains. The daemon binds its own socket at startup; systemd doesn't need
  to hold the FD.
- **Acceptance Criteria**: After change, `systemctl --user list-sockets |
  grep media-control` returns empty. `nixos-rebuild switch --flake
  ~/nix#malphas` succeeds. `systemctl --user status
  media-control-daemon.service` shows `active (running)` after restart, with
  no socket-restart loop.
- **Priority**: Must
- **Related Stories**: TBD

### FR-8: Migration safety — daemon-side FIFO cleanup
- **Description**: On daemon startup, best-effort `unlink` the legacy FIFO
  at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`. Failure to unlink is
  logged at `debug` and ignored (it's not the new daemon's path). Cleanup
  lives in the daemon (not the CLI) because the daemon owns the IPC
  lifecycle. CLAUDE.md and readme.md MUST be updated to reflect the new
  transport.
- **Acceptance Criteria**: Starting the new daemon on a host with a stale
  FIFO at the old path: FIFO is gone after daemon reaches "ready" log line.
  Starting it on a host without the stale FIFO: no error, no warn-level
  log entry.
- **Priority**: Should

### FR-9: Reserved version-byte wire format for future extensibility
- **Description**: The datagram wire format is defined as follows:
  - **Length 0**: canonical "re-evaluate" kick. MUST always be valid.
  - **Length ≥ 1**: byte 0 is the protocol version. `0x01` is reserved for
    a future v1 envelope (likely UTF-8 JSON). All other version bytes are
    reserved.
  - In this release the daemon ignores all non-empty datagrams and emits a
    single `debug!` log line (`Ignoring vN datagram (unsupported in this
    release)`) per receipt.
  - The CLI MUST NOT expose any flag that would generate a non-empty
    datagram in this release; `--reason` and friends are reserved for the
    v1 envelope and are not parsed today.
- **Acceptance Criteria**: A 0-byte datagram triggers an avoid pass. A
  1-byte datagram with payload `0x01` produces a `debug!` log line and no
  trigger. A 1-byte datagram with payload `0xFF` produces a `debug!` log
  line and no trigger. The CLI does not accept any payload-shaping flags
  (test: `media-control kick --reason foo` exits non-zero with "unrecognized
  option").
- **Priority**: Must (locks the wire contract before users start scripting
  against it)

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Kick latency end-to-end | Wall time from `media-control kick` invocation to daemon `Processing trigger` log line | < 50ms p95 on a healthy system |
| `kick` invocation cost | CLI process start + sendto + exit | < 100ms p99 (binds the shell-blocking concern in FR-5) |
| Recv-error backoff | CPU usage during a sustained `recv_from` error storm | ≤ 10% on the daemon thread |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| No keybind shell hang under any daemon state | `time media-control kick` with daemon down / socket missing / socket non-writable | All cases ≤ 100ms wall time |
| Per-event coalescing preserved | 100 rapid kicks → daemon avoid passes | ≤ 101 (existing channel-coalesce contract) |
| Symlink/wrong-owner attack surface closed | Pre-positioned symlink or wrong-uid socket at the bind path | Daemon refuses to start; non-zero exit |
| Existing tests survive transport swap | `cargo test --workspace --all-features` | green |
| Nix package still builds | `nix build .#default` | green |

### Maintainability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Single source of truth for the socket path | Both daemon and CLI resolve via the same lib helper | One `pub fn` in `media-control-lib`; one constant for the filename |
| Test posture survives nix-sandbox tmpfs flakiness | Socket tests use `is_socket()` + bind-success assertions, **not** inode equality | (Per `media-control-t8d` lessons.) Tests must not reintroduce inode-equality assumptions |
| Reduced LOC in `crates/media-control-daemon/src/main.rs` | After deletion of FIFO functions and addition of socket equivalents | Net negative LOC delta in main.rs (sanity check, not a hard target) |

### Security
| Requirement | Standard | Notes |
|-------------|----------|-------|
| Filesystem access control | UNIX mode `0o600` | Single-user. Future shared-group access (`0o660`) deferred until a real caller exists. |
| TOCTOU defense at bind path | `lstat` → reject-if-symlink → reject-if-not-socket → reject-if-not-ours → unlink → bind | Mirrors `create_fifo_at` posture. Symlink-resistance is the lethal one. |

### Observability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Per-trigger log discoverability | Daemon log line shape | `Received datagram trigger` → `Processing trigger` (replaces today's `Received FIFO trigger`/`Processing FIFO trigger`) |
| Migration-FIFO cleanup is visible at debug, silent at info | journal under default log level | No new info-level log entries from FIFO cleanup |

---

## Constraints

### Technical Constraints
- **Hyprland 0.54.3** is the current target. `layoutmsg` emits no socket
  events; `media-control kick` is the only way to wake the daemon from a
  layout reshuffle. Any kick-reliability regression is user-visible
  immediately.
- **Test isolation in nix sandbox.** Per `media-control-t8d` lessons, do
  not assert on inode-level identity in socket tests. Assert on file type
  via `is_socket()`, on bind success, and on reject-symlink behaviour.
- **`media-control kick` MUST work without the daemon running.** The CLI
  binary is stand-alone; "daemon down" is not a kick failure for keybind
  callers.
- **Single coordinated release.** Daemon socket transport, CLI kick
  subcommand, keybind migration in `~/.config/hypr/conf.d/common.conf`,
  and `~/nix` module update all land together (per Q7 / FR-1..7
  collectively). No intermediate state where the new keybinds reference a
  CLI subcommand that doesn't exist yet.
- **No new runtime crates required.** `tokio::net::UnixDatagram` is
  already transitively available; no `libsystemd` (per non-goal: no
  `sd_listen_fds()`).

### Business Constraints
- Single-author rollout window. Land in one release on `malphas`.
- Cross-repo coordination required: `media-control` (this repo),
  `~/nix-config` and/or `~/nix` (NixOS module), `~/.config/hypr` (user
  keybinds). Per Q5/Q6, all three are in scope for this intent's bolt
  plan.

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| `tokio::net::UnixDatagram::recv_from` reliably wakes on every received datagram (no edge-trigger surprises) | Daemon misses kicks under load → user-visible avoidance lag | Construction test: 1000 rapid kicks → ≥ 999 trigger evaluations recorded |
| Hyprland `exec` keybinds don't capture the kick CLI's exit code in a way that surfaces stderr | If they do, the silent-on-`ECONNREFUSED` behaviour might still produce a dunst notification | Manual smoke test on `malphas`: stop daemon, press keybind, verify no notification |
| `nixos-rebuild` with `.socket` unit removed doesn't leave a half-stale `media-control-daemon.socket` registered | systemd may keep the unit registered until daemon-reexec; could cause confusion | Document in inception-log; verify with `systemctl --user list-sockets` post-rebuild |
| The 9 keybind lines in `~/.config/hypr/conf.d/common.conf` are the only `media-avoider-trigger.fifo` callers in user config | A stray script somewhere else (cron, dotfile, etc.) breaks silently after FIFO removal | Pre-change: `rg media-avoider-trigger.fifo ~` — confirm 9 hits all in expected file |

---

## Out of Scope

- **Wiring `sd_listen_fds()`.** The simpler self-bind path is preferred
  over systemd FD-passing. Adding `libsystemd` or hand-rolled
  `LISTEN_FDS`/`LISTEN_PID` parsing is explicitly out of scope. Marginal
  benefit (lazy daemon startup) is negligible given the daemon's
  always-on role.
- **Turning the kick into a structured RPC.** v1 envelope (FR-9) is
  *reserved*, not designed. Any structured commands (`pause`, `unpause`,
  `reload-config`, `--reason "togglesplit"` telemetry) are deferred to a
  future intent — this intent only locks the wire format such that the
  future intent doesn't have to break compatibility.
- **Avoider per-event triggering on Hyprland socket2 events.** The
  deny-list / debounce / suppress logic is unchanged. This intent
  replaces the *external* trigger transport only.
- **The daemon's 90-second-stop-timeout bug** observed during deployment
  work on 2026-05-01 (`Daemon stopped cleanly` logged, but process
  doesn't exit until systemd's `stop-sigterm` timer fires SIGKILL). Its
  own ticket. May be related to the AbortOnDrop-on-FIFO-listener pattern
  (intent 017's discovered side issue), but a fix is out of scope here
  — the FIFO listener is being deleted regardless, so the issue may
  resolve incidentally.
- **Multi-user / shared-group socket access** (`0o660` permissions). No
  current caller. `0o600` matches the FIFO and is the conservative
  default.
- **Telemetry for kicks** (which keybind triggered, etc.). FR-9 reserves
  the wire format for it; the implementation is for a future intent.

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| ~~Should `kick` accept `--reason` for telemetry?~~ | — | — | **Resolved 2026-05-03**: No today. Wire format reserved (FR-9). Future intent adds the flag + v1 envelope. |
| ~~FIFO migration cleanup location?~~ | — | — | **Resolved 2026-05-03**: Daemon-side (FR-8). |
| ~~Listener task naming / shape?~~ | — | — | **Resolved 2026-05-03**: Keep AbortOnDrop shape, rename `fifo_listener_handle` → `dgram_listener_handle`. |
| ~~Permissions: 0o600 vs 0o660?~~ | — | — | **Resolved 2026-05-03**: `0o600`. Defer 0o660 until a real shared caller exists. |
| ~~`~/nix` and `~/.config/hypr` in scope?~~ | — | — | **Resolved 2026-05-03**: Yes, both in scope; bolt plan covers all three repos. |
| ~~Single bolt vs split bolts?~~ | — | — | **Resolved 2026-05-03**: Single coordinated rollout. Bolt plan to confirm whether one bolt or a tightly-sequenced pair (likely one bolt for the in-repo work; the cross-repo updates are mechanical follow-ups in the same release window). |
| Is the 5+ second daemon-stop hang resolved by deleting the FIFO listener, or does it persist? | Construction | Bolt validation stage | Pending — observe during validation; if persists, file a follow-up intent. |
| Should the lib helper expose `kick()` as a sync fn (CLI doesn't need tokio for one sendto) or keep async-symmetric with the rest of `media-control-lib`? | Construction | Bolt design stage | Pending — async-symmetric is consistent; sync is one less dep. Construction picks. |
