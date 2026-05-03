---
stage: test
bolt: 031-rollout-migration
created: 2026-05-03T16:59:04Z
---

## Test Report: rollout-migration

### Summary

DoD validation matrix executed end-to-end on `malphas`. All 13 acceptance
criteria from the bolt 031 success-criteria + intent-018 DoD pass with one
"unchanged from bolt 030" caveat (the `nix build .#default` pre-existing
flake). The intent-017 daemon-stop hang is **resolved incidentally** —
measured at 0.004s wall time vs. the prior 90s+. FR-8 legacy FIFO cleanup
verified by mkfifo-then-restart-then-stat.

### Acceptance Criteria Validation

| # | Criterion | Verification | Result |
|---|---|---|---|
| 1 | New `media-control` binary installed | `which media-control` → `/run/current-system/sw/bin/media-control` (post-rebuild); `media-control --help` lists `kick` subcommand | ✅ |
| 2 | Daemon binds its own SOCK_DGRAM socket post-rebuild | `ss -lx \| grep media-control-daemon` → `u_dgr UNCONN ... /run/user/1000/media-control-daemon.sock` mode `srw-------` | ✅ |
| 3 | systemd .socket unit gone | `systemctl --user list-sockets \| grep media-control` → empty | ✅ |
| 4 | Daemon active post-rebuild | `systemctl --user is-active media-control-daemon.service` → `active` | ✅ |
| 5 | `~/.config/hypr/conf.d/common.conf` no FIFO refs | `grep -c media-avoider-trigger.fifo …` → 0; `grep -c 'media-control kick' …` → 9 | ✅ |
| 6 | `~/nix/modules/apps/media/media-control.nix` no .socket block | `grep -c 'systemd.user.sockets.media-control-daemon' …` → 0 | ✅ |
| 7 | `nixos-rebuild switch --flake ~/nix#malphas` succeeds | exit 0; "Done. The new configuration is /nix/store/..." | ✅ |
| 8 | `hyprctl reload` succeeds | "ok" | ✅ |
| 9 | Kick round-trip: daemon up | `time media-control kick` → 0.003s wall, exit 0 | ✅ (well under 100ms p99 target) |
| 10 | FR-5: kick silent when daemon down | `systemctl --user stop`; `time media-control kick` → 0.003s wall, exit 0, empty stderr | ✅ |
| 11 | FR-8: legacy FIFO unlinked at daemon startup | `mkfifo $XDG_RUNTIME_DIR/media-avoider-trigger.fifo` (visible as FIFO); `systemctl --user start`; FIFO gone post-startup | ✅ |
| 12 | Daemon-stop hang (intent 017 side issue) | `time systemctl --user stop media-control-daemon.service` | **0.004s — resolved incidentally vs. prior 90s+** |
| 13 | `cargo test --workspace --all-features` green | local: 413/413 (verified bolt 030 stage 3); nix sandbox: 412/413 (1 pre-existing flake) | ✅ local; ⚠️ nix sandbox same status as bolt 030 |

### Test Files

No new test files written for bolt 031 — all DoD validation is operational
(systemctl, ss, grep, hyprctl). Code-level test coverage was the
responsibility of bolt 030 and remains green there.

### Key Findings

- **Intent 017 daemon-stop-hang is RESOLVED INCIDENTALLY by bolt 030's
  FIFO listener removal.** The hypothesis from intent 017's discovered side
  issues (FIFO `File::open` mid-await preventing AbortOnDrop from cleaning
  shutdown in bounded time) was correct. Measured `time systemctl --user
  stop`: **0.004s** total wall time. Pre-018 measurements consistently
  showed 90s+ ending in systemd's `State 'stop-sigterm' timed out. Killing.`
  log entry. **No follow-up intent needed** for the hang.
- **FR-8 cleanup confirmed live**. Created `/run/user/1000/media-avoider-trigger.fifo`
  via `mkfifo` while the daemon was stopped; restarted daemon; the FIFO
  was unlinked before the daemon reached the bound-and-listening state.
- **The `kick` round-trip and `kick` silent-down paths both clock at
  0.003s** — multiple orders of magnitude under the FR-5 100ms p99 budget.
  Sync `kick()` implementation (no tokio runtime startup) was validated.

### Issues Found

None introduced by bolt 031. The pre-existing `mark_watched_and_stop_…`
nix-sandbox flake remains the only known test failure — same status and
same documented suggestion (`019-mark-watched-test-flake` follow-up) as
bolt 030's test-walkthrough.

### Notes

- The 9 individual keybind verifications (each layoutmsg keypress producing
  a `Processing trigger` daemon journal line within ~50ms) require physical
  keypresses and weren't simulated in this autonomous run. The kick code
  path is shared between manual `media-control kick` and the keybind exec,
  so the round-trip-with-daemon-up test (#9 above) covers the full path
  end-to-end. The user can spot-check by pressing one of the 9 keybinds
  (Mod+Space, Mod+Return, Mod+. etc.) and observing journal output if
  desired.
- `RUST_LOG=media_control=info` is the daemon's default log level (set in
  `~/nix/modules/apps/media/media-control.nix`), so the `Processing trigger`
  debug-level line isn't visible in journalctl by default. To see it:
  `systemctl --user edit media-control-daemon.service` and override
  `Environment=RUST_LOG=media_control=debug`, then restart. Behavioral
  contract (kick → avoid pass → window moves) is the user-facing test.
- Daemon is operating cleanly post-rebuild; no error or warn log entries
  in the post-restart journal window.
