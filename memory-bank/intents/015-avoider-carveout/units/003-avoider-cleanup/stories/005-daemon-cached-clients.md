---
id: 005-daemon-cached-clients
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: should
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 005-daemon-cached-clients

## User Story

**As a** daemon hot-path performance reader
**I want** the `get_clients()` Hyprland round-trip cached across debounced events using a TTL-of-one-debounce strategy
**So that** back-to-back events within a single debounce window (the common case — `activewindow` fires often) don't re-fetch the full client list and re-parse the JSON, without depending on assumptions about Hyprland's event taxonomy

## Decided Strategy: TTL-of-one-debounce

The cache is valid for the duration of one debounce window (15ms today). The next `avoid()` call after that window expires refetches. Rationale: simpler than event-driven invalidation; no risk of staleness from undocumented client-mutating events; the efficiency win comes from collapsing burst-fired events into one fetch, which TTL covers fully.

## Acceptance Criteria

- [ ] **Given** the daemon's main event loop in `crates/media-control-daemon/src/main.rs`, **When** I add a `ClientCache` (`Mutex<Option<(Instant, Vec<Client>)>>` or equivalent) keyed by capture timestamp, **Then** the cache returns its stored value if `Instant::now() - captured_at < DEBOUNCE_WINDOW`, otherwise refetches
- [ ] **Given** the cache, **When** consecutive events arrive within the same debounce window, **Then** only the first triggers a `get_clients()` call; the rest hit the cache
- [ ] **Given** the cache, **When** more than one debounce window has elapsed since the last fetch, **Then** the next `avoid()` call refetches
- [ ] **Given** stories 003-001 through 003-004 have landed, **When** I write tests for the cache (using `test_helpers.rs` primitives), **Then** they verify: (a) cache hit on consecutive same-window calls, (b) cache miss after TTL expiry, (c) no stale state after expiry, (d) refetch updates the timestamp

## Technical Notes

- The TTL value is the existing debounce window (15ms today, defined in the daemon). Reuse the same constant — don't introduce a second tuning knob.
- The cache lives in the daemon, not the lib. The lib's `get_clients()` stays stateless.
- `Mutex<Option<(Instant, Vec<Client>)>>` is the recommended shape; `ArcSwap` is faster but adds a dep — only switch if benchmarks justify.
- Add a `tracing::debug!` on cache hit/miss/refetch so the operator can see the optimization working in `journalctl --user -u media-control-avoider`.
- Future optimization (out of scope): if the TTL approach proves insufficient, event-driven invalidation can be layered on top in a follow-up intent. The TTL acts as a safety net even then.

## Dependencies

### Requires

- 004-migrate-scenario-builders (test primitives needed)

### Enables

- (none in this intent; future intents may build on cached state)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Hyprland restarts mid-session | Cache entry expires after one debounce window regardless; next fetch reconnects via existing daemon logic |
| A client's geometry changes mid-window | Cache returns the pre-change snapshot for the rest of the window; next window picks up the change. Acceptable: 15ms staleness is invisible to users |
| Two events fire simultaneously | The mutex serializes; one fetches, the other reads from cache |
| Cache miss inside `avoid()` | Refetch synchronously; existing latency; no behavior change vs. today |

## Out of Scope

- Caching mpv state (workflow-only concern)
- Caching `find_media_windows` result (the `Vec<MediaWindow>` reuse is a separate, simpler item; can be folded in here or done as part of the buffer-reuse mention from audit item 5)
- Multi-process cache coherence (only the daemon caches; CLI commands stay stateless)
