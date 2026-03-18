---
story: 001-mock-server
unit: 001-mock-infrastructure
intent: 001-test-and-refactor
priority: Must
estimate: M
---

## Story: Mock Hyprland Socket Server

### Technical Story
**Description**: Create a mock Unix socket server that speaks Hyprland's IPC protocol (connect, write command, read response, disconnect).
**Rationale**: Every E2E test needs to talk to something that behaves like Hyprland.

### Acceptance Criteria
- [ ] Given a ResponseMap with j/clients configured, When HyprlandClient calls get_clients(), Then it receives the configured JSON and parses it correctly
- [ ] Given a ResponseMap with dispatch responses, When HyprlandClient calls dispatch(), Then it receives "ok"
- [ ] Given a [[BATCH]] command, When HyprlandClient calls batch(), Then mock processes it and returns "ok"
- [ ] Mock server runs in a tokio task and cleans up on drop
- [ ] Mock binds to a temp file path (no conflicts between parallel tests)

### Technical Notes
- Use `tokio::net::UnixListener` in a spawned task
- Use `tempfile` crate (already a dev-dependency) for socket paths
- Protocol: read all bytes until connection closes, match against response map, write response

### Dependencies
- None
