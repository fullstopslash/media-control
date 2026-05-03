---
id: 003-end-to-end-validation
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 031-rollout-migration
implemented: true
---

# Story: 003-end-to-end-validation

## User Story

**As an** intent owner about to declare 018 done
**I want** to run the full DoD validation matrix on `malphas` end-to-end
**So that** the user-visible payoff (no keybind hangs, faster recovery, cleaner systemd surface) is demonstrably in place — not just unit-tested — and any incidental side-effects (like the daemon-stop hang from intent 017) are observed and recorded

## Acceptance Criteria

- [ ] **Given** the new daemon is running and the migrated keybinds are loaded, **When** I press each of the 9 layoutmsg keybinds, **Then** each keypress produces a `Processing trigger` log line in `journalctl --user -u media-control-daemon.service` within ~50ms (FR-2 + FR-6 end-to-end)
- [ ] **Given** the daemon is forcefully stopped (`pkill -KILL media-control-daemon`), **When** I press a keybind, **Then** the keybind shell exits within 100ms with no dunst notification, no shell hang, and no error visible to the user (FR-5 end-to-end)
- [ ] **Given** the post-018 release, **When** I run `cargo test --workspace --all-features`, **Then** all tests pass green
- [ ] **Given** the post-018 release, **When** I run `nix build .#default`, **Then** the build succeeds
- [ ] **Given** the post-018 keybind file, **When** I `grep media-avoider-trigger.fifo ~/.config/hypr/conf.d/common.conf`, **Then** zero matches (FR-6 verification)
- [ ] **Given** the post-018 nix module, **When** I `grep media-control-daemon.socket ~/nix/modules/apps/media/media-control.nix`, **Then** zero matches (FR-7 verification)
- [ ] **Given** the post-rebuild systemd state, **When** I run `systemctl --user list-sockets | grep media-control`, **Then** zero matches (FR-7 verification)
- [ ] **Given** the post-rebuild systemd state, **When** I run `systemctl --user status media-control-daemon.service`, **Then** the unit is `active (running)`
- [ ] **Given** the new daemon is running, **When** I run `ss -lx | grep media-control-daemon.sock`, **Then** the daemon's own bound socket appears as `u_dgr` listener (proving daemon-side bind)
- [ ] **Given** the daemon was started against a host with a stale legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`, **When** I check the runtime dir post-startup, **Then** the FIFO is gone (FR-8 verification — set this up by `mkfifo $XDG_RUNTIME_DIR/media-avoider-trigger.fifo` before daemon restart)
- [ ] **Given** the daemon is running, **When** I observe a `systemctl --user stop media-control-daemon` and time the SIGTERM-to-actual-exit duration, **Then** the duration is recorded — if < 5s, intent 017's discovered side issue is **resolved incidentally**; if ≥ 5s, the issue **persists** and a follow-up intent is filed

## Technical Notes

- This story is a validation gate, not a code change. Run sequentially through the matrix; record results in `inception-log.md` Decision Log.
- For the keybind smoke test, run a `journalctl --user -u media-control-daemon.service -f` in one terminal and press the 9 keybinds in another. Each should produce `Processing trigger` (or whatever exact log shape lands per story 002's renamed log line — `Received datagram trigger` → `Processing trigger`).
- For the daemon-down silent test:
  ```
  pkill -KILL media-control-daemon
  ss -lx | grep media-control-daemon.sock  # confirm gone
  time hyprctl dispatch layoutmsg togglesplit && media-control kick
  echo $?  # expect 0
  ```
  Verify dunst log shows no notification. Verify shell didn't hang.
- For the FIFO cleanup verification:
  ```
  systemctl --user stop media-control-daemon
  mkfifo $XDG_RUNTIME_DIR/media-avoider-trigger.fifo
  ls -la $XDG_RUNTIME_DIR/media-avoider-trigger.fifo  # should show p (FIFO type)
  systemctl --user start media-control-daemon
  ls -la $XDG_RUNTIME_DIR/media-avoider-trigger.fifo  # expect: No such file or directory
  ```
- For the daemon-stop-hang investigation:
  ```
  time systemctl --user stop media-control-daemon
  ```
  - If wall time ≪ 5s: log "FIFO listener removal resolved the daemon-stop hang from intent 017's discovered side issues" in inception-log.
  - If wall time ≥ 5s: log "Daemon-stop hang persists post-018; not the FIFO listener. File follow-up intent."
- All validation results go into the inception-log Decision Log table with timestamp + outcome. The bolt's construction-log will reference this story's results.

## Dependencies

### Requires

- 001-hyprland-keybind-migration (validates the migrated keybinds work)
- 002-nixos-module-cleanup (validates the nix-side cleanup landed and the daemon binds its own socket post-rebuild)

### Enables

- Inception complete; transition to operations / closing the intent

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| One of the 9 keybinds doesn't trigger (silent failure) | Investigate: which keybind? does `media-control kick` work standalone? does the keybind's `exec` chain actually run? — likely a typo from story 001's edit; expand story 001's scope to fix |
| `cargo test` fails on a host-specific issue (e.g. socket path conflict with a leftover daemon) | Retry after ensuring no daemon is running; if persists, the test isolation needs hardening (file follow-up) |
| `nix build .#default` fails because of a flake.lock mismatch | Update flake.lock, retry. Could indicate the post-018 release wasn't pushed before validation |
| The legacy FIFO cleanup test shows the FIFO still present after restart | Story 004 (FR-8) didn't land or the cleanup runs in the wrong order — investigate `main.rs` startup sequence |
| The daemon-stop hang shows different timing (e.g. 30s instead of 90s) | Record the new timing; "different but still hung" warrants its own follow-up intent rather than declaring incidentally-resolved |
| dunst shows a notification I didn't expect during the daemon-down test | Confirm the notification source — could be unrelated to this intent (some other daemon-related notify-send call); document and continue |

## Out of Scope

- Performance benchmarking beyond the qualitative DoD targets — explicit numbers (~50ms, ~100ms) are sanity checks, not regression targets.
- Stress testing the daemon under high keybind load (the channel coalescing is unit-tested in story 002).
- Cross-host validation — single-author single-host rollout per the intent's business constraint.
- Long-burn-in validation (24-hour soak test, etc.) — out of scope for this intent's DoD; if a regression surfaces in normal use, it's a follow-up.
