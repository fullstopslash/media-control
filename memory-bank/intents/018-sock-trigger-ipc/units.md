---
intent: 018-sock-trigger-ipc
phase: inception
status: units-decomposed
created: 2026-05-03T15:44:51Z
updated: 2026-05-03T15:44:51Z
---

# Replace FIFO trigger IPC with `SOCK_DGRAM` — Unit Decomposition

## Units Overview

This intent decomposes into **2 units**, executed sequentially:

- **Unit 1 (`001-socket-transport`)** — All in-repo Rust changes: lib helpers, daemon socket binding, dgram listener, CLI `kick` subcommand, FIFO migration cleanup, docs. Produces a release-ready `media-control` workspace where the new transport is live and the legacy FIFO is gone — but no external callers have been migrated yet.
- **Unit 2 (`002-rollout-migration`)** — Cross-repo activation: 9 Hyprland keybinds in `~/.config/hypr/conf.d/common.conf`, deletion of the dead `.socket` unit from `~/nix/modules/apps/media/media-control.nix`, and end-to-end DoD validation on `malphas`. Cannot start until Unit 1 ships (the keybind migration depends on the `kick` subcommand existing).

The two-unit split matches Q7's "single coordinated rollout" while keeping responsibility boundaries clean: Unit 1 is reviewed/tested as a workspace change; Unit 2 is reviewed/validated as a deployment + config change.

---

### Unit 1: 001-socket-transport

**Description**: In-repo work. Add `media-control-lib` helpers (`socket_path()`, `kick()`), bind a `SOCK_DGRAM` socket in the daemon at startup with TOCTOU-safe creation (mirroring `create_fifo_at`), replace the `fifo_listener` task with a `dgram_listener` task that funnels into the existing `mpsc<()>` channel, add a `media-control kick` CLI subcommand, perform best-effort cleanup of the legacy FIFO at daemon startup, and update CLAUDE.md / readme.md / daemon docstring. Wire format is locked per FR-9 (0-byte = canonical kick; non-empty = reserved/ignored with `debug!` log).

**Stories** (5):

- `001-daemon-binds-sock-dgram` — Add the `socket_path()` lib helper. Daemon binds `UnixDatagram` at startup with TOCTOU-safe creation: lstat → reject-symlink → reject-non-socket → reject-wrong-uid → unlink → bind. Mode `0o600`. Bind failure → log `error`, exit non-zero. (FR-1)
- `002-dgram-listener-replaces-fifo` — Replace the `fifo_listener` task with `dgram_listener`. `recv_from` loop pushes idempotent kicks into the existing `mpsc<()>` channel via `try_send`. Length-0 datagrams → kick. Length ≥ 1 datagrams → `debug!` log, ignore (FR-9). Recv errors other than `WouldBlock` → `warn!` + `SOCKET_ERROR_BACKOFF` (~100ms) sleep, continue. Rename `fifo_listener_handle` → `dgram_listener_handle`; keep AbortOnDrop shape (Q3). (FR-2, FR-3, FR-9)
- `003-cli-kick-subcommand` — Add `Kick` variant to `Commands` enum in `crates/media-control/src/main.rs`. Add `kick()` async fn in `media-control-lib` (uses `socket_path()` from story 001). Connectionless `sendto` of 0 bytes. Exit code: 0 on Ok / `ECONNREFUSED` / `ENOENT`; 1 on other errors with stderr message. p99 wall time < 100ms in all daemon states. CLI does NOT accept any payload-shaping flag. (FR-4, FR-5, FR-9 enforcement)
- `004-daemon-fifo-cleanup` — At daemon startup (after socket bind succeeds), best-effort `unlink` of the legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`. Failure → `debug!` log, ignore. Delete all FIFO-specific functions (`get_fifo_path`, `create_fifo_at`, `create_fifo`, `remove_fifo`, `fifo_listener`) and their 4 unit tests. (FR-8)
- `005-docs-update` — Update `CLAUDE.md` (project + global where relevant), `readme.md`, and the daemon's module docstring to reflect the new transport, the wire format reservation (FR-9), and the `media-control kick` CLI subcommand. Remove FIFO references.

**Deliverables**:

- New `socket_path()` and `kick()` in `crates/media-control-lib/`
- Daemon `main.rs`: socket binding, `dgram_listener`, FIFO cleanup, ~118 LOC of FIFO machinery deleted
- New `Commands::Kick` variant in `crates/media-control/src/main.rs`
- 4 new socket tests replacing the 4 FIFO tests (rebind-on-restart, reject-symlink, reject-regular-file, reject-wrong-uid) using `is_socket()` not inode equality
- New mock or integration test for `media-control kick` exercising daemon-down (silent exit 0), socket-non-writable (exit 1), and round-trip cases
- Updated docs

**Dependencies**:

- Depends on: None
- Depended by: `002-rollout-migration` (the keybind migration calls `media-control kick`)

**Estimated Complexity**: M (cohesive but touches three crates; tightly bounded by existing FIFO scaffolding shape)

---

### Unit 2: 002-rollout-migration

**Description**: Cross-repo activation. With Unit 1's release in hand, swap the 9 Hyprland keybinds from FIFO `echo` to `media-control kick`, remove the dead `systemd.user.sockets.media-control-daemon` block from the NixOS module, run `nixos-rebuild switch --flake ~/nix#malphas`, and validate the DoD end-to-end (debug-logged daemon, press each of the 9 keybinds, observe `Processing trigger` in journal within ~50ms; `pkill -KILL media-control-daemon` then keybind press exits silently).

**Stories** (3):

- `001-hyprland-keybind-migration` — Edit `~/.config/hypr/conf.d/common.conf`. Find the 9 layoutmsg keybinds (grep for `media-avoider-trigger.fifo`) and replace `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` with `&& media-control kick`. Verify post-edit: `grep -c media-avoider-trigger.fifo` returns 0. `hyprctl reload`. (FR-6)
- `002-nixos-module-cleanup` — Edit `~/nix/modules/apps/media/media-control.nix`. Remove the `systemd.user.sockets.media-control-daemon` block (lines 48-53 as of f83d109). The `.service` unit remains. `nixos-rebuild switch --flake ~/nix#malphas` succeeds. Verify post-rebuild: `systemctl --user list-sockets | grep media-control` returns empty; `media-control-daemon.service` is `active (running)`. (FR-7)
- `003-end-to-end-validation` — Run the DoD validation matrix on `malphas`: (a) press each of 9 layoutmsg keybinds, observe `Processing trigger` log line within ~50ms; (b) `pkill -KILL media-control-daemon`, press a keybind, verify silent exit 0 with no dunst notification and no shell hang; (c) `cargo test --workspace --all-features` green; (d) `nix build .#default` green; (e) `grep media-avoider-trigger.fifo ~/.config/hypr/conf.d/common.conf` returns nothing; (f) `grep media-control-daemon.socket ~/nix/modules/apps/media/media-control.nix` returns nothing.

**Deliverables**:

- 9-line diff to `~/.config/hypr/conf.d/common.conf`
- ~6-line deletion in `~/nix/modules/apps/media/media-control.nix`
- DoD validation log entry in inception-log
- Confirmation that the daemon-stop hang from intent 017's discovered side issues either resolves incidentally or persists (file follow-up intent if persists)

**Dependencies**:

- Depends on: `001-socket-transport` (the keybind migration requires `media-control kick` to exist; the nix module cleanup requires the daemon to bind its own socket)
- Depended by: None

**Estimated Complexity**: S (mechanical edits + validation; no design work)

---

## Requirement-to-Unit Mapping

| FR | Description | Unit |
|----|-------------|------|
| FR-1 | Daemon binds a single SOCK_DGRAM socket at startup | `001-socket-transport` (story 001) |
| FR-2 | Daemon accepts trigger datagrams without parsing payload | `001-socket-transport` (story 002) |
| FR-3 | Daemon recovers from socket errors with bounded backoff | `001-socket-transport` (story 002) |
| FR-4 | `media-control kick` CLI subcommand | `001-socket-transport` (story 003) |
| FR-5 | Daemon-down kick must not block the keybind shell | `001-socket-transport` (story 003) |
| FR-6 | Replace 9 Hyprland keybinds | `002-rollout-migration` (story 001) |
| FR-7 | Delete dead `.socket` unit from NixOS module | `002-rollout-migration` (story 002) |
| FR-8 | Migration safety — daemon-side FIFO cleanup | `001-socket-transport` (story 004) |
| FR-9 | Reserved version-byte wire format | `001-socket-transport` (story 002 + story 003) |

All 9 FRs accounted for.

## Unit Dependency Graph

```text
[001-socket-transport] ──> [002-rollout-migration]
```

Strict linear chain. Unit 2's keybind migration calls `media-control kick`, which only exists after Unit 1 ships.

## Execution Order

1. **001-socket-transport** — All in-repo work; produces a release-ready `media-control` workspace. Reviewed and merged as a single PR with `verify` (lint + test + build) green.
2. **002-rollout-migration** — Cross-repo activation; runs `nixos-rebuild` + `hyprctl reload` + DoD validation against the new release. Coordinated within the same release window per Q7 (no intermediate state where keybinds reference a non-existent CLI subcommand).
