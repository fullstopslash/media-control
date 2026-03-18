---
story: 003-test-context
unit: 001-mock-infrastructure
intent: 001-test-and-refactor
priority: Must
estimate: S
---

## Story: CommandContext Test Constructor

### Technical Story
**Description**: Create a way to build a `CommandContext` for tests with a mock HyprlandClient and configurable Config, without needing real env vars or config files.
**Rationale**: Every E2E command test needs a CommandContext. Currently `CommandContext::new()` requires HYPRLAND_INSTANCE_SIGNATURE and a config file on disk.

### Acceptance Criteria
- [ ] Given a mock socket path and a Config, When creating a test CommandContext, Then it connects to the mock <!-- tw:91829f27-208f-4e03-b7bb-a01716495511 -->
- [ ] Config can be customized per test (different positions, patterns, overrides) <!-- tw:02e65642-004e-4f23-a308-8eb74c827f30 -->
- [ ] Window matcher is compiled from the test config's patterns <!-- tw:cba34967-0a69-4342-bf11-5d8a72537ff0 -->
- [ ] Helper provides sensible defaults so simple tests don't need full config setup <!-- tw:b32078f4-1109-4921-b97e-4e344508700e -->

### Technical Notes
- Add a `CommandContext::for_test(hyprland: HyprlandClient, config: Config)` or similar
- `HyprlandClient::with_socket_path` already exists behind `#[cfg(test)]` - may need to make it `#[cfg(any(test, feature = "test-support"))]` or just public

### Dependencies
- 001-mock-server
