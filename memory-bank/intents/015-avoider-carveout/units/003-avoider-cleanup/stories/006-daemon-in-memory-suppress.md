---
id: 006-daemon-in-memory-suppress
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: should
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 006-daemon-in-memory-suppress

## User Story

**As a** daemon hot-path performance reader
**I want** the daemon to hold its own suppress timestamp in `Arc<AtomicU64>` and consult that first, falling back to the file only when an external (cross-process) writer might have updated it
**So that** the per-event `should_suppress` file stat — which is ENOENT in the common case and yet runs every tick — disappears from the daemon's hot path

## Acceptance Criteria

- [ ] **Given** today's `should_suppress` (avoid.rs:262-298) which stats a file every tick, **When** I add an in-memory suppress state (`Arc<AtomicU64>` storing the last-write-millis) to the daemon and consult it before the file, **Then** the file stat is skipped when the in-memory state is fresh enough to answer
- [ ] **Given** CLI subcommands that warm suppression by writing the file (e.g., `media-control move` calling `suppress_avoider`), **When** the daemon next ticks, **Then** it sees the file's timestamp and updates its in-memory state — the file remains the cross-process IPC medium
- [ ] **Given** the daemon's own warming calls (the suppress-then-restore pair from story 003), **When** they fire, **Then** the in-memory state updates synchronously without a file write (or with a file write that's also still done for consistency — design-stage decision)
- [ ] **Given** the migrated `with_isolated_runtime_dir` primitive from story 004, **When** I write tests for the in-memory suppress, **Then** they verify: (a) in-memory hit path skips file IO, (b) external file write is observed, (c) suppress window timing matches today's behavior

## Technical Notes

- **Design-stage decision**: Should the daemon's own `suppress_avoider` calls still write the file?
  - **Yes (mirror)**: Simpler invariant — file is always authoritative; in-memory is a fast cache. Slight overhead per warm call.
  - **No (in-memory only for self)**: Faster; daemon writes file only when an external observer might care, but there's no consumer of the file other than the daemon itself once it's running. The CLI writes the file when it wants to warm the daemon; the daemon doesn't need to write its own warmings to a file no one else reads.
  - Recommend "no" but verify there's no other consumer (e.g., `mark_watched` warming itself doesn't read the file)
- **Safety**: `Arc<AtomicU64>` is fine for monotonic timestamps. Use `Ordering::Relaxed` for reads (worst case: one extra file stat) and `Ordering::Release`/`Acquire` if establishing happens-before with the file.
- The file-based path stays for cross-process callers and is unchanged in semantics

## Dependencies

### Requires

- 005-daemon-cached-clients (similar shape; staged together)

### Enables

- (none in this intent)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| External tool (not the daemon, not media-control CLI) writes the suppress file | Daemon observes via the fallback file-stat path on the next check that crosses the in-memory freshness window; suppress takes effect |
| Daemon restarts | In-memory state starts empty; first tick falls back to file; behavior matches pre-change |
| Concurrent CLI commands write the file rapidly | Atomic file replacement (which the existing code presumably uses) keeps semantics; the daemon's in-memory state may briefly lag — acceptable since suppress windows are coarse (millisecond-scale) |
| `Ordering::Relaxed` reads see a stale value | Worst case: one extra file stat; never wrong direction (suppress fires when it shouldn't, not vice versa, because we always re-check the file when in-memory says "not suppressed") |

## Out of Scope

- Changing `suppress_avoider`'s file format or atomicity guarantees
- Removing the file-based path entirely (cross-process IPC requires it)
- Adding a kill-switch to disable in-memory suppress (defer until needed)
