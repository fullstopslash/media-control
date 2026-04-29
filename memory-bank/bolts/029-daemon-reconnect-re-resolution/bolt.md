---
id: 029-daemon-reconnect-re-resolution
unit: 002-daemon-reconnect-re-resolution
intent: 017-daemon-his-autodetect
type: simple-construction-bolt
status: complete
stories:
  - 001-connect-loop-re-resolves
created: 2026-04-29T08:10:00Z
replanned: 2026-04-29T11:00:00Z
started: 2026-04-29T11:05:00Z
completed: 2026-04-29T11:50:00Z
requires_bolts: [028-his-resolve-with-probe]
enables_bolts: []
requires_units: [001-his-resolve-with-probe]
blocks: false
current_stage: done
stages_completed:
  - name: plan
    completed: 2026-04-29T11:05:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-04-29T11:30:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-04-29T11:50:00Z
    artifact: test-walkthrough.md

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 029-daemon-reconnect-re-resolution

### Objective

**Rescoped 2026-04-29 after bolt 028 landed.** The production-code change this
bolt was originally going to make — moving `get_socket2_path()` inside
`connect_hyprland_socket()`'s retry loop — was preemptively delivered by bolt
028 as part of the `runtime_socket_path` async refactor. See bolt 028's
implementation walkthrough, *Deviations from Plan*, "Why the deviation matters
for future bolts."

This bolt now covers the **remaining work**:

1. **Automated swap-mid-retry test** (story 001 AC #2) — a daemon-crate test
   that enters `connect_hyprland_socket()` against a tempdir with no live HIS,
   waits one retry tick, installs a `LiveWithClients` mock, and asserts the
   next loop iteration connects to it.
2. **Structured manual-validation log entry** (story 001 AC #3) — kill+restart
   Hyprland with the daemon running; capture the journal line confirming
   reconnection within ~1 retry tick. Documents whether the FR-4 EOF-on-
   Hyprland-death assumption holds in practice.

End state: AC #2 is locked in by an automated regression test; AC #3 has a
written record from a real run that future contributors can refer to when
deciding whether heartbeating is needed.

### Stories Included

- [ ] **001-connect-loop-re-resolves** — ACs #1 and #4 already delivered by
  bolt 028 (production code already calls `get_socket2_path().await` per
  iteration; existing daemon tests already pass). This bolt closes ACs #2
  (mock-test swap-in) and #3 (manual validation).

### Expected Outputs

- New daemon-crate test (`#[tokio::test]`) covering swap-mid-retry. Lives in
  `crates/media-control-daemon/src/main.rs`'s `mod tests` (or a new tests
  module if cleaner).
- `media-control-lib` exposes `test_helpers` behind a `test-helpers` feature
  flag so the daemon crate can reuse `MockHyprlandInstance` /
  `with_isolated_runtime_dir` without duplicating the mock layer (intent 015
  FR-5: single-source test infrastructure).
- `media-control-daemon`'s `[dev-dependencies]` activates the new feature.
- Construction-log entry under unit 002 documenting:
  - Replan rationale (production code subsumed by bolt 028)
  - Manual-validation observation: did socket EOF fire promptly on Hyprland
    `kill -9`? Cite journalctl line and timing.
- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --tests -- -D warnings` all clean.

### Dependencies

- **Requires bolt**: 028-his-resolve-with-probe (provides the resolver, the
  per-iteration call site in `connect_hyprland_socket`, and the
  `MockHyprlandInstance` scaffolding being reused).
- **Requires unit**: 001-his-resolve-with-probe (must be merged; verified —
  bolt 028 landed at `b3c108d4ff8b` on main).

### Notes

**Why the rescope didn't reduce this to a one-line addition**: the swap-mid-
retry test needs to run against the *daemon's* `connect_hyprland_socket`, not
against `runtime_socket_path` directly. That means it lives in the daemon
crate, which means the daemon crate needs access to the lib crate's
`MockHyprlandInstance` — which today is `#[cfg(test)] pub mod test_helpers`,
i.e. only visible to the lib's own tests. The conventional fix is a `pub mod`
behind a `test-helpers` feature, activated as a dev-dep feature in the daemon.
Small, unsurprising change.

**Manual validation observability**: the daemon already emits
`info!("Connected to Hyprland socket at {socket_path:?}")` on success and
`warn!("Failed to resolve Hyprland socket path: {e} ...")` on
`NoLiveInstance`. That's enough to read the recovery story from journalctl
without adding new logging.

**If the EOF assumption fails** (kernel keeps the conn in CLOSE_WAIT after
`kill -9`), document the negative result in the construction-log and open
follow-up intent 018-daemon-heartbeat. Don't extend this bolt's scope.
