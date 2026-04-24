---
unit: 002-daemon-substrate-tightening
bolt: 026-daemon-substrate-tightening
stage: model
status: complete
updated: 2026-04-26T00:00:00Z
---

# Static Model — daemon-substrate-tightening

## Note on DDD framing

This bolt's "domain" is module dependencies and build-time enforcement, not business entities. The DDD template is adapted here: "entities" are the modules participating in the boundary, "value objects" are the dependency relationships, "domain rules" are the invariants we want enforced, and "domain events" are the build-time signals we want to fire on violation. The intent is to keep the ddd-construction-bolt's stage discipline while honestly mapping the template to a boundary-enforcement task.

## Bounded Context

The daemon's compile-time dependency surface. Specifically: which lib modules `media-control-daemon` is allowed to reach into, and what mechanism fails the build when that contract is violated. The boundary lives at the workspace's crate-graph and module-tree level — it does not appear at runtime.

## Domain Entities

| Entity | Properties | Business Rules |
|---|---|---|
| `media-control-daemon` crate | binary; depends on `media-control-lib` | MUST resolve only items in the substrate + window subtree; importing workflow/jellyfin items MUST fail at build time |
| `media-control-lib::commands::shared` | `CommandContext`, `runtime_dir`, `async_env_test_mutex` | Always reachable from daemon; always reachable from CLI; the legitimate dual-use surface |
| `media-control-lib::commands::window` | 7 command modules + window-internal helpers | Reachable from daemon and CLI; this is what the daemon actually uses |
| `media-control-lib::commands::workflow` | 7 command modules + mpv-IPC plumbing | Reachable from CLI only after this bolt; daemon import MUST be a build-time error |
| `media-control-lib::jellyfin` | HTTP client (`reqwest`-using) | Reachable from CLI only after this bolt; ideally absent from daemon's resolved dep tree |
| `media-control` (CLI binary) | binary; depends on `media-control-lib` | Reachable surface: substrate + window + workflow + jellyfin; behavior unchanged by this bolt |
| `media-control-lib::commands` (top-level) | shim re-exports added by bolt 025 | Today re-exports everything via `pub use {shared,window,workflow}::*`; the shim is a known leak path the enforcement mechanism must close |

## Value Objects

| Value Object | Properties | Constraints |
|---|---|---|
| **Allowed daemon import** | `(daemon, target_module)` where `target_module ∈ {shared, window, hyprland, window.rs, config, error, test_helpers}` | Compiles |
| **Forbidden daemon import** | `(daemon, target_module)` where `target_module ∈ {workflow, jellyfin}` or any item re-exported through them | MUST fail at build (or build-and-test) time |
| **Allowed CLI import** | `(cli, target_module)` where target is anything reachable from the lib | Compiles; CLI surface is the union |
| **Daemon dep set** | `cargo tree -p media-control-daemon` resolved transitive deps | If the chosen mechanism strips workflow-only deps: `reqwest` MUST be absent |

## Aggregates

| Aggregate Root | Members | Invariants |
|---|---|---|
| **Boundary contract** | the chosen enforcement mechanism + the verification check | One mechanism is in force; one verification proves it; both run in CI on every push; CLI binary remains fully functional |
| **Re-export surface** | the `pub use` lines in `commands/mod.rs` | Whatever survives must not silently re-expose workflow items to the daemon |

## Domain Events

| Event | Trigger | Payload |
|---|---|---|
| `BoundaryViolation` | A future contributor adds `use media_control_lib::commands::workflow::...` (or any path that resolves to a workflow item) to a daemon source file | `cargo build` (or `cargo test`, depending on mechanism) fails with a diagnostic that names the offending import |
| `BoundaryConfirmed` | CI runs the verification check | Pass — daemon's dep tree / module reach is what the contract says it is |
| `ShimRegression` | A future contributor adds a `pub use workflow::X;` to `commands/mod.rs` that re-leaks a workflow item under the top-level `commands::X` shim | The verification check catches it (because the daemon's view of the violating top-level path resolves through the workflow re-export) |

## Domain Services

| Service | Operations | Dependencies |
|---|---|---|
| **EnforcementMechanism** | `apply()` — install the chosen mechanism into the workspace; `verify(daemon) -> bool` — produce pass/fail signal | Cargo / `rustc` module-resolution behavior; possibly `cargo tree` for dep-set inspection |
| **CIVerification** | `run() -> pass\|fail` — invoke the verification on every push | Forgejo workflow runner; `cargo` toolchain |

## Repository Interfaces

Not applicable — no persistent state. The "store" is the workspace itself; cargo's resolver is the read path.

## Ubiquitous Language

| Term | Definition |
|---|---|
| **Substrate** | The dual-use lib surface: `commands::shared` plus `hyprland`, `window` (the file, not the subnamespace), `config`, `error`, `test_helpers` |
| **Boundary** | The compile-time dependency contract that the daemon respects only `substrate + commands::window`; everything else is forbidden |
| **Enforcement mechanism** | The Rust-language device that turns a forbidden import into a compile failure: cargo feature flag, `pub(crate)` + facade module, or `compile_fail` doctest |
| **Verification** | A CI-runnable check that proves the boundary is in force right now (catches both unimplemented enforcement and silent regressions) |
| **Shim leak** | The risk path created by bolt 025's `pub use` re-exports in `commands/mod.rs` — a workflow item appearing under a top-level `commands::X` path that the daemon could reach |
| **Workflow** | `commands::workflow::*` and `jellyfin.rs` — everything CLI-only |
| **Window-mgmt** | `commands::window::*` — everything daemon-relevant |
