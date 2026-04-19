---
intent: 011-mpv-shim-integration
phase: inception
status: context-defined
updated: 2026-03-28T10:00:00Z
---

# mpv-shim Integration - System Context

## System Overview

media-control gains new integration points with the Rust mpv-shim: a query socket for fast cached lookups and new IPC commands for multi-store routing. All changes are additive; existing IPC protocol remains backward compatible.

## External Integrations

- **mpv IPC socket** (`/tmp/mpvctl-jshim`): Existing script-message commands + new `switch-store`, `queue-info`
- **Query socket** (`/tmp/mpv-shim-query.sock`): New — sub-ms cached item lookups, JSON protocol
- **Jellyfin HTTP API**: Existing — becomes fallback when query socket unavailable

## High-Level Constraints

- Backward compatible with Python shim
- Query socket is optional (graceful fallback)
- Low priority — shim must be stable first
