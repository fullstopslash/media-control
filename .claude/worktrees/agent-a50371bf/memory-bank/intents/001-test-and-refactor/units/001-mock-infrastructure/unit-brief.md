---
unit: 001-mock-infrastructure
intent: 001-test-and-refactor
phase: inception
status: ready
created: 2026-03-18T13:00:00Z
updated: 2026-03-18T13:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Mock Infrastructure

## Purpose

Build a mock Hyprland IPC server and test harness that enables end-to-end testing of all commands without a running Hyprland instance.

## Scope

### In Scope
- Mock Unix socket server that handles Hyprland's request/response protocol
- Canned response configuration for j/clients, j/activewindow, j/monitors
- Command capture for dispatch, keyword, and [[BATCH]] commands
- `HyprlandClient` constructor that accepts a custom socket path (already exists as `#[cfg(test)]`)
- `CommandContext` test constructor with mock client and configurable config
- Extension of existing test helpers (`make_client`, `make_client_full`)

### Out of Scope
- Hyprland socket2 event stream mocking (daemon events)
- Jellyfin HTTP API mocking
- mpv IPC socket mocking
- playerctl mocking

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Mock Hyprland IPC Infrastructure | Must |

---

## Domain Concepts

### Key Entities
| Entity | Description | Attributes |
|--------|-------------|------------|
| MockServer | Tokio-based Unix socket listener | socket path, response map, captured commands |
| ResponseMap | Maps command prefixes to canned responses | j/clients → JSON, dispatch → "ok" |
| CapturedCommand | Records what commands were sent | command string, timestamp |

### Key Operations
| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| start_mock | Spawn mock server on temp socket | ResponseMap | socket path |
| set_response | Configure response for a command | command prefix, response body | - |
| get_captured | Retrieve commands sent to mock | - | Vec<String> |
| make_test_context | Build CommandContext with mock | Config, ResponseMap | CommandContext |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |
| Should Have | 0 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-mock-server | Mock Hyprland socket server | Must | Planned |
| 002-command-capture | Command capture and assertion | Must | Planned |
| 003-test-context | CommandContext test constructor | Must | Planned |

---

## Dependencies

### Depends On
None - this is the foundation unit.

### Depended By
| Unit | Reason |
|------|--------|
| 002-test-coverage | Needs mock server for E2E tests |
| 003-logic-cleanup | Needs tests as safety net before refactoring |

---

## Constraints

- No new external crate dependencies (use tokio::net::UnixListener directly)
- Must work in `#[tokio::test]` context
- Mock must handle concurrent command calls (batch sends multiple in one connection)

---

## Success Criteria

### Functional
- [ ] Mock server responds to j/clients, j/activewindow, j/monitors with configured JSON <!-- tw:6b4e4285-8b5b-4489-83ce-d94e74704c9b -->
- [ ] Mock server returns "ok" for dispatch/keyword commands <!-- tw:26e4a302-1a97-44d5-80cc-f50b2cc7f8a8 -->
- [ ] Mock server handles [[BATCH]] commands <!-- tw:61ff2e20-1edb-402a-b490-b792777555c2 -->
- [ ] Captured commands can be inspected after test <!-- tw:26d899f2-3237-48bc-88c5-45d756f3514c -->

### Quality
- [ ] All mock infrastructure has its own unit tests <!-- tw:b4fc811d-5b31-4a5f-8c8f-a1a31d4a8f12 -->
- [ ] No flaky tests from socket timing issues <!-- tw:454690b9-9fc6-4fe7-a1ed-43973cb649ed -->
