---
bolt: 026-daemon-substrate-tightening
created: 2026-04-26T00:00:00Z
status: accepted
---

# ADR-001: Enforce daemon ↔ workflow boundary via cargo feature + grep-based integration test

## Context

Intent 015 (avoider-carveout) requires that `media-control-daemon` cannot import workflow / Jellyfin code from `media-control-lib` — and that this property be a build-enforced contract, not a contributor habit. Bolt 025 (commands-regrouping) created the visible split (`commands::window` vs `commands::workflow`) and added `pub use` shims at `commands::*` for back-compat, leaving a leak path: the daemon could today reach workflow items through the shim.

The inception listed three candidate enforcement mechanisms. **A first-pass technical design recommended Option 3 alone (`compile_fail` doctest), but empirical verification at Stage 4 falsified that approach** — see "Why doctest alone fails" below. The corrected analysis below evaluates the realistic options.

Project state at decision time:

- Workspace has **zero `[features]` blocks** in any `Cargo.toml` — feature-gating is unused as a project pattern (this would be the first)
- **No CI exists yet** (no `.forgejo/workflows/`, no `.github/workflows/`); local `cargo test --workspace` is the only check that runs anything
- The CLI and daemon are **separate crates** in the workspace; both depend on `media-control-lib`
- Workspace uses `resolver = "2"`
- `reqwest` is currently a non-optional dep of `media-control-lib`, used only by `jellyfin.rs` (workflow-side)

## Decision

Implement a two-mechanism enforcement:

1. **Cargo feature `cli` (non-default) on `media-control-lib`** — gates `commands::workflow`, `jellyfin`, the workflow-side `pub use` shims in `commands/mod.rs`, and makes `reqwest` an optional dep. CLI declares `features = ["cli"]` on its lib dep; daemon declares no features.
2. **`tests/boundary.rs` integration test in the daemon crate** — scans `crates/media-control-daemon/src/**/*.rs` for forbidden import patterns (`media_control_lib::commands::workflow`, `media_control_lib::jellyfin`, and plausible variants) and asserts none are present. Runs as part of `cargo test --workspace`.

Together: the feature flag provides type-system enforcement under single-package builds (`cargo build -p media-control-daemon`); the grep test catches workspace-unification escape paths and any future contributor who manages to slip a forbidden import past the feature.

## Rationale

### Why doctest alone fails (corrected analysis)

The first-pass design proposed `compile_fail` doctests in either the daemon's `main.rs` or the lib's source as the sole mechanism. Empirical testing during Stage 4 implementation revealed two blocking issues:

1. **Bin-crate doctests don't run.** `cargo test --workspace` only collects doctests from library targets. A `compile_fail` doctest in `daemon/src/main.rs` is silently never executed. Verified with a minimal reproduction (`/tmp/doctest-bin-test`): `running 0 tests` in the doc-tests section.
2. **Lib doctests can't distinguish daemon-view from CLI-view.** A doctest in the lib runs as if from an external crate, but the lib has *one* public surface that both binaries see equally. A doctest asserting `use media_control_lib::commands::workflow::X` fails to compile would itself fail (the import succeeds — the CLI uses it). The doctest can only express a useful negative if the daemon and CLI see *different* lib surfaces — which only happens with a feature flag (Option 1). And then **cargo workspace feature unification** activates the CLI's feature for the daemon's lib build under `cargo build --workspace`, defeating the purpose. The doctest works only under single-package test invocations (`cargo test -p media-control-lib --no-default-features`), which is not what `cargo test --workspace` runs.

The doctest mechanism therefore adds no enforcement value beyond what the cargo feature provides at single-package build time. It is dropped from the design.

### Alternatives Considered (corrected)

| Alternative | Pros | Cons | Decision |
|---|---|---|---|
| **Cargo feature `cli`** alone | Real type-system enforcement at single-package build (`cargo build -p media-control-daemon`); strips `reqwest` from daemon's binary (~2 MB) | Sets the project's first feature flag (style precedent); workspace unification means `cargo build --workspace` enables `cli` for the daemon's lib build (escape path) | Selected as **structural** layer |
| **`pub(crate)` + `cli` facade** | Uses an idiom present in this codebase | Structurally infeasible: separate crates can't differentiate via `pub(crate)`; collapses into Option 1 | Eliminated |
| **`compile_fail` doctest** alone | Zero structural change | Bin-crate doctests don't run; lib doctests can't distinguish daemon-view; workspace unification defeats the point even if one were placed | Eliminated |
| **`tests/boundary.rs` grep test** alone | Zero structural change; catches the realistic failure mode (contributor types `use commands::workflow::X` in daemon source); robust to workspace unification | Text-based not type-based; theoretically circumventable by clever renaming | Selected as **verification** layer |
| **Cargo feature + grep test** (chosen) | Structural enforcement under single-package builds + verification that catches workspace-unification escapes; defense in depth | Wider blast radius than either alone; sets the feature-flag precedent | Selected — practical gold standard for this workspace |
| **Lib split into multiple crates** | Truly separates the surfaces; no unification gotcha | Explicitly forbidden by the inception's non-goals | Out of scope |

### Why "cargo feature + grep" is the practical gold standard

The honest observation: cargo workspace feature unification is a known limitation. Within the inception's constraint of "no lib split," there is no Rust mechanism that prevents `cargo build --workspace` from enabling `cli` for all consumers of the lib (including the daemon). The practical answer is to combine a structural mechanism that handles the common case (single-package builds + the daemon's CI step) with a verification that handles the escape path (text-based scan of daemon source).

This is genuinely better than either alone:
- **Just the feature**: workspace builds defeat it
- **Just the grep test**: doesn't strip `reqwest`, doesn't enforce at build-time
- **Both**: feature catches the build-time case for single-package and CI; grep test catches workspace-build escape; together they make accidentally adding a forbidden import in the daemon source nearly impossible

## Consequences

### Positive

- The daemon's "no workflow, no Jellyfin" contract is enforced at two layers: structural (feature flag) and behavioral (grep test)
- `cargo build -p media-control-daemon` strips `reqwest` from the daemon's resolved dep tree (~2 MB binary-size win)
- `cargo test --workspace` runs the grep test as part of the standard test command — no new infrastructure needed
- The grep test catches both the unification escape path AND any creative future contributor who tries to add a forbidden import via re-export trickery
- Future CI integration is automatic: any CI step that runs `cargo test` runs the grep test; any CI step that builds the daemon as a single package gets the feature-flag enforcement

### Negative

- Sets the project's first `[features]` block precedent — increases conceptual surface area
- Workspace unification gotcha is now a known thing contributors should understand (documented in `AGENTS.md`)
- `commands/mod.rs` shims for workflow items now require `#[cfg(feature = "cli")]` gates — small amount of conditional-compilation noise
- Three `Cargo.toml` files edited (lib gets the feature, CLI enables it, daemon doesn't)
- Workflow command tests inside `commands::workflow::*` modules need to be feature-gated too (so they only compile when `cli` is on); workflow-only tests will not run under `cargo test -p media-control-lib --no-default-features`

### Risks

- **Workspace-unification escape path**: `cargo build --workspace` builds the daemon's lib with `cli` enabled (because the CLI activates it). A contributor running only workspace-wide builds could add a forbidden import and not see a compile failure until the grep test runs. **Mitigation**: the grep test runs as part of `cargo test --workspace`, which is the standard pre-push check. Documented in `AGENTS.md` so contributors know.
- **Grep test specificity**: the test scans for literal patterns (`commands::workflow`, `jellyfin`). A contributor importing via a renamed path (e.g., re-exported through a third module) could theoretically slip through. **Mitigation**: Stage 5 verification includes a "what the grep test catches and what it doesn't" section in the test report; AGENTS.md documents the surface; future contributors who genuinely need a daemon→workflow bridge must update both the grep test patterns and add a justification in the PR description (signal of deliberate scope expansion).
- **Feature-gated tests**: tests inside `commands::workflow::*` modules become feature-gated; `cargo test -p media-control-lib --no-default-features` won't run them. **Mitigation**: this is fine — `cargo test --workspace` (the standard) does run them because the CLI activates the feature.

## Related

- **Intent**: 015-avoider-carveout
- **Stories**: 002-001 (pick and apply enforcement), 002-002 (prove isolation)
- **Predecessor bolt**: 025-commands-regrouping (created the boundary; introduced the shim-leak risk this ADR closes via the `cli`-gated shims)
- **Successor bolt**: 027-avoider-cleanup (depends on this boundary being firm before adding daemon-only state)
- **Standards impact**: When CI is added to this project (per global CLAUDE.md's Attic-CI pattern guidance), the workflow's standard `cargo test --workspace` step automatically runs the grep test. A future CI improvement worth considering: add a `cargo build -p media-control-daemon` (or `cargo check -p media-control-daemon`) step that exercises the single-package isolation directly.
- **Previous ADRs**: None (this is the project's first ADR; it documents both the decision and the empirical falsification process that led to the corrected design)
