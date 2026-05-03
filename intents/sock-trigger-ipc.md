---
intent: sock-trigger-ipc
phase: inception
status: draft
created: 2026-05-01
---

# Intent: Replace FIFO trigger IPC with `SOCK_DGRAM` UNIX socket

Consolidate the daemon's external-trigger IPC onto a single UNIX domain
socket (SOCK_DGRAM). Today the daemon binds a FIFO at
`$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` for manual avoid kicks while
the systemd `.socket` unit at `%t/media-control-daemon.sock` exists but is
**dead code** — the daemon never accepts on it. This intent unifies both
into one transport, removes the dead `.socket` unit (or wires it through
properly), and adds a first-class `media-control kick` CLI subcommand so
keybinds and scripts no longer have to know the transport's path.

## Motivation

1. **Bifurcation is misleading.** The `.socket` unit looks like it does
   something; it doesn't. Anyone reading the systemd config for the first
   time has to grep the daemon source to discover the FIFO is the only
   live trigger transport.
2. **FIFO has a real footgun.** `echo > $fifo` from a Hyprland keybind
   blocks indefinitely if no reader is open (i.e. daemon is down,
   restarting, or the process is wedged). A single bad daemon state can
   freeze the user's keybind shell.
3. **`SOCK_DGRAM` semantics fit the trigger model better.** Datagram
   delivery is one syscall, never blocks the writer waiting for a reader,
   drops cleanly when the daemon isn't up. Failure mode is "the kick was
   silently lost" instead of "the keybind is now wedged."
4. **Consolidating IPC pays a one-time complexity cost** to delete a
   permanently-confusing artifact (the unused `.socket` unit) and tighten
   the daemon's external surface to one well-documented entry point.

## Background — current state (frozen as of f83d109)

### Code paths to FIFO (daemon side)

- `crates/media-control-daemon/src/main.rs`:
  - `get_fifo_path()` — line 235 — resolves `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`
  - `create_fifo_at()` — line 394 — `mkfifo` + symlink-rejection + ownership-rejection + 0o600 permission enforcement (~46 LOC)
  - `create_fifo()` / `remove_fifo()` — wrappers (~13 LOC)
  - `fifo_listener()` — line 475 — async loop that opens FIFO, `read_line`s into an `mpsc::Sender<()>`, with backoff on errors (~59 LOC)
  - `run_event_loop()` — line ~640 — `tokio::select!` arm that drains `fifo_rx` and calls `trigger_avoid`
- 4 unit tests in `tests` mod targeting `create_fifo_at` (fresh-path, symlink, regular-file, replace-our-own)

### Systemd units (frozen)

- `~/nix/modules/apps/media/media-control.nix:48-53` declares
  `systemd.user.sockets.media-control-daemon` with
  `ListenStream = "%t/media-control-daemon.sock"` and `SocketMode = "0600"`.
  The `.service` ExecStart is `media-control-daemon foreground` — the
  daemon does **not** call `sd_listen_fds()`, has no `libsystemd` dep, and
  has no `LISTEN_FDS`/`LISTEN_PID` env handling. The socket is bound by
  systemd but never accepted by the daemon. Confirmed empirically: `ss -lx
  | grep media-control-daemon` returns no listener while the service is
  active.

### Hyprland keybinds (frozen)

In `~/.config/hypr/conf.d/common.conf` (around line 372) — 9 layoutmsg
binds currently chain `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"`
after the `hyprctl dispatch` because `layoutmsg` does not emit
`movewindow`/`resizewindow` events on Hyprland 0.54.3 and the daemon would
otherwise miss the layout reshuffle. These are the production callers of
the FIFO outside the daemon itself.

### What the existing tests assert

- "FIFO is created at fresh path" — file-type check
- "rejects symlink" — symlink-resistance contract (TOCTOU defense)
- "rejects regular file" — type-check rejection
- "replaces our own existing FIFO" — recreate-on-restart contract; current
  implementation removes-then-mkfifos. **Note**: this test was recently
  destabilised by tmpfs inode reuse in the nix sandbox and rewritten to
  drop the inode comparison; the equivalent socket test should not
  reintroduce the inode-equality assumption.

## Target state

1. Daemon binds **one** `UnixDatagram` socket at
   `$XDG_RUNTIME_DIR/media-control-daemon.sock` at startup, accepts
   datagrams in a `dgram_listener()` task that pushes idempotent trigger
   pulses into the same `mpsc<()>` channel `fifo_rx` currently feeds.
2. Daemon code drops `mkfifo` / `remove_fifo` / `fifo_listener`
   functions and the four FIFO-specific tests are replaced with the
   socket equivalents (rebind-on-restart, reject-symlink-at-path,
   reject-regular-file-at-path, ownership-check).
3. New `media-control kick` CLI subcommand sends a single byte via
   `UnixDatagram::sendto` to the daemon socket. Exit code 0 on
   `Ok`/`ECONNREFUSED` (silent — daemon-down is not a user error in the
   keybind context); exit code 1 with stderr message on any other
   `sendto` error so `media-control kick` from a script can still be
   debugged.
4. Hyprland keybinds in `~/.config/hypr/conf.d/common.conf` (9 lines)
   replace `&& echo > "$XDG_RUNTIME_DIR/...fifo"` with `&& media-control kick`.
5. Dead `systemd.user.sockets.media-control-daemon` block removed from
   `~/nix/modules/apps/media/media-control.nix`. The socket is now bound
   by the daemon process itself; systemd doesn't need to hold the FD.

## Functional requirements

### FR-1: Daemon binds a single SOCK_DGRAM socket at startup
Path: `$XDG_RUNTIME_DIR/media-control-daemon.sock`. Mode 0o600.
On a stale socket from a previous run: lstat → reject if symlink → reject
if not a socket → reject if uid != ours → unlink → bind. Mirrors the
existing `create_fifo_at` safety posture. **MUST not** follow symlinks.

### FR-2: Daemon accepts trigger datagrams without parsing payload
Any datagram, regardless of content, is treated as a single idempotent
"re-evaluate placement" pulse. Coalesce into the existing trigger channel
the same way `fifo_listener` does (try_send + drop on full channel).
Datagram payload is reserved for future structured commands; a 0-byte
payload is the canonical "kick".

### FR-3: Daemon recovers from socket errors with bounded backoff
On `recv_from` error other than `WouldBlock`: log at `warn`, sleep
`SOCKET_ERROR_BACKOFF` (~100ms, mirroring `FIFO_ERROR_BACKOFF`), continue.
The socket FD is held for the daemon's lifetime — no per-message rebind.
On bind failure at startup: log `error`, exit non-zero (same severity as
the current `create_fifo` failure path).

### FR-4: `media-control kick` CLI subcommand
Connectionless `sendto` of 0 bytes to the daemon socket. Exit code:
- `0` on `Ok`
- `0` on `ECONNREFUSED` / `ENOENT` (daemon not running — silent for
  keybind ergonomics)
- `1` with stderr message on any other error (permission denied, path
  validation failure, etc.)
This subcommand resolves the socket path via the same
`runtime_dir().join("media-control-daemon.sock")` helper the daemon uses,
so a single rename later only touches one constant.

### FR-5: Daemon-down kick must not block the keybind shell
Connect or sendto with a non-blocking flag, or rely on
`UnixDatagram::sendto`'s native non-blocking semantics for connectionless
sockets. **MUST NOT** block longer than 100ms total per kick attempt
across all error paths.

### FR-6: Replace 9 Hyprland keybinds in `~/.config/hypr/conf.d/common.conf`
Swap `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` for
`&& media-control kick` in every layoutmsg-prefixed bind. The 9 binds were
added 2026-05-01 in this session; their text body is grepable by
`media-avoider-trigger.fifo`.

### FR-7: Delete dead `.socket` unit from NixOS module
Remove the `systemd.user.sockets.media-control-daemon` block from
`~/nix/modules/apps/media/media-control.nix:48-53`. The `.service` unit
remains (still socket-activated by Hyprland-session.target dependency).
No new socket dependency required; the daemon binds its own socket on
startup. `nixos-rebuild` must succeed against this change without a
service restart loop.

### FR-8: Migration safety
On daemon startup against an existing FIFO at the OLD path
(`media-avoider-trigger.fifo`), best-effort `unlink` it so the user
isn't left with a stale FIFO file from a previous `0.1.7` daemon.
Failure to unlink is logged at `debug` and ignored — it's not the new
daemon's path. Document the FIFO removal in CLAUDE.md / readme.md.

## Non-goals

- **NOT** wiring `sd_listen_fds()`. The simpler self-bind path is
  preferred over the systemd FD-passing path. Adding `libsystemd` or
  hand-rolled `LISTEN_FDS`/`LISTEN_PID` parsing is explicitly out of
  scope; the marginal benefit (lazy daemon startup) is negligible since
  the daemon's primary job is to listen continuously.
- **NOT** turning the kick into a structured RPC. The single byte
  "re-evaluate" trigger is sufficient. Future commands (e.g. `pause`,
  `unpause`, `reload-config`) would need a real protocol; this intent
  pre-empts none of those design choices — a 0-length datagram is the
  reserved-for-now "kick" payload.
- **NOT** changing the avoider's per-event triggering on Hyprland
  socket2 events. The deny-list / debounce / suppress logic is unchanged.
- **NOT** tackling the daemon's 90-second-stop-timeout bug observed
  during deployment work on 2026-05-01 (logs `Daemon stopped cleanly`
  but process doesn't exit until systemd's timeout fires SIGKILL). That
  is its own ticket.

## Key constraints

- **Hyprland 0.54.3** is the current target. `layoutmsg` emits no socket
  events; the kick is the only way to wake the daemon from a layout
  reshuffle. Any regression in kick reliability is user-visible
  immediately.
- **Test isolation in nix sandbox.** The same tmpfs flakiness that
  destabilised `create_fifo_at_replaces_our_own_existing_fifo` will
  affect socket tests if they assert on inode-level identity. Tests
  should assert on file type (`is_socket()`), bind success, and
  reject-symlink behaviour — not on inode equality.
- **`media-control kick` must work without the daemon running.** The
  CLI binary is stand-alone; "daemon down" is not a kick failure for
  keybind callers.
- **Rollout sequencing.** The keybind change uses `media-control kick`,
  which doesn't exist until the new release ships. Land the daemon
  socket transport + new subcommand in the same PR to avoid an
  intermediate broken state.

## Touchpoints (paths frozen as of `f83d109`)

| File | Change |
|---|---|
| `crates/media-control-daemon/src/main.rs` | Replace FIFO functions + listener; rename `fifo_rx` → `dgram_rx`; update tests |
| `crates/media-control/src/main.rs` | Add `Kick` variant to `Commands` enum, route to new lib helper |
| `crates/media-control-lib/src/commands/<new>.rs` (or extend `mod.rs`) | New `kick()` async fn — opens UnixDatagram, sends, handles errors per FR-4 |
| `crates/media-control-lib/src/commands/mod.rs` | Re-export `kick` |
| `~/.config/hypr/conf.d/common.conf` | 9 keybinds: `echo > $fifo` → `media-control kick` |
| `~/nix/modules/apps/media/media-control.nix` | Delete `systemd.user.sockets.media-control-daemon` block |
| `CLAUDE.md`, `readme.md`, daemon docstring | Reflect the new transport |

## Open questions for the inception agent

1. Should the `kick` subcommand take an optional argument (e.g.
   `--reason "togglesplit"`) for future telemetry, even though the
   daemon ignores payload today? If yes, datagram body becomes a
   reserved-for-text channel with a version byte; if no, zero-byte is
   the only sanctioned payload and the protocol can be evolved in
   v0.2.x with a magic-byte prefix.
2. Should the migration FIFO-cleanup (FR-8) live in the daemon's
   `create_socket_at` path or in the `kick` CLI's first run? Daemon-
   side is simpler; CLI-side gives a no-op upgrade path even when the
   new daemon hasn't been restarted yet.
3. Do we also retire the (currently exposed) `fifo_listener_handle`
   AbortOnDrop pattern in `run_event_loop`, or keep the same
   spawn/abort-on-drop shape with the renamed task? Mechanical, but
   names will leak into git blame either way.
4. Permission policy: the FIFO is currently `0o600`. Should the
   socket be `0o600` (matches FIFO) or `0o660` for future shared-group
   trigger access? Default to `0o600`.

## Definition of done

- All 9 layoutmsg keybinds work via `media-control kick` against a
  running daemon — verified end-to-end on `malphas` with the existing
  test harness (debug-log the daemon, press each keybind, observe
  `Received datagram trigger` in journal within ~50ms).
- `pkill -KILL media-control-daemon` followed by a keybind press: the
  CLI exits 0 silently (no dunst spam, no shell hang).
- `cargo test --workspace --all-features` green.
- `nix build .#default` green (with `doCheck = true` if the existing
  parallel-test isolation issues from `media-control-t8d` are also
  fixed; otherwise `doCheck = false` is preserved as today).
- `~/.config/hypr/conf.d/common.conf` does not contain the string
  `media-avoider-trigger.fifo`.
- `~/nix/modules/apps/media/media-control.nix` does not contain
  `systemd.user.sockets.media-control-daemon`.
- `nixos-rebuild switch --flake ~/nix#malphas` succeeds; daemon
  restarts cleanly into the new transport.

## Reference: live diagnostics from the 2026-05-01 session

- Daemon log line shape today: `DEBUG Received FIFO trigger` →
  `DEBUG Processing FIFO trigger`. After this work, expect:
  `DEBUG Received datagram trigger` (or similar) → `DEBUG Processing
  trigger`.
- Two confirmed empirical findings (do not re-investigate):
  - Hyprland 0.54.3 emits **zero** socket events on
    `dispatch layoutmsg togglesplit`. Avoid relies entirely on the
    kick path for these.
  - The current FIFO writer-blocks-on-no-reader hazard is real and
    user-visible: `echo > $fifo` hangs indefinitely if the daemon is
    down. SOCK_DGRAM eliminates this.
