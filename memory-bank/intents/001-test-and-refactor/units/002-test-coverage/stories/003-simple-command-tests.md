---
story: 003-simple-command-tests
unit: 002-test-coverage
intent: 001-test-and-refactor
priority: Must
estimate: M
---

## Story: Move, Pin, Close, Focus E2E Tests

### Technical Story
**Description**: Test the simpler commands end-to-end. These have less complex logic but still need verification.
**Rationale**: Complete coverage - every command should have at least happy-path and no-window tests.

### Acceptance Criteria

**Move:**
- [ ] move left: dispatches movewindowpixel with x_left, current y
- [ ] move right: dispatches movewindowpixel with x_right, current y
- [ ] move up: dispatches movewindowpixel with current x, y_top
- [ ] move down: dispatches movewindowpixel with current x, y_bottom
- [ ] All moves also dispatch resizewindowpixel with configured width/height
- [ ] No media window: silent no-op

**Pin-and-float:**
- [ ] Toggle on: unfloated+unpinned → float + pin + position to default corner
- [ ] Toggle off: floated+pinned → unpin + unfloat
- [ ] Fullscreen guard: fullscreen window → no-op
- [ ] No media window: silent no-op

**Close:**
- [ ] mpv class: calls playerctl stop (best effort), does NOT killwindow
- [ ] jellyfin class: dispatches killwindow
- [ ] firefox PiP: returns error
- [ ] other class: dispatches killwindow
- [ ] No media window: silent no-op

**Focus:**
- [ ] Media window exists: dispatches focuswindow
- [ ] No media window + launch cmd: spawns the launch command
- [ ] No media window + no launch cmd: returns Ok(false)

### Dependencies
- 001-mock-infrastructure (all stories)
