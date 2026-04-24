---
id: 026-daemon-substrate-tightening
unit: 002-daemon-substrate-tightening
intent: 015-avoider-carveout
type: ddd-construction-bolt
status: complete
stories:
  - 001-pick-and-apply-enforcement
  - 002-prove-isolation
created: 2026-04-26T00:00:00Z
completed: 2026-04-26T00:00:00Z
requires_bolts: [025-commands-regrouping]
enables_bolts: [027-avoider-cleanup]
requires_units: [001-commands-regrouping]
blocks: false
current_stage: complete
stages_completed:
  - name: model
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-01-domain-model.md
  - name: design
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-02-technical-design.md
  - name: adr
    completed: 2026-04-26T00:00:00Z
    artifact: adr-001-daemon-boundary-via-feature-and-greptest.md
  - name: implement
    completed: 2026-04-26T00:00:00Z
    artifact: (source code; no walkthrough doc per ddd-construction-bolt template)
  - name: test
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-03-test-report.md

complexity:
  avg_complexity: 2
  avg_uncertainty: 2
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 026-daemon-substrate-tightening

### Objective

Make "the daemon does not import workflow or Jellyfin code" a contract the
build enforces, not a habit contributors maintain. Choose one of three
mechanisms (cargo feature flag, module visibility, or `compile_fail` doctest),
apply it, and add a CI-runnable verification.

### Stories Included

- [ ] **001-pick-and-apply-enforcement** — Survey existing project conventions
  (any existing `[features]` blocks? any existing `pub(crate)` patterns in
  `media-control-lib`?). Evaluate the three options against those conventions.
  Pick one. Apply it. Document the choice in this bolt's construction log with
  one-paragraph rationale.

  Option comparison:

  | Option | Pros | Cons |
  |--------|------|------|
  | Cargo feature `cli` | Strips `reqwest` from daemon's dep tree; clean isolation | Adds `--features cli` to CLI builds; conditional-compilation noise |
  | `pub(crate)` + `cli` facade | No feature noise; module-system enforced | `reqwest` still in lib's dep set |
  | `compile_fail` doctest | Zero structural change | Catches at test-time, not build-time |

  Default if no signal: Option 1 (cargo feature) — strips `reqwest` is
  measurable value beyond the symbolic boundary.

- [ ] **002-prove-isolation** — Write the verification (form depends on choice
  above):
  - Option 1: Integration test that runs `cargo tree -p media-control-daemon`
    and asserts absence of `reqwest`
  - Options 2 or 3: `compile_fail` doctest in `media-control-daemon/src/main.rs`
    with `use media_control_lib::commands::workflow::mark_watched;`

  Wire the verification into `.forgejo/workflows/ci.yaml`. Document the signal
  in `AGENTS.md`.

### Expected Outputs

- One enforcement mechanism applied (Cargo.toml edits and/or visibility changes
  and/or doctest)
- CI-runnable verification that fails on workflow imports from the daemon
- `cargo build --workspace` clean (with whatever feature flags are needed)
- `cargo test --workspace` clean
- If Option 1 chosen: `cargo tree -p media-control-daemon` shows no `reqwest`
- Construction log: enforcement-mechanism decision with rationale

### Dependencies

Requires bolt 025 (commands-regrouping) — the `commands::workflow` boundary
must exist before it can be enforced.

### Notes

This bolt is small but high-leverage. The substantive content is the design
decision; the application is mechanical. Treat the design stage seriously
because the chosen mechanism has knock-on effects (Option 1 changes how the
CLI is built; Option 2 changes how the lib's API surface is shaped).
