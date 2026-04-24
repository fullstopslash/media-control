---
last_updated: 2026-04-26T00:00:00Z
total_decisions: 1
---

# Decision Index

This index tracks all Architecture Decision Records (ADRs) created during Construction bolts.
Use this to find relevant prior decisions when working on related features.

## How to Use

**For Agents**: Scan the "Read when" fields below to identify decisions relevant to your current task. Before implementing new features, check if existing ADRs constrain or guide your approach. Load the full ADR for matching entries.

**For Humans**: Browse decisions chronologically or search for keywords. Each entry links to the full ADR with complete context, alternatives considered, and consequences.

---

## Decisions

### ADR-001: Enforce daemon ↔ workflow boundary via cargo feature + grep-based integration test
- **Status**: accepted
- **Date**: 2026-04-26
- **Bolt**: 026-daemon-substrate-tightening (002-daemon-substrate-tightening)
- **Path**: `bolts/026-daemon-substrate-tightening/adr-001-daemon-boundary-via-feature-and-greptest.md`
- **Summary**: Intent 015 requires the daemon cannot import workflow / Jellyfin code from the lib, with the property build-enforced rather than habit-enforced. Use a non-default `cli` cargo feature on `media-control-lib` (gating `commands::workflow`, `jellyfin`, workflow shims, optional `reqwest`) plus a `tests/boundary.rs` integration test in the daemon crate that scans daemon source for forbidden import patterns. The ADR also documents why a `compile_fail`-doctest-only approach was empirically falsified.
- **Read when**: Working on the daemon's dependency surface or considering daemon ↔ lib coupling; adding new commands to either `commands::window` or `commands::workflow` (consider whether the workflow side needs `#[cfg(feature = "cli")]` gating); evaluating whether to extend or remove cargo features in this project (this ADR sets the first-feature precedent); designing CI for this project (recommend a single-package `cargo build -p media-control-daemon` step in addition to `cargo test --workspace`); reviewing or extending the `commands/mod.rs` shim re-exports (workflow shims must stay `cli`-gated to keep the feature meaningful).
