---
id: 001-hyprland-keybind-migration
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 031-rollout-migration
implemented: true
---

# Story: 001-hyprland-keybind-migration

## User Story

**As a** Hyprland user pressing layoutmsg keybinds (Mod+Enter, Mod+S, etc.)
**I want** the keybind to trigger the avoider via `media-control kick` instead of `echo > $fifo`
**So that** my keybind shell never hangs when the daemon is down (the original FR-5 motivator) and there's a uniform, transport-independent way to kick the avoider

## Acceptance Criteria

- [ ] **Given** the pre-edit state of `~/.config/hypr/conf.d/common.conf`, **When** I `grep -c media-avoider-trigger.fifo`, **Then** the count is 9 (the keybinds added 2026-05-01)
- [ ] **Given** the post-edit state, **When** I `grep -c media-avoider-trigger.fifo`, **Then** the count is 0
- [ ] **Given** the post-edit state, **When** I `grep -c 'media-control kick'`, **Then** the count is 9 (matching the original 9 keybind lines)
- [ ] **Given** I do `rg media-avoider-trigger.fifo ~`, **When** the search completes, **Then** results are confined to historical/memory-bank locations (no other live caller of the FIFO exists in user config or scripts) — if a stray caller IS found, this story EXPANDS to migrate it before declaring done
- [ ] **Given** the post-edit conf and `hyprctl reload`, **When** I press one of the 9 keybinds against a running new-daemon, **Then** the daemon's journal shows `Processing trigger` within ~50ms of the keypress
- [ ] **Given** all 9 edited lines, **When** I diff the file, **Then** the change is exactly `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` → `&& media-control kick` per line — no other modifications, no whitespace drift, no comment changes

## Technical Notes

- The 9 keybinds were added 2026-05-01 in this session (per intent draft note). They live in `~/.config/hypr/conf.d/common.conf` around line 372.
- Edit can be done with a single `sed -i 's|&& echo > "\$XDG_RUNTIME_DIR/media-avoider-trigger\.fifo"|\&\& media-control kick|g'` invocation, or interactively via editor — both paths are fine; the diff should be minimal and reviewable line-by-line.
- Pre-edit safety check: `rg media-avoider-trigger.fifo ~` to confirm the 9 keybind lines are the only live callers. If other callers exist (a forgotten cron, a script in `~/bin`, a dotfile), they expand this story's scope before the FIFO is functionally gone.
- Post-edit reload: `hyprctl reload`. Hyprland accepts the reload atomically; if the syntax is malformed, `hyprctl reload` returns an error and the keybinds remain on the previous config — fail-safe.
- Smoke-test by pressing each keybind and observing the journal in another terminal: `journalctl --user -u media-control-daemon.service -f` filtered for `Processing trigger`.

## Dependencies

### Requires

- Unit 1 (`001-socket-transport`) shipped and installed — `media-control kick` must exist before the keybind migration lands

### Enables

- 002-nixos-module-cleanup (the keybind migration validates the new transport works end-to-end against a running new-daemon; module cleanup follows immediately)
- 003-end-to-end-validation (the DoD validation matrix exercises these keybinds)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| One of the 9 keybinds has a typo or differs slightly from the canonical pattern | The grep-count check (9 → 0) catches this; investigate the outlier before declaring done |
| User has multiple `~/.config/hypr` configs (e.g. host-specific) | This story targets `common.conf` only; if other config files reference the FIFO, the broader `rg ~/.config/hypr` discovers them and this story extends to cover them |
| The FIFO `echo` line was inside a multi-command `exec` chain that does other things too | Sed replacement only swaps the FIFO suffix; the rest of the chain is untouched |
| `hyprctl reload` fails (syntax error in unrelated config) | Edit stays on disk; old runtime keybinds remain; user fixes the syntax error and re-runs reload |

## Out of Scope

- Editing the parallel `~/nix-config` Hyprland module (if any) — out-of-band; this intent targets the live config on `malphas`.
- Adding new keybinds — this story is purely a migration.
- Touching non-layoutmsg keybinds — only the 9 layoutmsg-related ones are in scope.
- Testing keybind behavior on hosts other than `malphas` — single-author single-host rollout per the intent's business constraint.
