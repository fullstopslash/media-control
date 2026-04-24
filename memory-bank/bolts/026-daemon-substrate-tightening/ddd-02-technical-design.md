---
stage: design
bolt: 026-daemon-substrate-tightening
created: 2026-04-26T00:00:00Z
---

## Technical Design — daemon-substrate-tightening

### Architecture Pattern

Compile-time module-boundary enforcement, with verification embedded in the standard test command.

The boundary is enforced by **`compile_fail` doctests inside `crates/media-control-daemon/src/main.rs`**. Each doctest attempts an import that the contract forbids (`commands::workflow::*`, `jellyfin::*`, and the shim-leaked top-level paths the daemon must not reach). If any of these imports compile, the doctest fails, which fails `cargo test --workspace`.

### Decision: Option 3 — `compile_fail` doctest

The inception left three options open. Survey of project state changed which one fits:

#### Option 1: Cargo feature `cli`

- **Pros**: Strips `reqwest` from `cargo tree -p media-control-daemon` (real binary-size win, ~2 MB).
- **Cons**:
  - **Zero precedent**: There are no `[features]` blocks anywhere in the workspace. Adding one for a single boundary is style noise without parallel.
  - **Wide surface**: Edits all 3 `Cargo.toml` files, plus `commands/mod.rs` shims need `#[cfg(feature = "cli")]` gates, plus the CLI binary needs `--features cli` everywhere it's built (or the lib's default-features must flip).
  - **Brittle verification**: A `cargo tree` parser breaks across rust-toolchain versions; output format is not stable.

#### Option 2: `pub(crate)` + `cli` facade module

- **Status: structurally infeasible** in this workspace.
- The CLI and daemon are separate *crates*, not modules. `pub(crate)` only restricts visibility within the lib crate — both binaries are external to it, so `pub(crate)` would hide `commands::workflow` from *both* binaries (defeating the CLI).
- To make Option 2 work, the lib would need to *conditionally* re-export workflow under a `pub` path the CLI can see but the daemon cannot. The only Rust mechanism for "visible to one external crate but not another" is a feature flag — which collapses Option 2 into Option 1.
- Eliminated.

#### Option 3: `compile_fail` doctest (chosen)

- **Pros**:
  - **Zero structural change** to the workspace. No `Cargo.toml` edits. No `cfg` gates.
  - **Runs where verification can run today**: `cargo test --workspace`. The project has no CI yet (no `.forgejo/workflows/`, no `.github/workflows/`), so a check that piggybacks on the existing test command is the only check that catches drift right now. When CI is eventually added (per global CLAUDE.md guidance for projects shipping installable Nix packages), this doctest runs automatically as part of any `cargo test`-shaped step.
  - **Easy to extend**: Adding a new forbidden-path doctest is one block, not a feature-table edit.
  - **Catches shim leaks too**: Because the daemon doctest can attempt the top-level shim path (`use media_control_lib::commands::send_mpv_script_message;`) and assert it fails, the leak path bolt 025 left open gets closed.
- **Cons**:
  - Catches violations at `cargo test` time, not `cargo build` time. ~5 second later signal.
  - Doctest output is noisier than a build error, but only on failure (which should be rare).
  - **Does not strip `reqwest` from the daemon's resolved dep tree.** If binary size becomes pressing later, layering Option 1 on top is still possible.

### Why the `reqwest` strip is acceptable to forgo

- Daemon binary today is ~12 MB stripped. `reqwest`'s contribution is ~2 MB. The daemon runs once per session and is never user-visible at startup time; size is not a hot constraint.
- The strip is a binary-output benefit; the *boundary correctness* property (the load-bearing one for intent 015) is satisfied by Option 3 just as well as by Option 1.
- If the strip becomes valuable, Option 3 doesn't preclude adding Option 1 later — they compose. Option 3 alone is the smallest-step right answer.

### Layer Structure

Bolt 026 doesn't introduce layers — it adds a verification layer at the daemon crate boundary. Conceptually:

```text
┌──────────────────────────────────────────────┐
│  media-control-daemon                        │
│   ├── source code                            │
│   └── boundary doctests  ← Bolt 026 adds     │
│       (compile_fail proofs)                  │
└──────────────────────────────────────────────┘
                       │ depends on
                       ▼
┌──────────────────────────────────────────────┐
│  media-control-lib                           │
│   ├── commands::shared    ← daemon may use   │
│   ├── commands::window    ← daemon may use   │
│   ├── commands::workflow  ← daemon may NOT   │
│   ├── commands::* shims   ← daemon may NOT   │
│   │   (workflow leaks)                       │
│   └── jellyfin            ← daemon may NOT   │
└──────────────────────────────────────────────┘
```

The doctests are physically located *in the daemon crate's `main.rs` module-level docs*, so they execute against the daemon's exact view of the lib (not the lib's own tests). This catches "the daemon can compile this import" precisely.

### API Design

No public API change. The verification is internal to the daemon crate's doc-tests.

The doctest block (concept; exact text decided at Stage 4):

```rust
//! # Boundary contract
//!
//! These compile_fail doctests prove the daemon cannot reach into the
//! workflow / jellyfin side of media-control-lib. If a future change makes
//! any of these imports compile, the build's test step fails — surfacing the
//! regression in the same `cargo test --workspace` invocation contributors
//! and CI run today.
//!
//! ```compile_fail
//! use media_control_lib::commands::workflow::mark_watched;
//! ```
//!
//! ```compile_fail
//! use media_control_lib::jellyfin;
//! ```
//!
//! ```compile_fail
//! use media_control_lib::commands::send_mpv_script_message;  // shim leak path
//! ```
```

The third doctest is the one that closes bolt 025's `pub use` shim hole: even if `commands/mod.rs` re-exports `workflow::send_mpv_script_message` under a top-level `commands::*` path, the daemon must not reach it.

### Data Model

Not applicable — boundary verification has no persistent state.

### Security Design

This bolt is itself a security-adjacent concern: it prevents the daemon (a long-running process) from accidentally pulling in HTTP-client code (`reqwest`) it has no business shipping. Even though Option 3 doesn't strip `reqwest` from the binary, it ensures no daemon code path can reach Jellyfin HTTP calls — a meaningful defense-in-depth property.

### NFR Implementation

| NFR | Approach |
|---|---|
| Daemon depends only on substrate + window (FR-2) | Three `compile_fail` doctests in `daemon/src/main.rs` proving the negative cases |
| Verification runs in CI (NFR from inception) | Verification runs in `cargo test --workspace`. There is no CI yet. When CI is added (recommended by global guidance), it will run `cargo test` as standard, so the doctests will run there too. |
| `cargo build --workspace` and `cargo test --workspace` pass | Both must pass after the doctests are added. The doctests intentionally fail their inner compile attempts — but `compile_fail` makes that the *expected* outcome, so the test itself passes. |

### Plan integrations

- **AGENTS.md**: Add a one-paragraph note under a "Boundary contracts" section explaining what the doctests do and what to do if one fires (probably: don't add the import; if you genuinely need to bridge daemon ↔ workflow, that's a scope question for a new intent, not a fix).
- **No CI integration needed today.** When a future intent adds CI per the global Attic-CI pattern, the existing `cargo test --workspace` step will run the doctests.

### Implementation footprint (preview of Stage 4)

- 1 file modified: `crates/media-control-daemon/src/main.rs` (add `//! ```compile_fail\n... ` block to the module-level doc)
- 1 file modified: `AGENTS.md` (one paragraph)
- 0 `Cargo.toml` edits
- 0 source-code logic changes

Total: ~30 lines added across 2 files.

### Risks

- **`compile_fail` doctest false positives**: A `compile_fail` doctest that *does* fail to compile (for any reason — typo, unrelated breakage, or future rustc behavior change) reports as "passing." That's the contract — `compile_fail` only fails when the body *succeeds* in compiling. To detect "the doctest is testing the wrong thing" cases, each doctest has an explicit comment naming what it's proving. If the import path the doctest references no longer exists at all, the doctest still passes (the import fails to resolve, which is a compile failure, which is what `compile_fail` expects). This is acceptable for our boundary case: if `commands::workflow` is renamed or removed, that's a separate intent's decision and the doctest can be updated then. The doctest catches the case it's designed for: "someone made an import that should be impossible *possible*."
- **Doctest discovery**: doctests only run when `cargo test --workspace` is invoked. A contributor running just `cargo build` won't see the failure. The mitigation is that `cargo test` is the standard pre-commit / pre-push step (covered by `just lint` once that's wired up — currently the project has no `justfile`, but global CLAUDE.md mandates one with a `lint` recipe).
