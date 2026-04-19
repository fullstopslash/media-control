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
- [ ] move left: dispatches movewindowpixel with x_left, current y <!-- tw:bfed3212-ebba-4afc-9b25-bf92448039ab -->
- [ ] move right: dispatches movewindowpixel with x_right, current y <!-- tw:b04653c8-d64d-4bc6-b44e-54d176058177 -->
- [ ] move up: dispatches movewindowpixel with current x, y_top <!-- tw:d2019be6-3492-4ad4-8b94-059c490f5ff5 -->
- [ ] move down: dispatches movewindowpixel with current x, y_bottom <!-- tw:bc3e2a58-9b7b-4d5f-acff-2bc6784ec515 -->
- [ ] All moves also dispatch resizewindowpixel with configured width/height <!-- tw:61b8a9b0-0afd-4429-8423-d513d15c3dbd -->
- [ ] No media window: silent no-op <!-- tw:3d1356e8-db6b-45ec-90d2-809ccb924261 -->

**Pin-and-float:**
- [ ] Toggle on: unfloated+unpinned → float + pin + position to default corner <!-- tw:06e70501-fb5e-4e32-a66f-6ebafe3cd73a -->
- [ ] Toggle off: floated+pinned → unpin + unfloat <!-- tw:17256dd3-e9c4-4f7b-b37b-a66ed2ae12a0 -->
- [ ] Fullscreen guard: fullscreen window → no-op <!-- tw:d394d654-3e77-4cab-ae0e-88bfe4efae56 -->
- [ ] No media window: silent no-op <!-- tw:384bcc2e-89ff-4903-9893-1fb65088c02e -->

**Close:**
- [ ] mpv class: calls playerctl stop (best effort), does NOT killwindow <!-- tw:8eb1847f-0671-4d14-b163-36f2da6244bb -->
- [ ] jellyfin class: dispatches killwindow <!-- tw:36e5d7bd-2773-48a5-b9ae-4acf60275951 -->
- [ ] firefox PiP: returns error <!-- tw:6bd4b76a-fbf1-4272-bd36-bf7e55d717c9 -->
- [ ] other class: dispatches killwindow <!-- tw:6f32fb61-fe58-42db-99d3-636c8f5e9fd4 -->
- [ ] No media window: silent no-op <!-- tw:b1f680e0-b6d0-4dda-be0b-243af951128b -->

**Focus:**
- [ ] Media window exists: dispatches focuswindow <!-- tw:afd9d9ba-db32-4752-8df6-20a19114293a -->
- [ ] No media window + launch cmd: spawns the launch command <!-- tw:bf759d4e-b564-4b8d-b610-b8f33163e11b -->
- [ ] No media window + no launch cmd: returns Ok(false) <!-- tw:76758cf0-56ee-4dee-b1f7-43112e8b5ef6 -->

### Dependencies
- 001-mock-infrastructure (all stories)
