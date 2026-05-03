---
id: 031-rollout-migration
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
type: simple-construction-bolt
status: in-progress
stories:
  - 001-hyprland-keybind-migration
  - 002-nixos-module-cleanup
  - 003-end-to-end-validation
created: 2026-05-03T15:44:51Z
started: 2026-05-03T16:45:54Z
completed: null
current_stage: plan
stages_completed: []

requires_bolts:
  - 030-socket-transport
enables_bolts: []
requires_units: []
blocks: true

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 3
---

# Bolt: 031-rollout-migration

## Overview

Cross-repo activation bolt for intent 018. With bolt 030's release in hand, swap 9 Hyprland keybinds from FIFO `echo` to `media-control kick`, remove the dead `systemd.user.sockets.media-control-daemon` block from the NixOS module, run `nixos-rebuild switch --flake ~/nix#malphas` and `hyprctl reload`, then execute the full DoD validation matrix on `malphas` end-to-end. Records the outcome of intent 017's daemon-stop-hang side issue (resolved incidentally vs. persists).

## Objective

Activate the new transport across all caller-side surfaces and verify the user-visible payoff. End state:

1. `~/.config/hypr/conf.d/common.conf` contains 0 references to `media-avoider-trigger.fifo` and 9 references to `media-control kick`.
2. `~/nix/modules/apps/media/media-control.nix` contains 0 references to `systemd.user.sockets.media-control-daemon`.
3. `nixos-rebuild switch --flake ~/nix#malphas` succeeds; `media-control-daemon.service` is `active (running)` post-rebuild; `systemctl --user list-sockets | grep media-control` is empty.
4. All 9 layoutmsg keybinds produce `Processing trigger` log lines within ~50ms.
5. Daemon-down keybind press exits silently in < 100ms (no shell hang, no dunst notification).
6. `cargo test --workspace --all-features` and `nix build .#default` are green.
7. Daemon-stop-hang outcome documented in inception-log.

## Stories Included

- **001-hyprland-keybind-migration** (Must) — Sed-replace 9 FIFO `echo` lines with `media-control kick`. Pre-edit `rg ~ media-avoider-trigger.fifo` confirms the 9 keybind lines are the only callers (story expands if other callers found). `hyprctl reload` post-edit.
- **002-nixos-module-cleanup** (Must) — Delete the `systemd.user.sockets.media-control-daemon` block (~6 lines) from `~/nix/modules/apps/media/media-control.nix`. `nixos-rebuild switch --flake ~/nix#malphas`. Verify `systemctl --user list-sockets | grep media-control` is empty post-rebuild.
- **003-end-to-end-validation** (Must) — Execute the DoD matrix on `malphas`: 9 keybinds → trigger logs; daemon-down silent kick; cargo test; nix build; grep checks for FIFO and socket-unit removal; daemon's own bind shows in `ss -lx`; legacy FIFO unlinked at daemon startup (FR-8); record daemon-stop-hang timing for the intent-017 side-issue follow-up decision.

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

Rationale: pure config edits + nixos-rebuild + validation matrix. No code, no domain modeling.

## Stages

- [ ] **1. plan**: Pending → implementation-plan.md (sequenced edits + validation procedure)
- [ ] **2. implement**: Pending → implementation-walkthrough.md (the actual edits + rebuild + reload)
- [ ] **3. test**: Pending → test-walkthrough.md (DoD validation matrix results)

## Dependencies

### Requires

- 030-socket-transport (the new daemon binary and `media-control kick` subcommand must exist and be installed before any caller-side migration is meaningful)

### Enables

- None (final bolt; intent complete after validation passes)

## Success Criteria

- [ ] Story 001 acceptance: 9 → 0 FIFO refs in keybind file; 0 → 9 `kick` refs; `hyprctl reload` clean
- [ ] Story 002 acceptance: 0 socket-unit refs in nix module; `nixos-rebuild switch` succeeds; `systemctl --user list-sockets | grep media-control` empty; daemon `active (running)` post-rebuild
- [ ] Story 003 acceptance: full DoD matrix passes (9 keybinds → trigger logs ≤ 50ms; daemon-down kick silent + ≤ 100ms; cargo test + nix build green; grep verifications clean; legacy FIFO unlinked on daemon startup)
- [ ] Daemon-stop-hang outcome recorded in `memory-bank/intents/018-sock-trigger-ipc/inception-log.md` Decision Log
- [ ] If daemon-stop hang persists post-018: follow-up intent filed (next intent number, ~`019-daemon-shutdown-hang`)

## Notes

### Sequencing within the bolt

1. Pre-flight: confirm bolt 030 release is installed; `which media-control` shows the new binary; `media-control kick --help` shows `kick` subcommand.
2. Story 001 (keybind edit) → `hyprctl reload` → smoke-test one keybind manually.
3. Story 002 (nix module edit) → `nixos-rebuild switch` → verify daemon restarts cleanly.
4. Story 003 (full DoD validation) → record results.

### Risk + rollback

- **`nixos-rebuild` failure mid-switch**: rare but possible. Use `nixos-rebuild --rollback` to recover. The block being removed is dead code, so the rollback risk is purely procedural (the system should be no-worse-off after rollback than before the rebuild).
- **Hyprland `hyprctl reload` failure**: edits remain on disk; runtime keybinds stay on previous config. Fix syntax error in conf, reload again.
- **Daemon doesn't bind socket post-rebuild**: bolt 030 acceptance criteria not met; investigate the daemon binary version (Cargo.toml workspace version reflects the bumped value? flake.lock pulled the new version?).

### Daemon-stop hang investigation (intent 017 side issue)

The intent-017 inception log noted: "Daemon takes 5+ seconds to exit after SIGTERM, gets SIGKILL'd by systemd. … Possibly an un-cancelled spawned task in `run_event_loop` despite the `AbortOnDrop` guard around the FIFO listener — possibly a `File::open` mid-await on the FIFO path."

Bolt 030 deletes the FIFO listener entirely. If the hypothesis is correct, the hang resolves incidentally. Story 003 measures `time systemctl --user stop media-control-daemon`:

- **< 5s**: hang resolved. Add inception-log Decision Log entry: "Daemon-stop hang from intent 017 side issues resolved incidentally by FIFO listener removal in bolt 030."
- **≥ 5s**: hang persists; not the FIFO. File `019-daemon-shutdown-hang` with the timing measurement and the new hypothesis-set (likely AbortOnDrop on the dgram_listener has the same shape; or socket2 reader has the issue; or it's somewhere else entirely).

### Cross-repo trust boundary

This bolt edits files outside the `media-control` repo (`~/.config/hypr`, `~/nix`). The bolt's construction-log records the diffs for traceability, but the repos themselves don't get a `bolt-031` commit — those repos have their own version control and changelog conventions. Document the diffs in the construction-log + inception-log, link to commits in the respective repos.
