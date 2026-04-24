---
id: 001-pick-and-apply-enforcement
unit: 002-daemon-substrate-tightening
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 026-daemon-substrate-tightening
implemented: false
---

# Story: 001-pick-and-apply-enforcement

## User Story

**As a** developer hardening the daemon's contract
**I want** one of the three enforcement options (cargo feature `cli`, `pub(crate)` + facade module, or `compile_fail` doctest) chosen and applied
**So that** importing workflow code from the daemon becomes a build-time error, not a code-review catch

## Acceptance Criteria

- [ ] **Given** the bolt's design stage, **When** the three options are evaluated against project conventions and recorded in the construction log, **Then** one option is selected with a one-paragraph rationale
- [ ] **Given** the chosen option, **When** it's applied (Cargo.toml edits and/or visibility changes and/or doctest), **Then** `cargo build --workspace` and `cargo test --workspace` pass and the CLI binary still has all subcommands
- [ ] **Given** the choice was option 1 (cargo feature `cli`), **When** I run `cargo tree -p media-control-daemon`, **Then** `reqwest` is absent from the resolved dependency tree

## Technical Notes

**Option comparison table** (also in unit brief; reproduced for the construction agent):

| Option | Pros | Cons |
|--------|------|------|
| Cargo feature `cli` gating workflow + jellyfin | Eliminates `reqwest` from daemon's dep tree; clean compile-time isolation | Adds `--features cli` to CLI builds; conditional compilation noise in lib |
| `pub(crate)` + `cli` facade module | No feature noise; module-system enforcement | `reqwest` still in lib's dep set even for daemon builds |
| `compile_fail` doctest | Zero structural change | Catches violations at test time, not build time |

**Project convention check**: Before deciding, grep for existing `[features]` blocks across `crates/*/Cargo.toml` and existing `pub(crate)` usage in `media-control-lib`. Pick the option that matches existing style.

**Default recommendation if no signal**: Option 1 (cargo feature). It's the only option that also strips `reqwest` from the daemon's binary, which is real value beyond the symbolic boundary.

## Dependencies

### Requires

- All stories in unit 001 (commands-regrouping) — the `commands::workflow` module must exist before it can be gated

### Enables

- 002-prove-isolation

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `jellyfin.rs` is at lib root, not under `commands/workflow/` | The chosen mechanism must gate `jellyfin.rs` too (or accept that `commands::workflow` being unreachable from the daemon is sufficient, since nothing else uses `jellyfin.rs`) |
| The CLI uses a workflow command in a test | Tests must be feature-gated alongside the modules they exercise |
| `cargo check --workspace` runs without `--features cli` | Must succeed; daemon-only build is the default; CLI build is opt-in via `--features cli` (if option 1 chosen) |

## Out of Scope

- The verification test (story 002)
- Splitting the lib into multiple crates (explicit non-goal)
