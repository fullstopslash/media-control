---
id: 029-daemon-reconnect-re-resolution
unit: 002-daemon-reconnect-re-resolution
intent: 017-daemon-his-autodetect
type: simple-construction-bolt
status: planned
stories:
  - 001-connect-loop-re-resolves
created: 2026-04-29T08:10:00Z
completed: null
requires_bolts: [028-his-resolve-with-probe]
enables_bolts: []
requires_units: [001-his-resolve-with-probe]
blocks: false
current_stage: planned
stages_completed: []

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 1
---

## Bolt: 029-daemon-reconnect-re-resolution

### Objective

Move the path-resolution call inside `connect_hyprland_socket()`'s exponential-backoff
loop in `crates/media-control-daemon/src/main.rs`. Today it resolves once before the
loop and hammers the same path forever; after this bolt each retry tick re-resolves
through the new live-aware resolver, so a Hyprland restart with a new HIS is recovered
within ~1 retry tick.

### Stories Included

- [ ] **001-connect-loop-re-resolves** — Move `let socket_path = get_socket2_path()?;`
  from before the loop into the loop body. Treat resolve failure identically to connect
  failure for backoff purposes. Add a daemon-level test using the `MockHyprlandInstance`
  scaffolding from bolt 028: enter the loop with no live HIS dirs, install a
  `LiveWithClients` mock mid-loop, assert the next iteration connects.

### Expected Outputs

- `connect_hyprland_socket()` re-resolves per iteration; signature unchanged
- New unit test in `crates/media-control-daemon/src/main.rs` covering swap-mid-retry
- Manual validation note in the bolt's construction log: kill+restart Hyprland with
  daemon running, confirm reconnection within ~1 retry tick of the new instance going
  probe-alive
- `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` clean

### Dependencies

- **Requires bolt**: 028-his-resolve-with-probe (uses `runtime_socket_path` now backed
  by `resolve_live_his`; uses `MockHyprlandInstance` test scaffolding)
- **Requires unit**: 001-his-resolve-with-probe (full unit must be merged)

### Notes

This bolt is small enough that it could have been a fourth story in bolt 028. Keeping
it separate matches the unit boundary (lib substrate vs. daemon application) and lets
unit 1 land independently — if FR-4 turns out to need different handling, this bolt
can be reshaped without touching the substrate.

During manual validation, watch what happens when Hyprland is `kill -9`'d (not graceful
exit). The FR-4 assumption is that the socket FD's listening side closes on process
death, delivering EOF to readers. Confirm or deny in the construction log:

- If EOF fires reliably → assumption holds; this bolt completes the FR-4 work
- If reader stays blocked on socket2 → open a follow-up intent for heartbeat/ping
  (out of scope for 017)
