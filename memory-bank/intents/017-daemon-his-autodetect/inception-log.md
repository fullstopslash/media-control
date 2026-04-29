---
intent: 017-daemon-his-autodetect
created: 2026-04-29T08:05:00Z
completed: 2026-04-29T08:15:00Z
status: complete
---

# Inception Log: 017-daemon-his-autodetect

## Overview

**Intent**: Daemon resolves the live Hyprland instance by probing instead of trusting `HYPRLAND_INSTANCE_SIGNATURE` env, fixing a class of "avoider silently does nothing" bugs.

**Type**: defect-fix.

**Created**: 2026-04-29

## Triggering Incident

On 2026-04-29 the user reported "media window avoider is DEFINITELY not working at all now" shortly after intent 015 (avoider carve-out) landed. Triage found:

- Two Hyprland instances alive: `…1777405352…` (the user's interactive session, started Apr 28 14:42) and `…1777448701…` (a second instance with no clients, started Apr 29 02:45 by the systemd `hyprland.service`).
- systemd's user-bus environment held `HYPRLAND_INSTANCE_SIGNATURE=…1777448701…` (the empty one). The daemon, restarted ~12 minutes after `hyprland.service`, inherited that env.
- Daemon connected to a real socket of an empty Hyprland and waited indefinitely for events that never came. Logs showed `Connected to Hyprland socket` followed by silence — no socket events processed in 90+ seconds.
- Not a regression from intent 015 (socket resolution unchanged). Pre-existing fragility surfaced by an HIS rotation.

Hot-fix applied: `systemctl --user set-environment HYPRLAND_INSTANCE_SIGNATURE=<live-instance>` then daemon restart. Avoider resumed within 700ms. Fix is non-durable: any future `hyprland.service` restart re-rotates the env.

## Artifacts Created

| Artifact | Status | File |
|----------|--------|------|
| Requirements | ✅ approved | requirements.md |
| Inception Log | ✅ in-progress | inception-log.md |
| System Context | ✅ | system-context.md |
| Units | ✅ | units.md + units/001-his-resolve-with-probe/unit-brief.md + units/002-daemon-reconnect-re-resolution/unit-brief.md |
| Stories | ✅ | 4 stories across 2 units (3 in unit 001, 1 in unit 002) |
| Bolt Plan | ✅ | memory-bank/bolts/028-his-resolve-with-probe/bolt.md + memory-bank/bolts/029-daemon-reconnect-re-resolution/bolt.md |

## Summary

| Metric | Count |
|--------|-------|
| Functional Requirements | 5 |
| Non-Functional Requirements | 6 (across Performance / Reliability / Maintainability) |
| Units | 2 |
| Stories | 4 |
| Bolts Planned | 2 |

## Units Breakdown

| Unit | Stories | Bolts | Priority |
|------|---------|-------|----------|
| 001-his-resolve-with-probe | 3 | 1 (028) | Must |
| 002-daemon-reconnect-re-resolution | 1 | 1 (029) | Should |

## Decision Log

| Date | Decision | Rationale | Approved |
|------|----------|-----------|----------|
| 2026-04-29 | Honor `HYPRLAND_INSTANCE_SIGNATURE` when the named instance is live (FR-2) | Multi-seat / nested-Hyprland users may pin a specific instance; silent override would surprise them | Pending |
| 2026-04-29 | Fall through to autodetection only when env-named instance is dead (FR-3) | Today's behavior connects blindly; the autodetect path catches exactly the incident scenario | Pending |
| 2026-04-29 | Re-resolve on socket-EOF reconnect (FR-4) | Without this, Hyprland-restart-while-daemon-runs would still get stuck on the old socket path | Pending |
| 2026-04-29 | Defer to construction: choice of liveness predicate, exact module API shape, periodic re-probe vs reconnect-only | Implementation detail with multiple reasonable answers; better decided with code in front | Pending |

## Discovered Side Issues (Out of Scope, Worth Tracking)

| Issue | Evidence | Suggested Follow-Up |
|-------|----------|---------------------|
| Daemon takes 5+ seconds to exit after SIGTERM, gets SIGKILL'd by systemd every restart | Multiple `media-control-daemon.service: State 'stop-sigterm' timed out. Killing.` in journal across separate restarts | Separate intent. Likely an un-cancelled spawned task in `run_event_loop` despite the `AbortOnDrop` guard around the FIFO listener — possibly a `File::open` mid-await on the FIFO path |
| systemd's `hyprland.service` ran a second Hyprland instance at 02:45 with no clients | `cat /proc/640848/cgroup` → `app.slice/hyprland.service`; that instance's `activewindow` returns Invalid | NixOS config concern, not media-control. Worth user investigation but outside this repo |

## Scope Changes

| Date | Change | Reason | Impact |
|------|--------|--------|--------|

## Ready for Construction

**Checklist**:
- [x] Triggering incident documented
- [x] Requirements drafted
- [x] Requirements approved (Checkpoint 2)
- [x] System context defined
- [x] Units decomposed
- [x] Stories created
- [x] Bolts planned
- [x] Human review complete (Checkpoint 3)

## Next Steps

1. Begin Construction Phase
2. Start with Unit: 001-his-resolve-with-probe → Bolt: 028-his-resolve-with-probe
3. Execute: `/specsmd-construction-agent --unit="001-his-resolve-with-probe"`

## Dependencies

Linear: 028-his-resolve-with-probe → 029-daemon-reconnect-re-resolution. Bolt 029 cannot start until bolt 028 lands (uses the resolver and the mock scaffolding).
