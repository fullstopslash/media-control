---
intent: 017-daemon-his-autodetect
phase: inception
status: inception-complete
created: 2026-04-29T08:05:00Z
updated: 2026-04-29T08:15:00Z
---

# Requirements: Daemon Live-HIS Resolution

## Intent Overview

The daemon resolves Hyprland's IPC socket from `HYPRLAND_INSTANCE_SIGNATURE` (HIS) once at startup, with no liveness check. When systemd's user-bus environment holds a stale or empty HIS — which happened on 2026-04-29 after `hyprland.service` restarted into a second instance with no clients — the daemon connects to a real socket of a wrong/empty Hyprland and waits forever for events that never arrive. Symptom: avoider silently does nothing.

This intent makes the daemon resolve the *currently-live* Hyprland instance by probing instead of trusting env state.

## Type

Defect-fix / hardening.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Daemon connects to a Hyprland that actually has client activity | After daemon restart with multiple HIS dirs present, journal shows real socket events within 5s of any user interaction | Must |
| Stale `HYPRLAND_INSTANCE_SIGNATURE` cannot silently break the avoider | Daemon process started with a HIS env pointing at an empty/stale Hyprland still finds and uses the live one | Must |
| Behavior survives Hyprland restart | If Hyprland is restarted while the daemon is running, on socket EOF the daemon re-resolves and connects to the new instance | Should |
| User-pinned HIS still wins | If `HYPRLAND_INSTANCE_SIGNATURE` is set and that instance is alive, daemon uses it (multi-seat / nested Hyprland scenarios stay supported) | Must |

---

## Functional Requirements

### FR-1: Probe-based HIS resolution at startup
- **Description**: On startup, the daemon enumerates `$XDG_RUNTIME_DIR/hypr/*/` and probes each instance's `.socket.sock` with a cheap synchronous query (e.g., `activewindow`). An instance is considered "live" if the socket accepts the connection and the response is non-empty. An instance whose `activewindow` returns `Invalid` is "live but empty" — acceptable as a fallback but not preferred.
- **Acceptance Criteria**: With three HIS dirs present (one dead socket, one live-empty, one live-with-windows), the daemon picks live-with-windows. With only live-empty available, the daemon picks it. With only a dead socket, the daemon falls through to FR-5.
- **Priority**: Must
- **Related Stories**: TBD

### FR-2: Honor explicit `HYPRLAND_INSTANCE_SIGNATURE` when live
- **Description**: If the env var is set AND the named instance passes the FR-1 liveness probe, use it without scanning. This preserves explicit user/multi-seat configuration.
- **Acceptance Criteria**: Daemon launched with `HYPRLAND_INSTANCE_SIGNATURE=<live-instance>` connects to that exact instance even if other live instances exist on the system.
- **Priority**: Must
- **Related Stories**: TBD

### FR-3: Fall through to autodetection when env-named instance is dead
- **Description**: If `HYPRLAND_INSTANCE_SIGNATURE` is set but the named socket is unreachable or returns Invalid (current incident's exact failure mode), the daemon emits a `warn!` naming the stale HIS and proceeds with FR-1 scanning instead of connecting blindly.
- **Acceptance Criteria**: Reproduce the 2026-04-29 incident (systemd env points at empty Hyprland while a live one exists) — daemon connects to the live one and logs a warning identifying the stale env.
- **Priority**: Must
- **Related Stories**: TBD

### FR-4: Re-resolve on reconnect
- **Description**: The existing `run_event_loop` reconnect path (`Ok(true)` after socket EOF in `run_event_session`) currently reuses the originally-resolved socket path. If Hyprland is restarted into a new HIS, this loops forever trying to reach a defunct socket. Reconnect must re-run FR-1 + FR-2 to discover the new live instance.
- **Acceptance Criteria**: Restart Hyprland while the daemon is running; daemon's next reconnect attempt finds and connects to the new instance within the existing 500ms reconnect delay + probe window. Existing automatic-reconnection tests still pass.
- **Priority**: Should
- **Related Stories**: TBD

### FR-5: Conservative fallback when no live instance is found
- **Description**: If FR-1 finds zero responsive sockets (e.g., Hyprland is mid-restart), the daemon falls back to today's behavior: use the env var path, enter the existing exponential-backoff retry loop in `connect_hyprland_socket`. No regression for the cold-Hyprland case.
- **Acceptance Criteria**: Stop Hyprland entirely, then start the daemon — daemon enters retry loop just as it does today; once Hyprland comes back, daemon connects on the next retry tick.
- **Priority**: Must
- **Related Stories**: TBD

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Startup probe overhead | Wall time spent probing all HIS dirs | < 100 ms with up to 4 instances on a healthy system; concurrent probes |
| Probe per-instance timeout | Per-socket connect+read deadline | ≤ 1 s — a wedged Hyprland must not block daemon startup |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| No regression on single-instance hosts | Daemon on a host with exactly one Hyprland behaves identically to today | 100% existing daemon tests pass |
| Reconnect path correctness | Hyprland restart mid-session is recovered without daemon restart | Manual test: kill+restart Hyprland, verify daemon resumes within reconnect window |

### Maintainability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Resolution logic colocated | Probe + selection lives in one module (likely `media-control-lib::hyprland`), used by both daemon and CLI | Single source of truth for "which HIS"; no daemon-specific copy |
| Test coverage | Probe with mock sockets exercises live / empty / dead / no-HIS-dir / stale-env / multi-live cases | All five FRs covered by unit tests using existing `test_helpers` mock infra (per intent 015 FR-5) |

---

## Constraints

### Technical Constraints
- No new runtime crates. `tokio::net::UnixStream` (already used) is sufficient for probing.
- CLI commands also use `runtime_socket_path` — the resolution change should benefit them too without behavior regression. (Probe cost per CLI invocation is acceptable since CLI is short-lived; but cache the resolved HIS for the duration of a single invocation.)
- Per intent 015 carve-out: probe + selection is substrate (`hyprland` module), reachable from both `commands::window` and the daemon. Do not add daemon-specific resolution code in the lib's command modules.

### Business Constraints
- Defect fix, not a feature. Keep the change surgical: one probe function in `hyprland`, two callsites updated (daemon startup, daemon reconnect).

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| `activewindow` IPC reply is reliable for liveness — "Invalid" means empty, anything else means populated | False positives (treats empty as live, picks wrong instance) or false negatives (rejects live instance with no current focus) | Prefer multi-signal: socket accepts AND any response received within deadline. Treat "Invalid" as a tiebreaker, not a rejection. |
| `$XDG_RUNTIME_DIR/hypr/*/` is the canonical location across Hyprland versions used here | Path moves in a future Hyprland release | Read the existing `runtime_socket_path` source for the authoritative path construction; reuse it instead of rebuilding |
| Multiple-live-instance hosts are rare; no need for sophisticated tiebreakers | User on multi-seat setup gets wrong instance silently | Honor env var (FR-2) for explicit pinning; fall back to most-recently-modified HIS dir as final tiebreaker |
| No need for periodic re-probing while connected | Hyprland gets replaced without sending socket EOF (kill -9 of hyprland process); daemon stays bound to dead socket | Hyprland process death closes the listening socket FD, which delivers EOF to readers — the existing reconnect path triggers. Validate this assumption in construction. |

---

## Out of Scope

- Investigating *why* systemd's `hyprland.service` restarted at 02:45 on 2026-04-29 (NixOS configuration concern, not a media-control concern)
- The 5-second SIGTERM-to-SIGKILL hang on daemon shutdown (discovered during this triage; deserves its own intent — appears to be an un-cancelled spawned task in `run_event_loop`, possibly the FIFO listener inside `File::open` despite the AbortOnDrop guard)
- Cross-platform support (still Linux/Hyprland only)
- Auto-cleanup of stale HIS dirs (Hyprland's responsibility, not ours)

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| "Live" predicate: socket-accepts only, OR socket-accepts AND `activewindow != Invalid`? | Construction | Bolt design stage | Pending — FR-1 leaves room for either. Probably: live = accepts; live-with-clients = accepts AND non-Invalid; prefer the latter. |
| Should the lib expose `resolve_live_his()` or just `resolve_socket(kind)` that internally probes? | Construction | Bolt design stage | Pending — the latter is a tighter interface, but the former is more testable in isolation. |
| Should CLI cache the resolved HIS via env var so repeated invocations in a session don't re-probe? | Intent owner | Defer | Probably no — CLI invocations are independent and the probe is cheap. Reconsider only if measured cost is annoying. |
| Validate FR-4 assumption: does Hyprland-process death cleanly EOF readers on socket2? | Construction | Bolt validation stage | Pending — if not, periodic heartbeat is needed instead of (or in addition to) reconnect-time re-resolution. |
