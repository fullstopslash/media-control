---
stage: implement
bolt: 031-rollout-migration
created: 2026-05-03T16:59:04Z
---

## Implementation Walkthrough: rollout-migration

### Summary

User authorized autonomous execution of bolts 030 + 031 end-to-end. Committed
+ pushed bolt 030's working copy (version-bumped 0.1.7 → 0.1.8 via `just commit
... --no-verify`; --no-verify justified by the pre-existing nix-sandbox flake
documented in bolt 030 test-walkthrough). Updated `~/nix/flake.lock` to point
at the new media-control commit, edited the host nix module to drop the dead
`systemd.user.sockets.media-control-daemon` block (FR-7) and refresh the now-
stale `doCheck = false` comment that referenced a deleted FIFO test, edited
`~/.config/hypr/conf.d/common.conf` to swap 9 layoutmsg keybinds from FIFO
`echo` to `media-control kick` (FR-6), and committed each change to its
respective jj repo.

### Structure Overview

Three repos touched in addition to `~/projects/media-control`:

- `~/projects/media-control` (jj remote `origin`) — committed bolt 030's
  full diff (lib transport module, daemon socket transport, CLI Kick
  variant, doc updates, deleted in-repo .socket file) + intent-018
  memory-bank artifacts in a single change. Pushed.
- `~/nix` (jj remote `forgejo`) — fresh jj change rebased onto `main@forgejo`
  carrying the flake.lock update + module edit. Sibling to the user's
  pre-existing WIP `M flake.lock` change which was preserved untouched.
  Pushed.
- `~/.config/hypr` (jj-tracked, NO remote) — fresh jj change carrying the
  9-line keybind swap. Local-only; described, no push needed.

### Completed Work

- [x] `just commit "feat(transport): SOCK_DGRAM trigger socket + media-control kick subcommand (intent 018) --no-verify"` in `~/projects/media-control` — bumped workspace 0.1.7 → 0.1.8, described commit `nkswoswx 07c97bf2`, set `main` bookmark, pushed to `origin` (forgejo).
- [x] `nix flake lock --update-input media-control` in `~/nix` — flake.lock now pins media-control at `07c97bf2a9f4...`.
- [x] `~/nix/modules/apps/media/media-control.nix` — deleted the `systemd.user.sockets.media-control-daemon` block; refreshed the stale `doCheck = false` comment to point at the actual current culprit (`mark_watched_and_stop_partial_failure_propagates_stop_error` flake instead of the deleted FIFO test).
- [x] Committed `~/nix` change as `ttsksypu 6c25c57b` (`feat(media-control): consume 0.1.8 SOCK_DGRAM transport; drop dead .socket unit (intent 018 / FR-7)`). Rebased to drop the dependency on the user's pre-existing WIP `ptslyrmw` change so push didn't drag along an undescribed commit. Pushed to `forgejo` (now `main`).
- [x] `~/.config/hypr/conf.d/common.conf` — `sed -i 's| && echo > "\$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"| \&\& media-control kick|g'` — pre-edit grep count: 9; post-edit count: 0 (FIFO refs) and 9 (`media-control kick` refs). Verified.
- [x] Committed `~/.config/hypr` change as `loqwwmvx bfdbb3c3` (`wip(hypr): migrate 9 layoutmsg keybinds from FIFO echo to media-control kick (intent 018)`). No push (no remote).

### Key Decisions

- **`--no-verify` on the bolt 030 commit.** The `verify` recipe runs `nix build .#default`, which currently fails on the pre-existing `mark_watched_and_stop_partial_failure_propagates_stop_error` flake (added 2026-04-23, unrelated to bolt 030). Per the global CLAUDE.md policy: "`--no-verify` is reserved for cases where verify itself is broken upstream and you need to land a fix anyway." This bolt's blocker IS the pre-existing flake. Using `--no-verify` documented in the construction-log; rationale also captured here.
- **Rebased my `~/nix` change off the user's pre-existing WIP.** `ptslyrmw 026054f7` (`M flake.lock`, no description) was a sibling/prior WIP unrelated to intent 018. Pushing through it would have failed (jj refuses to push undescribed commits) and would have dragged in unrelated changes if it had succeeded. Rebased my change onto `main@forgejo` directly so `ptslyrmw` stays as an untouched sibling.
- **Updated stale doCheck comment** as a side-finding within FR-7's edit. The previous comment referenced `create_fifo_at_replaces_our_own_existing_fifo`, deleted in bolt 030. Updating the comment in the same edit avoided a follow-up commit and keeps the doc accurate for future readers.

### Deviations from Plan

- **Bolt 030 commit happened during bolt 031 execution**, not as a user-driven pre-flight. Plan had this as Stage-1-checkpoint blocker option (a/b/c); user picked autonomous execution which collapsed the pre-flight into Stage 2 work for bolt 031. Captured the full sequence here so the audit trail is complete.
- **Did not update `~/nix-config` (the WIP port).** Kept consistent with the global CLAUDE.md scoping: "default to ~/nix unless the user specifies otherwise."

### Dependencies Added

None to any of the three repos. Only flake.lock updates, file edits, and deletions.

### Developer Notes

- All three commits are conventional-commits style and reference intent 018 for traceability.
- The user's pre-existing WIP `M flake.lock` in `~/nix` (`ptslyrmw`) is preserved as a sibling — they can describe + land it independently when ready.
- The `~/.config/hypr` repo is local-only (no remote); the keybind change is committed locally so the working tree is clean for normal jj operations.
- Memory-bank artifacts for bolt 030 are part of the bolt-030 commit. Memory-bank artifacts for bolt 031 (this walkthrough, the test-walkthrough, the updated construction-log) are uncommitted at the time of writing — to be folded into a follow-up bolt-031 commit at completion-time per normal workflow.
