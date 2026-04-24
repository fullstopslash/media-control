---
intent: 015-avoider-carveout
phase: inception
status: complete
created: 2026-04-26T00:00:00Z
updated: 2026-04-26T00:00:00Z
---

# Requirements: Avoider Daemon Carve-Out

## Intent Overview

`media-control` has evolved into two distinct concerns sharing one workspace:

1. **Window-avoidance daemon** — long-running, event-driven, Hyprland-only. Reacts to socket events and repositions floating media windows so they don't overlap the focused window.
2. **Media-server workflow** — short-lived CLI commands that talk to mpv-IPC and the Jellyfin HTTP API (mark-watched, chapter, play, seek, status, etc.).

These have nothing to say to each other at runtime. They share infrastructure (Hyprland IPC, window matching, config, error types) but not semantics. Today they live in one flat `commands/` namespace, which means the daemon imports a module tree that contains `jellyfin.rs` (2275 LOC, `reqwest`-using) even though it never calls into it.

This intent carves the avoider away from the workflow side **inside the existing workspace** so the daemon can be reasoned about, tested, and tuned in isolation — without splitting the repo and without breaking any user-visible CLI surface.

## Type

Refactor.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Daemon binary is provably independent of Jellyfin/workflow code | `cargo tree -p media-control-daemon` shows no path through `jellyfin.rs` or `commands::{jellyfin,mark_watched,chapter,play,seek,status,random}` | Must |
| Avoider hot path is tighter and clearer | Removal of redundant per-event syscalls and re-allocations identified by audit; observed reduction in stat/socket calls per debounced event | Must |
| Future contributors can change one side without reading the other | Module layout makes the boundary obvious; `commands/window/` and `commands/workflow/` are visibly separate | Must |
| No CLI surface change | Existing `media-control` subcommands and flags work identically; existing systemd unit works unchanged | Must |

---

## Functional Requirements

### FR-1: Group commands by concern
- **Description**: Split `crates/media-control-lib/src/commands/` into two submodules: `window` (avoider-relevant: avoid, fullscreen, move_window, pin, minify, focus, close, keep) and `workflow` (CLI-only: mark_watched, chapter, play, seek, status, random). Shared helpers from today's `commands/mod.rs` move to `commands/shared.rs` (or stay in `mod.rs` if scope-limited). `jellyfin.rs` stays at lib root, accessed only from `commands::workflow`.
- **Acceptance Criteria**: `cargo build --workspace` and `cargo test --workspace` pass with no behavior change; `media-control` CLI invocations produce identical results before/after.
- **Priority**: Must
- **Related Stories**: 001-001, 001-002, 001-003

### FR-2: Daemon depends only on substrate + window commands
- **Description**: `crates/media-control-daemon` must import only from `media_control_lib::{config, error, hyprland, window, commands::shared, commands::window}`. Importing `commands::workflow` or `jellyfin` from the daemon must be a compile error.
- **Acceptance Criteria**: Daemon `main.rs` has no `use` referencing workflow modules; an enforcement mechanism (cargo feature flag, module visibility, or compile-time check) prevents accidental coupling.
- **Priority**: Must
- **Related Stories**: 002-001, 002-002

### FR-3: Apply prioritized cleanup pass to avoider hot path
- **Description**: Apply the audit hit list to `avoid.rs` and the daemon event loop. Concretely:
  - Plumb `is_minified()` once per event instead of per-window
  - Collapse the two duplicate `overlaps_focused` closures into a `Rect::overlaps` helper
  - Eliminate the double-dispatch (`classify_case` then `match case`) in `avoid()`
  - Extract `restore_focus_suppressed()` to deduplicate the suppress-then-restore pair
  - Replace the 8-arg `rectangles_overlap` with `Rect`-based call
  - Replace magic numbers (`fullscreen > 0`, `monitor < 0`, `100`) with named constants
  - Move scenario builders out of inline tests into `test_helpers.rs`
- **Acceptance Criteria**: All 7 sub-items landed; `cargo test --workspace` still passes; no observable change in avoidance behavior on real Hyprland.
- **Priority**: Must
- **Related Stories**: 003-001, 003-002, 003-003, 003-004

### FR-4: Daemon-owned hot-path state (where carve-out enables it)
- **Description**: Things the avoider could not do while sharing a flat namespace with stateless CLI commands now become possible: cache `get_clients()` results across debounced events, hold suppress state in memory (`Arc<AtomicU64>`) instead of restat'ing the file every tick, and reuse a `Vec<MediaWindow>` buffer across iterations. Implement these inside the daemon, not the lib, so CLI commands keep their stateless semantics.
- **Acceptance Criteria**: Daemon holds explicit cache state with documented invalidation rules (`openwindow`/`closewindow`/`movewindow` events); CLI commands continue to call the lib statelessly.
- **Priority**: Should
- **Related Stories**: 003-005, 003-006

### FR-5: Test infrastructure stays single-source
- **Description**: The avoider must not grow a parallel mock layer. `test_helpers.rs` remains the one place for Hyprland mock servers, scenario builders, and runtime-dir isolation. New helpers needed for daemon-state tests (FR-4) belong here.
- **Acceptance Criteria**: No new `mod test_helpers` outside `crates/media-control-lib/src/test_helpers.rs`; daemon tests import from the lib's helpers.
- **Priority**: Must
- **Related Stories**: 003-004

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Avoider per-event work | Syscalls per debounced event | Strictly fewer than before — no new syscalls; remove the per-tick `should_suppress` file stat in the hot path (FR-4) |
| Avoider per-event allocations | Heap allocations in the hot loop | Strictly fewer — reuse buffers (FR-4); collapse duplicate Vec collection (audit item 5) |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Behavioral parity | Existing avoidance test scenarios | 100% pass without modification (modulo helper extraction) |
| CLI parity | Existing CLI integration tests | 100% pass without modification |

### Maintainability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Module clarity | Daemon contributor can identify "what the avoider depends on" by reading the daemon's `Cargo.toml` and `use` block | Yes |
| `avoid.rs` size | LOC after cleanup | Strictly smaller; production code (today ~727 LOC) shrinks via DRY items 2/3/6/7/8/11/15 |

---

## Constraints

### Technical Constraints

- **No repo split.** Single `media-control` workspace remains.
- **No new top-level crates.** The carve-out happens via module reorganization inside `media-control-lib` plus tightened daemon dependencies. (If a future need emerges to split the lib into `media-control-core` + `media-control-workflow`, that is a follow-up intent, not this one.)
- **No public CLI surface changes.** Subcommand names, flags, exit codes, and stderr/notify-send messages are preserved.
- **No new runtime dependencies.** Existing crates only.
- **`jellyfin.rs` is not rewritten.** Its only change is becoming reachable only via `commands::workflow`.

### Business Constraints

- **Refactor + focused cleanup, not a rewrite.** No new features. No behavior changes outside the explicit hot-path cleanups in FR-3 and FR-4.

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Daemon currently only imports `CommandContext`, `runtime_dir`, `Config`, `MediaControlError`, `HyprlandError`, `runtime_socket_path`, `commands::avoid::avoid` | Carve-out scope expands; FR-2 enforcement breaks builds | Verified by coupling survey (2026-04-26): exactly these 7 symbols, zero workflow/jellyfin imports |
| `avoid.rs` has zero dependency on workflow commands or jellyfin | Module split forces additional refactor first | Verified by coupling survey: zero cross-cuts |
| Hyprland event sequence numbers (or specific event types) can be used to invalidate the client cache safely | Cache returns stale state; windows fail to avoid | Validate against Hyprland IPC docs in the cleanup bolt; if unreliable, keep cache invalidation conservative (invalidate on any event) |
| `cargo tree`-based enforcement is acceptable for FR-2 | Future contributor adds workflow import without noticing | Add a CI check or a `#[deny]` lint at the daemon crate root; consider a `compile_fail` test that proves workflow imports break |

---

## Out of Scope

- Splitting the repository into two
- Splitting `media-control-lib` into multiple crates
- Rewriting or restructuring `jellyfin.rs`
- Restructuring the systemd unit layout (any change is whatever falls out naturally; not a goal)
- Adding new CLI subcommands or new daemon behaviors
- Cross-platform support (still Linux/Hyprland only)

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| Should FR-2 be enforced via cargo features, module visibility (`pub(crate)` boundaries), or a `compile_fail` doctest? | Construction | Bolt 026 design stage | Pending — intent owner deferred to construction (2026-04-26) |
| Should the cached `get_clients()` invalidation be event-driven (subscribe to `openwindow`/`closewindow`/`movewindow`) or simpler (TTL of one debounce window)? | Intent owner | 2026-04-26 | **Resolved: TTL-of-one-debounce.** Simpler, no Hyprland event-taxonomy assumptions; conservative correctness over peak efficiency. |
