---
unit: 002-daemon-substrate-tightening
intent: 015-avoider-carveout
phase: inception
status: ready
created: 2026-04-26T00:00:00Z
updated: 2026-04-26T00:00:00Z
---

# Unit Brief: daemon-substrate-tightening

## Purpose

Make the daemon's "no workflow, no Jellyfin" property a contract the build enforces, not a discipline contributors must remember. Today the daemon happens to import only what it needs; after this unit, importing anything else will fail to compile.

## Scope

### In Scope

- Survey the three enforcement options against project conventions, pick one, apply it:
  1. **Cargo features.** Gate `commands::workflow` (and the `jellyfin` module) behind a `cli` feature in `media-control-lib/Cargo.toml`. Daemon's lib dependency declares `default-features = false`.
  2. **Module visibility.** Make `commands::workflow` `pub(crate)` and re-export only through a `media_control_lib::cli` facade module that the daemon doesn't use.
  3. **`compile_fail` doctest.** Keep modules public; add a `compile_fail` test in the daemon crate root proving `use media_control_lib::commands::workflow::...` doesn't compile.
- Apply the chosen mechanism
- Add a verification test (form depends on choice)
- Verify with `cargo tree -p media-control-daemon` (and confirm `reqwest` is absent from the daemon's resolved deps if option 1 is chosen)
- Document the decision in the bolt's design-stage notes

### Out of Scope

- Any logic changes in the daemon or lib
- Restructuring the workspace
- Splitting `media-control-lib` into multiple crates (explicit non-goal of the intent)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-2 | Daemon depends only on substrate + window commands; importing workflow is a compile error | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| Enforcement mechanism | The chosen approach (feature flag / visibility / compile_fail) | One of three options, decided in bolt design stage |
| Verification test | Proof in CI that the boundary holds | Form depends on mechanism: doctest, build assertion, or `cargo tree` check |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| Apply mechanism | Edit `Cargo.toml` features and/or add `pub(crate)` and/or write doctest | mechanism choice | enforced boundary |
| Verify | `cargo tree -p media-control-daemon`; review for absence of workflow modules and (if feature-gated) `reqwest` | none | pass/fail |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 2 |
| Must Have | 2 |
| Should Have | 0 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-pick-and-apply-enforcement | Decide between feature-flag / visibility / compile_fail; apply the choice | Must | Planned |
| 002-prove-isolation | Add the verification test; confirm `cargo tree` shows the expected dep set | Must | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| 001-commands-regrouping | The `commands::window` / `commands::workflow` boundary must exist before it can be enforced |

### Depended By

| Unit | Reason |
|------|--------|
| 003-avoider-cleanup | Daemon-owned hot-path state (FR-4) is safer to add once the daemon's surface is contractually sealed |

### External Dependencies

None.

---

## Technical Context

### Suggested Technology

Rust cargo features and/or module visibility. No new crates.

**Decision input for bolt design stage:**

| Option | Pros | Cons |
|--------|------|------|
| Cargo feature `cli` | Eliminates `reqwest` from daemon's dep tree; clean compile-time isolation | Adds `--features cli` to CLI builds; conditional compilation noise in lib |
| `pub(crate)` + `cli` facade module | No feature noise; module-system enforcement | `reqwest` still in lib's dep set even for daemon builds |
| `compile_fail` doctest | Zero structural change | Catches violations at test time, not build time; doctest is fragile |

The bolt picks one. If the project already uses cargo features elsewhere, prefer consistency.

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `media-control-lib/Cargo.toml` | Build config | features section (if option 1) |
| `media-control-daemon/Cargo.toml` | Build config | `default-features = false` (if option 1) |
| `media-control/Cargo.toml` | Build config | `features = ["cli"]` (if option 1) |

---

## Constraints

- **Cannot break the CLI.** Whatever mechanism is chosen, `cargo build --workspace` must produce a working `media-control` binary with all subcommands intact.
- **Verification must run in CI**, not just locally. The boundary is worth the build-time cost only if it actually catches drift.

---

## Success Criteria

### Functional

- [ ] One enforcement mechanism applied
- [ ] Adding `use media_control_lib::commands::workflow::...` to `crates/media-control-daemon/src/main.rs` causes a build failure (or a doctest failure, depending on mechanism)
- [ ] `cargo build --workspace` and `cargo test --workspace` pass

### Non-Functional

- [ ] If feature-flag option chosen: `cargo tree -p media-control-daemon` does not include `reqwest` in its resolved tree
- [ ] Decision is documented in the bolt's construction log with a one-paragraph rationale

### Quality

- [ ] CI runs the verification check on every push

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 026-daemon-substrate-tightening | DDD-style (one design choice + apply + verify) | 001, 002 | Pick mechanism, apply, prove |

---

## Notes

This unit is intentionally small but high-leverage. Its value is preventing future regressions, not changing today's behavior. The decision between the three options is the entire substantive content; the application is mechanical.
