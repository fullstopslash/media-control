---
story: 002-command-capture
unit: 001-mock-infrastructure
intent: 001-test-and-refactor
priority: Must
estimate: S
---

## Story: Command Capture and Assertion

### Technical Story
**Description**: The mock server should record all commands it receives so tests can assert on what Hyprland commands were dispatched.
**Rationale**: Tests need to verify that "move right" sends the correct movewindowpixel command, not just that it doesn't error.

### Acceptance Criteria
- [ ] Given a mock server, When multiple commands are sent, Then all commands are captured in order <!-- tw:637895ae-712d-4638-bdab-e62c041dfbd5 -->
- [ ] Captured commands can be filtered (e.g., "show me only dispatch commands") <!-- tw:16f927fb-f57a-4288-9bff-a45b85a6179c -->
- [ ] Captured commands include the full command string (including batch prefix if present) <!-- tw:89f55dc3-853e-41d8-82b0-6c19212f399a -->
- [ ] Capture state can be cleared between test phases <!-- tw:47291be8-873c-4616-90e3-e55274a9abc3 -->

### Technical Notes
- Use `Arc<Mutex<Vec<String>>>` shared between mock server task and test code
- Consider a helper like `assert_dispatched("focuswindow address:0x1")` for ergonomic assertions

### Dependencies
- 001-mock-server
