---
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
phase: inception
status: complete
created: 2026-05-03T15:44:51.000Z
updated: 2026-05-03T15:44:51.000Z
---

# Unit Brief: rollout-migration

## Purpose

With Unit 1's release in hand, perform the cross-repo activation that completes the intent's user-visible payoff: 9 Hyprland keybinds switch from FIFO `echo` to `media-control kick`, the dead `systemd.user.sockets.media-control-daemon` unit is removed from the NixOS module, and the DoD validation matrix is run end-to-end on `malphas`.

## Scope

### In Scope

- Edit `~/.config/hypr/conf.d/common.conf` — replace 9 occurrences of `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` with `&& media-control kick`. Verify post-edit: `grep -c media-avoider-trigger.fifo` returns 0.
- Edit `~/nix/modules/apps/media/media-control.nix` — remove the `systemd.user.sockets.media-control-daemon` block (lines 48-53 as of `f83d109`).
- Run `nixos-rebuild switch --flake ~/nix#malphas`.
- Run `hyprctl reload` (Hyprland config reload).
- Execute the DoD validation matrix:
  - Each of 9 layoutmsg keybinds produces a `Processing trigger` log line in the daemon journal within ~50ms.
  - `pkill -KILL media-control-daemon` followed by a keybind press: silent exit 0, no dunst notification, no shell hang.
  - `cargo test --workspace --all-features` green.
  - `nix build .#default` green.
  - `grep media-avoider-trigger.fifo ~/.config/hypr/conf.d/common.conf` returns nothing.
  - `grep media-control-daemon.socket ~/nix/modules/apps/media/media-control.nix` returns nothing.
  - `systemctl --user list-sockets | grep media-control` returns empty post-rebuild.
  - `media-control-daemon.service` is `active (running)` after restart.
- Validate or refute intent 017's hypothesis: did deleting the FIFO listener resolve the 5+ second daemon-stop hang? Document the outcome in inception-log; file follow-up intent if hang persists.

### Out of Scope

- Any `media-control` workspace code change — Unit 1.
- Designing the v1 envelope — future intent.
- Migrating `~/nix-config` (the work-in-progress port) — out-of-band per global CLAUDE.md ("default to ~/nix unless the user specifies otherwise"); cross-port can happen in a separate change.
- Updating any non-keybind callers of the legacy FIFO. (Per FR-6 acceptance criteria: pre-change `rg media-avoider-trigger.fifo ~` confirms the 9 keybind lines are the only callers. If that grep finds other callers, this story expands; otherwise no other call sites exist.)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-6 | Replace 9 Hyprland keybinds in `~/.config/hypr/conf.d/common.conf` | Must |
| FR-7 | Delete dead `.socket` unit from NixOS module | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| Hyprland keybind line | A `bind = …, exec, …` entry in `~/.config/hypr/conf.d/common.conf` | 9 lines currently end with `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` |
| NixOS systemd.user.sockets block | Declarative systemd `.socket` unit declaration in `~/nix/modules/apps/media/media-control.nix:48-53` | Currently dead code (daemon never accepts on it); to be removed |
| DoD validation run | A single end-to-end run of the validation matrix on `malphas` | Pass/fail per matrix row; logged in inception-log |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| Keybind migration | `sed`-style replace in 9 places | conf file | edited file |
| Module cleanup | Delete a 6-line block | nix module | edited file |
| `nixos-rebuild switch --flake ~/nix#malphas` | Apply NixOS configuration change | edited nix module | running system with new config |
| `hyprctl reload` | Reload Hyprland config | edited keybind file | active keybinds reflect change |
| DoD validation | Execute matrix; record pass/fail | running system + new release | inception-log entry |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |
| Should Have | 0 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-hyprland-keybind-migration | Replace 9 FIFO `echo` lines with `media-control kick` in Hyprland conf | Must | Planned |
| 002-nixos-module-cleanup | Remove dead `.socket` unit from `media-control.nix`; nixos-rebuild | Must | Planned |
| 003-end-to-end-validation | Run DoD validation matrix on `malphas`; document daemon-stop-hang outcome | Must | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| 001-socket-transport | Keybind migration calls `media-control kick`; module cleanup assumes daemon binds its own socket. Both prerequisites are satisfied only after Unit 1 ships. |

### Depended By

| Unit | Reason |
|------|--------|
| (none) | Final activation step for the intent |

### External Dependencies

| System | Purpose | Risk |
|--------|---------|------|
| `~/.config/hypr/conf.d/common.conf` | User keybind config (outside this repo) | Low — text file edit, version-controlled in user dotfiles |
| `~/nix/modules/apps/media/media-control.nix` | NixOS module (outside this repo) | Low — declarative removal of dead code |
| `nixos-rebuild switch` | Apply NixOS change | Medium — failure mid-rebuild can leave system in awkward state; mitigation: rebuild target is `malphas` (the user's primary) and rollbacks are first-class via `nixos-rebuild --rollback` |
| `hyprctl reload` | Apply Hyprland config | Low — reload is fast and atomic |

---

## Technical Context

### Suggested Technology

- Plain text edits. No tooling beyond a text editor and `sed -i` if helpful.
- Validation uses existing tooling: `grep`, `journalctl`, `systemctl`, `pkill`.

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `~/nix` repo | Filesystem edit | `nixos-rebuild switch` to apply |
| `~/.config/hypr` | Filesystem edit | `hyprctl reload` to apply |

### Data Storage

None. Edits to text files.

---

## Constraints

- Both edits MUST land in the same release window as Unit 1 ships (Q7 / single coordinated rollout). No intermediate state where keybinds reference a CLI subcommand that doesn't exist.
- The `~/nix` change targets `malphas` (per global CLAUDE.md: "default to `~/nix` unless the user specifies otherwise"); the parallel `~/nix-config` port is **not** updated as part of this unit.
- DoD validation MUST be run on the actual `malphas` system, not in a sandbox — the FR-5 "kick must not block keybind shell" assertion is only meaningful end-to-end.

---

## Success Criteria

### Functional

- [ ] All 9 layoutmsg keybinds in `~/.config/hypr/conf.d/common.conf` use `media-control kick` (FR-6)
- [ ] `grep -c media-avoider-trigger.fifo ~/.config/hypr/conf.d/common.conf` returns 0
- [ ] `systemd.user.sockets.media-control-daemon` block deleted from `~/nix/modules/apps/media/media-control.nix` (FR-7)
- [ ] `nixos-rebuild switch --flake ~/nix#malphas` succeeds
- [ ] `systemctl --user list-sockets | grep media-control` returns empty post-rebuild
- [ ] `media-control-daemon.service` is `active (running)` after restart with no socket-restart loop

### Non-Functional

- [ ] Each of 9 keybinds produces a `Processing trigger` log line in the daemon journal within ~50ms of keypress
- [ ] `pkill -KILL media-control-daemon` followed by a keybind press: keybind exits 0 silently; no dunst notification; no shell hang (FR-5 end-to-end validation)
- [ ] `cargo test --workspace --all-features` green
- [ ] `nix build .#default` green

### Quality

- [ ] DoD validation results documented in `inception-log.md` (Decision Log section)
- [ ] Outcome of daemon-stop-hang investigation (resolved incidentally / persists / new behaviour observed) documented; if persists, follow-up intent created

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 031-rollout-migration | simple-construction-bolt | 001, 002, 003 | Mechanical edits + nixos-rebuild + DoD validation. Stories are tightly sequenced (keybind edit → module edit → rebuild → validate); single bolt minimizes context-switching and keeps the activation atomic. |

---

## Notes

- This unit is the smaller of the two but is where the user-visible payoff lands. Up until story 003 succeeds, the intent is vapourware to the user — the daemon binds a socket nobody calls. Story 003 is therefore the *real* DoD checkpoint.
- The grep-for-other-callers check in story 001 is cheap insurance against silent regressions: if some forgotten cron job or dotfile sources the legacy FIFO, removing the keybinds without finding it would surface as "the daemon's kick path stopped working for that other thing".
- The nixos-rebuild step has a known risk (system left in awkward state on partial failure); user should be primed to use `nixos-rebuild --rollback` if needed. Confirmed-safe rollback path is one of the reasons we land Unit 2 in the same session as Unit 1's release rather than days later.
