---
intent: 018-sock-trigger-ipc
created: 2026-05-03T15:44:51Z
completed: 2026-05-03T16:03:16Z
status: complete
---

# Inception Log: 018-sock-trigger-ipc

## Overview

**Intent**: Replace the daemon's FIFO trigger transport (`$XDG_RUNTIME_DIR/media-avoider-trigger.fifo`) with a `SOCK_DGRAM` UNIX socket (`$XDG_RUNTIME_DIR/media-control-daemon.sock`), retire the dead `systemd.user.sockets.media-control-daemon` unit, add a first-class `media-control kick` CLI subcommand, and migrate 9 Hyprland keybinds + the NixOS module in a single coordinated rollout.

**Type**: refactor + hardening (transport replacement; user-visible only via fixed keybind reliability).

**Created**: 2026-05-03

## Triggering Context

Three issues converged to motivate this intent:

1. **Bifurcated IPC.** The systemd `.socket` unit declared at `~/nix/modules/apps/media/media-control.nix:48-53` looks like it does something but doesn't — daemon never calls `sd_listen_fds()` and `ss -lx` confirms no listener while the service is active. New readers of the systemd config have to grep daemon source to find that the FIFO is the only live transport.
2. **FIFO writer-blocks-on-no-reader hazard.** `echo > $fifo` from a Hyprland keybind blocks indefinitely if the daemon is down, restarting, or wedged. Empirically confirmed on 2026-05-01: daemon was down momentarily and the keybind shell hung. `SOCK_DGRAM` eliminates this — sendto on a connectionless datagram socket never blocks waiting for a reader.
3. **Hyprland 0.54.3 emits zero socket events for `dispatch layoutmsg togglesplit`.** Empirically confirmed during 2026-05-01 work. The avoider has no other way to notice a layout reshuffle except an external kick. The kick path is therefore *the* user-visible reliability surface for layoutmsg keybinds; making it bulletproof matters disproportionately.

The intent draft at `intents/sock-trigger-ipc.md` (frozen as of `f83d109`) captured the full problem analysis, FR-1..8, non-goals, and 4 explicit open questions for the inception agent. This intent formalizes that draft into the memory-bank structure.

## Artifacts Created

| Artifact | Status | File |
|----------|--------|------|
| Requirements | ✅ approved (Checkpoint 2) | requirements.md |
| Inception Log | ⏳ in-progress | inception-log.md |
| System Context | ✅ generated | system-context.md |
| Units | ✅ generated | units.md + units/001-socket-transport/unit-brief.md + units/002-rollout-migration/unit-brief.md |
| Stories | ✅ generated | 8 stories (5 in unit 001, 3 in unit 002) |
| Bolt Plan | ✅ generated | memory-bank/bolts/030-socket-transport/bolt.md + memory-bank/bolts/031-rollout-migration/bolt.md |

## Summary

| Metric | Count |
|--------|-------|
| Functional Requirements | 9 |
| Non-Functional Requirements | 11 (Performance / Reliability / Maintainability / Security / Observability) |
| Units | 2 |
| Stories | 8 (7 Must, 1 Should) |
| Bolts Planned | 2 (both simple-construction-bolt) |

## Units Breakdown

| Unit | Stories | Bolts | Priority |
|------|---------|-------|----------|
| 001-socket-transport | 5 | 1 (030) | Must (the substrate) |
| 002-rollout-migration | 3 | 1 (031) | Must (the user-visible payoff) |

## Decision Log

| Date | Decision | Rationale | Approved |
|------|----------|-----------|----------|
| 2026-05-03 | `kick` CLI does NOT accept `--reason` or any payload-shaping flag in this release (Q1 → FR-9) | "Most future-modular" answer. Reserve a version-byte wire protocol now (0-byte = canonical kick forever; non-empty = `[ver, …payload]`) so a future telemetry intent can add `--reason` without breaking the canonical kick. CLI argument shape can also evolve in lockstep. | User (2026-05-03) |
| 2026-05-03 | Legacy FIFO cleanup lives daemon-side (Q2 → FR-8) | Daemon owns IPC lifecycle. CLI-side cleanup leaks legacy concerns into the new entry point and adds first-run state to a stateless command. | User-deferred ("no clue") — agent picked daemon-side |
| 2026-05-03 | Keep AbortOnDrop spawn shape, rename `fifo_listener_handle` → `dgram_listener_handle` (Q3) | Mechanical. No reason to restructure. Git blame leaks the rename either way. | User-deferred ("don't care") — agent picked rename-only |
| 2026-05-03 | Socket mode `0o600` (Q4) | Matches FIFO posture. Single-user. No current shared-group caller. | User-deferred ("don't care") — agent picked `0o600` |
| 2026-05-03 | NixOS module change in scope (Q5 → FR-7) | User: "yes definitely if there need to be changes to the nix repo from changes in this project that will need to land." Bolt plan covers `~/nix/modules/apps/media/media-control.nix`. | User (2026-05-03) |
| 2026-05-03 | Hyprland keybind file in scope (Q6 → FR-6) | User: "also in scope. anything that needs updated will need updated." Bolt plan covers `~/.config/hypr/conf.d/common.conf`. | User (2026-05-03) |
| 2026-05-03 | Single coordinated rollout (Q7) | User: "yeah. get it all. we want roll out as quick as possible." Avoids any intermediate state where the new keybinds reference a CLI subcommand that doesn't exist yet. Bolt structure to be decided at bolt-plan stage (likely one bolt for in-repo work + sequenced cross-repo updates in the same release window). | User (2026-05-03) |
| 2026-05-03 | Defer to construction: lib `kick()` sync vs async, exact module placement, choice of daemon-side `is_socket()` predicate | Implementation detail with multiple reasonable answers; better decided with code in front. | Self |
| 2026-05-03 | Two-unit split (substrate vs activation) instead of one big unit | Clean review boundary: Unit 1 is a workspace PR (Rust + tests + docs); Unit 2 is a config + deployment change. Same release window per Q7, but different review surfaces and different validation gates. | User (2026-05-03 Checkpoint 3) |
| 2026-05-03 | Bolt 030 holds all 5 unit-1 stories despite being at the upper end of the bolt-plan size rule | Stories share scaffolding (lib helpers, TOCTOU pattern, mutual deletion of FIFO machinery). Splitting would create unnecessary intermediate-state PRs. | User (2026-05-03 Checkpoint 3) |
| 2026-05-03 | Story 004 (FIFO cleanup) priority Should rather than Must | The FIFO file is harmless to leave behind; cleanup is hygiene. Note: the ~118 LOC code-deletion portion of story 004 is implicitly Must (FIFO functions can't co-exist with the new transport). | User (2026-05-03 Checkpoint 3) |
| 2026-05-03 | Daemon-stop-hang follow-up intent (`019-daemon-shutdown-hang`) is conditional, not pre-filed | Bolt 031 story 003 measures the hang post-018; if resolved incidentally by FIFO listener removal, no follow-up needed. Avoids creating speculative intent backlog. | User (2026-05-03 Checkpoint 3) |

## Discovered Side Issues (Out of Scope, Worth Tracking)

| Issue | Evidence | Suggested Follow-Up |
|-------|----------|---------------------|
| Daemon takes 90s+ to exit after SIGTERM, gets SIGKILL'd by systemd | `Daemon stopped cleanly` log line followed by silence, then systemd `State 'stop-sigterm' timed out. Killing.`. Same issue noted in intent 017's inception log. | May resolve incidentally when this intent deletes the FIFO listener (intent 017 hypothesised the FIFO `File::open` mid-await as the culprit). If it persists post-018, file a dedicated intent. Validate during construction. |

## Scope Changes

| Date | Change | Reason | Impact |
|------|--------|--------|--------|

## Ready for Construction

**Checklist**:
- [x] Triggering context documented
- [x] Open questions resolved (Checkpoint 1)
- [x] Requirements drafted
- [x] Requirements approved (Checkpoint 2)
- [x] System context defined
- [x] Units decomposed
- [x] Stories created
- [x] Bolts planned
- [x] Human review complete (Checkpoint 3, approved 2026-05-03T16:03:16Z)
- [x] Ready for Construction (Checkpoint 4, approved 2026-05-03T16:03:16Z)

## Dependencies

Linear: 030-socket-transport → 031-rollout-migration. Bolt 031 cannot start until bolt 030's release ships (the keybind migration calls `media-control kick`, which only exists post-030; the nix module cleanup assumes the daemon binds its own socket).

## Next Steps

1. User reviews all artifacts (Checkpoint 3): system-context.md, units.md, both unit-briefs, all 8 story files, both bolt files
2. On approval (Checkpoint 4): transition to Construction Agent → start with bolt 030-socket-transport
3. Execute: `/specsmd-construction-agent --unit="001-socket-transport"` (or bolt-targeted: `--bolt="030-socket-transport"`)
