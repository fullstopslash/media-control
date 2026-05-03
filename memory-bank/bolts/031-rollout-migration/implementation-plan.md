---
stage: plan
bolt: 031-rollout-migration
created: 2026-05-03T16:45:54Z
---

## Implementation Plan: rollout-migration

### Objective

Activate intent 018 across the user's live system on `malphas`: migrate 9 Hyprland keybinds from FIFO `echo` to `media-control kick`, remove the dead `systemd.user.sockets.media-control-daemon` block from `~/nix/modules/apps/media/media-control.nix`, run `nixos-rebuild switch --flake ~/nix#malphas` and `hyprctl reload`, then execute the full DoD validation matrix end-to-end. Records whether intent 017's daemon-stop hang is resolved incidentally.

### ⚠️ Pre-flight blocker (must resolve before Stage 2)

**Bolt 030's release has not yet shipped.** Pre-flight checks at Stage 1 start:

- `which media-control` → `/run/current-system/sw/bin/media-control` (system-installed via NixOS)
- `media-control kick` → `error: unrecognized subcommand 'kick'` (exit 2)

The currently-installed binary is from the pre-bolt-030 release. Migrating the keybinds **before** the new binary is installed would break them — `media-control kick` would exit non-zero on every layoutmsg keypress and the Hyprland keybind shell would surface stderr noise (and possibly a dunst notification via the keybind's exec chain).

**Required to unblock Stage 2:**

1. Bolt 030's working-copy changes in `~/projects/media-control` need to be committed via jj (currently uncommitted change `nkswoswx`, "no description set").
2. The commit needs to be pushed to `forgejo.chimera-micro.ts.net/rain/media-control.git` ref=main (the URL `~/nix/flake.lock` pulls from).
3. `~/nix/flake.lock` needs to be updated to point at the new commit (`nix flake lock --update-input media-control` from the `~/nix` working copy).

Steps 1 and 2 are user-policy territory (the `just commit` / `jj git push` workflow per `~/.claude/CLAUDE.md`). Step 3 can ride along with the Stage 3 `nixos-rebuild` invocation.

**This plan documents Stage 2 + Stage 3 as if the pre-flight is satisfied.** The Stage 1 checkpoint should confirm whether the user wants:

- (a) to handle the commit/push themselves via `just commit "..."` and tell me to proceed, OR
- (b) me to run `jj describe` + `jj git push` on their behalf with explicit approval, OR
- (c) the bolt to wait until they've shipped the release through their normal flow.

---

### Deliverables

**File edits (Stage 2)**

- `~/.config/hypr/conf.d/common.conf` — replace 9 occurrences of `&& echo > "$XDG_RUNTIME_DIR/media-avoider-trigger.fifo"` with `&& media-control kick`. Pre-edit count confirmed: exactly 9, all on `bind = $mainMod, …, exec, hyprctl dispatch layoutmsg …` lines (lines 375, 377, 378, 389, 390, 391, 392, 393, 394).
- `~/nix/modules/apps/media/media-control.nix` — delete the `systemd.user.sockets.media-control-daemon` block (the bottom of the module, ~10 lines). Update the now-stale `doCheck = false` comment that references `create_fifo_at_replaces_our_own_existing_fifo` (a test that no longer exists post-bolt-030; the comment will mislead future readers if left as-is).

**Live-system operations (Stage 3)**

- `nix flake lock --update-input media-control` (in `~/nix`) — pulls the new flake commit into `flake.lock`. Required so `nixos-rebuild` builds the new binary.
- `nixos-rebuild switch --flake ~/nix#malphas` — installs the new media-control package + applies the module change (drops the .socket unit).
- `hyprctl reload` — picks up the keybind file edit.
- DoD validation matrix (per bolt 031 story 003).

**Documentation update (intent-018 inception-log)**

- Append the daemon-stop-hang outcome (resolved incidentally vs. persists, with measured `time systemctl --user stop` value).

---

### Dependencies

| Dependency | Why needed | Status |
|---|---|---|
| Bolt 030 release shipped (commit pushed + flake.lock updated) | New `media-control kick` binary must be installed before keybinds reference it | **NOT MET — see pre-flight blocker above** |
| `~/nix` working copy clean enough for a rebuild | `nixos-rebuild` will surface any uncommitted unrelated changes | Verified separately at Stage 3 start |
| `malphas` is the live target | Per intent business constraint; matches global CLAUDE.md "default to ~/nix unless specified" | Confirmed (`hostname` shows malphas) |
| Hyprland 0.54.x running | `hyprctl reload` needs an active session | Confirmed (current session) |

---

### Technical Approach

**Stage 2 ordering:**

1. **Edit `~/nix/modules/apps/media/media-control.nix`** first — pure file edit, no live-system effect. Removes the dead `systemd.user.sockets.media-control-daemon` block and updates the stale `doCheck` comment. After save, the module is ready for the next `nixos-rebuild` to apply.
2. **Edit `~/.config/hypr/conf.d/common.conf`** second — same property: pure file edit, no live-system effect until `hyprctl reload`. After save, the file is ready for the next reload to pick up.

Order is interchangeable; doing the nix edit first means a single jj/git commit could group "all Stage-2 edits" if the user wants to track the cross-repo change in a single change.

**Why Stage 2 stops here:** Both edits are inert until the live operations of Stage 3 run. Stopping at the Stage 2 checkpoint gives the user a chance to review the diffs before anything affects the running system — the standard "show diff, confirm, then deploy" gate.

**Stage 3 ordering:**

1. **Confirm pre-flight prerequisites** — re-check `which media-control` and `media-control kick --help` to verify the new binary will be available after rebuild. Re-check `git`/`jj` state of `~/projects/media-control` to confirm bolt 030 was actually committed.
2. **`nix flake lock --update-input media-control`** in `~/nix` — picks up the new flake hash. May surface as a working-copy change in `~/nix` itself; user's commit policy applies.
3. **`nixos-rebuild switch --flake ~/nix#malphas`** — installs the new media-control package + applies the module change. Verify exit code; if rebuild fails, halt and investigate (likely flake-eval error or build error in the nix sandbox; `nixos-rebuild --rollback` available as safety net).
4. **`systemctl --user daemon-reload && systemctl --user restart media-control-daemon.service`** — explicitly restart the daemon so it picks up the new binary. The systemd .socket unit removal may need `systemctl --user daemon-reexec` to flush.
5. **`hyprctl reload`** — picks up the keybind file edit.
6. **DoD validation matrix** — verify each acceptance criterion from story 003 in order; record results in inception-log Decision Log.

**Risk mitigation:**

- nixos-rebuild has `--rollback` available if it leaves the system in a bad state.
- Hyprland's `hyprctl reload` is atomic — malformed config keeps the previous keybinds active and surfaces an error.
- File edits live in jj-tracked working copies (both `~/nix` and `~/.config/hypr` if the user has dotfile tracking) and can be reverted via jj.
- The keybind change is fully reversible (edit back to `echo > $fifo` and reload) — but only AFTER the FIFO exists again, which it won't post-bolt-030 daemon. So functional rollback path is `nixos-rebuild --rollback`, not "edit keybind file back".

---

### Acceptance Criteria

**Per-story (rolled up from story files)**

- [ ] Story 001 — Keybind migration: `grep -c media-avoider-trigger.fifo ~/.config/hypr/conf.d/common.conf` returns 0; `grep -c 'media-control kick' ~/.config/hypr/conf.d/common.conf` returns 9; each of 9 keybinds produces `Processing trigger` in daemon journal within ~50ms of keypress.
- [ ] Story 002 — NixOS module cleanup: `grep media-control-daemon.socket ~/nix/modules/apps/media/media-control.nix` returns nothing; `nixos-rebuild switch` succeeds; `systemctl --user list-sockets | grep media-control` returns empty post-rebuild; daemon `active (running)`.
- [ ] Story 003 — DoD validation: full matrix runs to completion; daemon-stop-hang outcome recorded.

**DoD validation matrix (from intent requirements.md "Definition of Done")**

- [ ] All 9 layoutmsg keybinds work via `media-control kick` against running daemon
- [ ] `pkill -KILL media-control-daemon` followed by keybind press: silent exit 0, no shell hang, no dunst spam
- [ ] `cargo test --workspace --all-features` green (already verified in bolt 030 — re-confirm)
- [ ] `nix build .#default` green (currently blocked by 1 pre-existing test flake — same status as bolt 030 closure; not introduced by 031)
- [ ] `~/.config/hypr/conf.d/common.conf` does not contain `media-avoider-trigger.fifo`
- [ ] `~/nix/modules/apps/media/media-control.nix` does not contain `systemd.user.sockets.media-control-daemon`
- [ ] `nixos-rebuild switch --flake ~/nix#malphas` succeeds
- [ ] Daemon restarts cleanly into new transport; `ss -lx | grep media-control-daemon.sock` shows `u_dgr` listener bound by daemon
- [ ] Legacy FIFO at `$XDG_RUNTIME_DIR/media-avoider-trigger.fifo` is unlinked after daemon startup (FR-8 verification)
- [ ] `time systemctl --user stop media-control-daemon` measured; recorded in inception-log Decision Log

---

### Discovered side issues to address

1. **Stale `doCheck` comment in host module.** The current `~/nix/modules/apps/media/media-control.nix` line 17-19 reads:
   > Skip cargo-test in the Nix sandbox: one daemon test (create_fifo_at_replaces_our_own_existing_fifo) asserts inode changes after rm+mkfifo, which is fragile in tmpfs and trips the build.

   That test no longer exists (deleted in bolt 030). The comment now misleads. Bolt 031 story 002 should update or delete this comment as part of the module edit — it's already in scope (touching the same file) and a one-line fix.

2. **Flake input pin model.** `~/nix/flake.lock` pins `media-control` by commit hash. `nix flake lock --update-input media-control` is required to refresh after each new commit. This is standard for the project but worth flagging in case the user prefers a `?ref=tag` or version-based pin scheme.

---

### Open Questions for Implementation

| Question | Resolution path |
|---|---|
| Who handles the bolt-030 commit + push? | Stage 1 checkpoint blocker (a/b/c above) |
| Does the user want me to run `nix flake lock --update-input media-control` in `~/nix`, or will they do it during their normal flake-update flow? | Stage 1 checkpoint clarification |
| At which point in Stage 3 does the user want to manually verify (vs. auto-proceed)? | Stage 1 checkpoint preference |
| If the daemon-stop hang persists post-018, should I file `019-daemon-shutdown-hang` immediately or wait for explicit ask? | Per inception decision log: file conditional follow-up (not pre-filed) |

---

### Out of Scope (reminder)

- Updating the parallel `~/nix-config` work-in-progress port — out of scope per intent constraint.
- Adding new keybinds — pure migration only.
- Touching non-layoutmsg keybinds.
- Cross-host validation (single-author single-host rollout).
- Long-burn-in validation (24h soak test).
- Investigating broader `~/nix` doc rot beyond the single stale `doCheck` comment.
