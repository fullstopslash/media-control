---
unit: 003-avoider-cleanup
bolt: 027-avoider-cleanup
stage: model
status: complete
updated: 2026-04-26T00:00:00Z
---

# Static Model — avoider-cleanup

## Note on DDD framing

The avoider's "domain" is genuine geometry + state-machine work — Rect/overlap, focus context, position resolution, hot-path caching, suppress timing. The DDD template fits this bolt cleanly (unlike bolt 026's boundary-enforcement framing, which had to be adapted).

## Bounded Context

The window-avoidance loop. Spans the avoider's pure logic (`avoid.rs`) and the daemon's hot-path state (`media-control-daemon/src/main.rs`). Excludes everything outside avoidance: window-management commands invoked by the CLI, mpv/Jellyfin workflow, and the substrate (hyprland IPC, window matching, config) which is consumed but not modified.

## Domain Entities

| Entity | Properties | Business Rules |
|---|---|---|
| **`Rect`** (new) | `x: i32, y: i32, w: i32, h: i32` | `overlaps(&Rect) -> bool` uses `i64` widening to avoid overflow on adversarial Hyprland geometry near `i32::MIN/MAX`; degenerate (w/h ≤ 0) overlaps return `false` |
| **`FocusedWindow<'a>`** (refactored) | `client: &'a Client, is_media: bool` | Borrowed view of the focused client; replaces today's by-value copy that mirrors `Client` fields |
| **`PositionResolver<'a>`** (new) | `ctx: &'a CommandContext, minified: bool` | Bundles position-resolution inputs so `get_position_pair`/`calculate_target_position` stop rebuilding closures; methods: `resolve_or(name, default)`, `resolve_opt(name)` |
| **`AvoidCase`** (refactored) | enum variants for the 5 avoidance scenarios | Carries a `dispatch(self, ctx, ...)` method or is inlined into `avoid()` — eliminates today's classify-then-immediately-match double dispatch |
| **`ClientCache`** (new, daemon) | `Mutex<Option<(Instant, Vec<Client>)>>` | TTL-of-one-debounce (15 ms today); reuse the existing `DEBOUNCE_WINDOW` constant; mutex-serialised so concurrent events one-fetch-many-read; logs hit/miss/refetch at debug level |
| **`SuppressState`** (new, daemon) | `Arc<AtomicU64>` (millis) | In-memory mirror of the suppress file; daemon's own warm calls update in-memory directly without writing the file (no other consumer reads daemon's writes); cross-process callers (CLI commands) keep writing the file, daemon picks up via fallback file-stat path |

## Value Objects

| Value Object | Properties | Constraints |
|---|---|---|
| **Named geometry constants** | `FULLSCREEN_NONE: u8 = 0`, `PERCENT_MAX: u16 = 100`, `SCRATCHPAD_MONITOR: i32 = -1` | Replace scattered raw `0`/`100`/`-1` checks (`fullscreen > 0`, `wide_window_threshold.min(100)`, `monitor < 0`); each constant carries a doc-comment naming the Hyprland convention or regression-test backstop it represents |
| **`AvoidInput`** (optional refactor) | `(focused, clients, media_windows, is_single_workspace)` | If extracted from `avoid()`, lets each case-handler be tested in isolation against a synthetic input |

## Aggregates

| Aggregate Root | Members | Invariants |
|---|---|---|
| **Avoidance tick** | `(focused, media_windows, AvoidCase)` | Per `avoid()` invocation: `is_minified()` is read at most once and threaded through all consumers; `get_clients()` is fetched at most once per debounce window (cached); the suppress check consults in-memory state first, falls through to file only on cache miss |
| **Cleanup envelope** | The audit's 15 items + the "what NOT to touch" backstop | Behavior is preserved — no avoidance test passes today and fails after; no overflow-defense `i64` widening is removed; the three diagnostic branches in `should_suppress` survive; the double-suppress in mouseover handlers survives; `fullscreen > 0` is not "simplified" to `== 1` |

## Domain Events

| Event | Trigger | Payload |
|---|---|---|
| `AvoidTick` | Daemon dispatches `avoid()` after debounce | The avoidance loop runs once; cache hit/miss is logged; suppress state checked |
| `ClientCacheRefetch` | TTL elapsed since last fetch | New `Vec<Client>` snapshot with current `Instant` |
| `SuppressTouchedInMemory` | Daemon's own `suppress_avoider`-equivalent fires | `Arc<AtomicU64>` updated; no file write |
| `SuppressFileObserved` | Cross-process write detected via fallback file-stat | In-memory state updated to match file's timestamp |

## Domain Services

| Service | Operations | Dependencies |
|---|---|---|
| **`Rect::overlaps`** | `(self, other) -> bool` (i64-widening) | Replaces 8-arg `rectangles_overlap` and the two duplicate `overlaps_focused` closures |
| **`PositionResolver::resolve_*`** | `resolve_or(name, default) -> i32`, `resolve_opt(name) -> Option<i32>` | `commands::shared::CommandContext`, precomputed `minified` |
| **`restore_focus_suppressed(ctx, addr)`** | Helper combining suppress-then-restore-focus pattern | Replaces the two duplicated 3-line blocks at avoid.rs:615-618 and avoid.rs:651-659 |

## Repository Interfaces

Not applicable — no persistent state. The "store" is the daemon's in-memory cache + the existing on-disk suppress file (unchanged format, unchanged semantics from cross-process callers' perspective).

## Ubiquitous Language

| Term | Definition |
|---|---|
| **Avoidance tick** | One execution of `commands::avoid()` after the daemon's 15 ms debounce |
| **Hit list** | The 15-item prioritized cleanup audit produced during inception (2026-04-26); guides scope but is not exhaustive |
| **TTL-of-one-debounce** | Cache invalidation strategy chosen at intent owner's instruction: cache valid for `DEBOUNCE_WINDOW` (15 ms today); refetch on next event past that window |
| **Shim leak** (carried over from bolt 026) | The risk the `commands::*` shim re-exports created; bolt 026 closed it via cli-gating + grep test; this bolt does not interact with that boundary |
| **Suppress (in-memory)** | Daemon-local `Arc<AtomicU64>` carrying the most recent suppress timestamp the daemon itself wrote; fast path |
| **Suppress (file)** | The on-disk `media-avoider-suppress` file written by CLI commands when warming the daemon; cross-process IPC medium; format unchanged |
| **What NOT to touch** | The audit's explicit backstop: load-bearing-but-ugly code preserved across this cleanup (i64 widening, three diagnostic branches in should_suppress, double-suppress in mouseover, scratchpad early-return, `fullscreen > 0`) |
