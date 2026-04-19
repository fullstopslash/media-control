---
intent: 011-mpv-shim-integration
phase: inception
status: complete
created: 2026-03-28T10:00:00.000Z
updated: 2026-03-28T10:00:00.000Z
---

# Requirements: mpv-shim Integration

## Intent Overview

Update media-control to leverage new Rust mpv-shim capabilities. The Rust shim replaces the Python jellyfin-mpv-shim fork with backward-compatible IPC plus new features: query socket for sub-ms cached lookups, multi-store plugin routing, and richer status info.

**Priority: Low** — enhancements only. Current media-control works with the new shim as-is.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Faster item resolution | play command uses query socket (<1ms vs ~100ms HTTP) | Should |
| Multi-store support | switch-store and queue-info commands available | Could |
| Simplified keep routing | Single socket, shim routes internally | Should |
| Richer status output | Store name, queue depth, cache freshness in status | Could |

---

## Functional Requirements

### FR-1: New IPC commands — switch-store and queue-info
- **Description**: Add `switch-store <name>` and `queue-info` script-message support
- **Acceptance Criteria**: Commands sent via existing IPC infrastructure; degrade gracefully on old shim
- **Priority**: Could

### FR-2: Query socket integration for play command
- **Description**: Resolve items via `/tmp/mpv-shim-query.sock` instead of Jellyfin HTTP API
- **Acceptance Criteria**: play command uses query socket when available, falls back to HTTP
- **Priority**: Should

### FR-3: Simplify keep command routing
- **Description**: Keep only sends to `/tmp/mpvctl-jshim`; shim routes to active plugin
- **Acceptance Criteria**: Keep works with new shim (single socket) and standalone mpv
- **Priority**: Should

### FR-4: Enrich status output
- **Description**: Query shim's query socket for store name, queue depth, cache freshness
- **Acceptance Criteria**: New fields in --json output; absent fields when query socket unavailable
- **Priority**: Could

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Query socket latency | Item resolution | < 1ms |
| Backward compatibility | Old shim works | 100% |

---

## Constraints

- Must remain backward compatible with Python shim
- Query socket is optional — fallback to HTTP when unavailable
- New IPC commands are ignored by old shim (no error)

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Rust mpv-shim is stable | Changes premature | Wait for stability before construction |
| Query socket protocol is finalized | API changes | Protocol is simple JSON, easy to update |
