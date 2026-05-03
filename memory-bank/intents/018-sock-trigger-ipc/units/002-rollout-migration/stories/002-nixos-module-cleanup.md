---
id: 002-nixos-module-cleanup
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 031-rollout-migration
implemented: true
---

# Story: 002-nixos-module-cleanup

## User Story

**As a** future reader of the NixOS configuration for `media-control-daemon`
**I want** the dead `systemd.user.sockets.media-control-daemon` block removed
**So that** the systemd config truthfully reflects the daemon's IPC surface (it now binds its own socket; no `.socket` unit is needed) and nobody has to grep daemon source to discover the discrepancy — directly resolving the "bifurcation is misleading" motivator from the intent

## Acceptance Criteria

- [ ] **Given** the pre-edit `~/nix/modules/apps/media/media-control.nix`, **When** I look at lines 48-53 (as of `f83d109`), **Then** I see the `systemd.user.sockets.media-control-daemon` block with `ListenStream = "%t/media-control-daemon.sock"`
- [ ] **Given** the post-edit module, **When** I `grep media-control-daemon.socket ~/nix/modules/apps/media/media-control.nix`, **Then** zero matches
- [ ] **Given** the post-edit module, **When** I `grep -A2 'systemd.user.sockets' ~/nix/modules/apps/media/media-control.nix`, **Then** no `media-control-daemon` block appears (other socket units, if any, remain undisturbed)
- [ ] **Given** the post-edit module, **When** I run `nixos-rebuild switch --flake ~/nix#malphas`, **Then** the rebuild succeeds without errors
- [ ] **Given** the rebuild succeeds, **When** I run `systemctl --user list-sockets`, **Then** no `media-control-daemon.socket` entry appears
- [ ] **Given** the rebuild succeeds, **When** I run `systemctl --user status media-control-daemon.service`, **Then** the unit is `active (running)` with no socket-restart loop and no `Failed to start` errors in journalctl during the post-rebuild minute
- [ ] **Given** the daemon is running post-rebuild, **When** I run `ss -lx | grep media-control-daemon.sock`, **Then** the daemon's own bound socket appears (proving the daemon binds it itself, not via systemd FD-passing)

## Technical Notes

- The block to remove (per intent draft, lines 48-53 of `~/nix/modules/apps/media/media-control.nix`):
  ```nix
  systemd.user.sockets.media-control-daemon = {
    ...
    ListenStream = "%t/media-control-daemon.sock";
    SocketMode = "0600";
    ...
  };
  ```
  Exact lines may differ — read the file, identify the block, delete cleanly. Preserve surrounding whitespace and any comments referencing the service unit.
- The `.service` unit MUST remain. The daemon is still socket-activated only in the loose sense (Hyprland-session.target dependency); systemd doesn't hold an FD for it.
- `nixos-rebuild switch --flake ~/nix#malphas` applies the change. If the rebuild fails (syntax error, evaluation error), revert the edit, fix, retry. `nixos-rebuild --rollback` is available as a safety net post-switch if the system enters a bad state.
- Per global CLAUDE.md: this targets `~/nix` (the live deployment), NOT `~/nix-config` (the work-in-progress port). Cross-port can happen separately.
- Validation depends on the new daemon binary (Unit 1) being installed by this rebuild — if the rebuild's `media-control` package isn't pinned to the post-018 version, the running daemon won't bind its own socket, and the `ss -lx` check will fail. Confirm package version (Cargo.toml workspace version reflects the bumped value from `just commit`) is the post-018 release.
- Note: Once the `.socket` unit is removed, systemd may keep it registered until daemon-reexec. If `systemctl --user list-sockets` still shows it after rebuild, run `systemctl --user daemon-reexec` to flush.

## Dependencies

### Requires

- Unit 1 (`001-socket-transport`) shipped and the new daemon binary present in the nix store before this rebuild lands
- 001-hyprland-keybind-migration (sequencing: keybinds first, module cleanup second; this way the daemon-down test in story 003 can be done with the new keybinds and the new daemon both in place)

### Enables

- 003-end-to-end-validation (DoD checks `systemctl --user list-sockets | grep media-control` is empty post-rebuild)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `nixos-rebuild` fails because the `media-control` package version isn't bumped | Diagnose: confirm `just commit` bumped the version and pushed; the nix flake input may need updating to pull the new version |
| Removing the `.socket` block also removes a `Wants=`/`Requires=` reference elsewhere | Read the broader module; if other units reference the socket, restructure those references too (likely they don't, since the socket is dead code today) |
| Post-rebuild, the daemon's `media-control-daemon.service` enters a restart loop | Check `journalctl --user -u media-control-daemon.service -e` — likely the new daemon's bind path fails (story 001 acceptance criterion not met). Roll back rebuild, investigate. |
| `~/nix-config` (the parallel WIP port) still has the dead socket block | Out-of-band per CLAUDE.md scoping; may sync later as a separate change |
| The `nixos-rebuild` finds an unrelated change in `~/nix` (e.g. ongoing user work) | This story is for the surgical socket-block removal only; broader rebuild concerns are separate. If unrelated changes exist, consider committing them first or stashing |

## Out of Scope

- Updating `~/nix-config` (work-in-progress port).
- Restructuring other systemd unit declarations in the module beyond the socket block deletion.
- Pinning the `media-control` package to a specific version (the rebuild picks up whatever's in flake.lock; ensure flake.lock reflects the post-018 release before this story runs).
- Documenting the change in any nix-side changelog or release notes.
