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
- [ ] Given a ResponseMap with j/clients configured, When HyprlandClient calls get_clients(), Then it receives the configured JSON and parses it correctly <!-- tw:9a20bd87-91aa-41be-9873-4df9d6bfd6c1 -->
- [ ] Given a ResponseMap with dispatch responses, When HyprlandClient calls dispatch(), Then it receives "ok" <!-- tw:2643f12c-6ad4-4497-882b-d4a7c379569d -->
- [ ] Given a [[BATCH]] command, When HyprlandClient calls batch(), Then mock processes it and returns "ok" <!-- tw:b771969f-5790-4939-80df-088cc9a6e9e7 -->
- [ ] Mock server runs in a tokio task and cleans up on drop <!-- tw:00f25df5-c085-425a-a560-32695d6fb697 -->
- [ ] Mock binds to a temp file path (no conflicts between parallel tests) <!-- tw:558fef6b-99d5-4daa-8488-b89e9c6f89d3 -->

### Technical Notes
- Use `tokio::net::UnixListener` in a spawned task
- Use `tempfile` crate (already a dev-dependency) for socket paths
- Protocol: read all bytes until connection closes, match against response map, write response

### Dependencies
- None
