---
id: 023-audit-handler-fixes
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: planned
stories:
  - fullscreen-pin-dead-address-guard
  - close-suppress-avoider-before-dispatch
  - avoid-scratchpad-monitor-guard
  - chapter-direction-case-insensitive
  - focus-launch-shlex-no-shell
created: 2026-04-23T00:00:00Z
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 2
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 023-audit-handler-fixes

### Objective
Cluster of small fixes across command handlers. Each fix is in its own file,
but they're grouped because each is too small to be its own bolt.

### Stories Included

- [ ] **fullscreen-pin-dead-address-guard** — `fullscreen.rs:316` (in
  `restore_after_fullscreen_exit`) — when `fresh_window` is `None` (window died
  mid-exit), `pin_action(addr)` is still dispatched against the dead address.
  Wrap the pin restore in `if fresh_window.is_some() && should_restore_pin && !current_pinned`
  so the pin dispatch only fires when the window still exists. The reposition
  is already gated correctly.

- [ ] **close-suppress-avoider-before-dispatch** — `close.rs:124,139` dispatch
  `closewindow` without first calling `suppress_avoider()`. The avoider daemon
  sees the closeevent and races to reposition siblings. Add a `suppress_avoider()`
  call before each `closewindow` dispatch (and any related kill calls).

- [ ] **avoid-scratchpad-monitor-guard** — `avoid.rs:443` — `focused.monitor`
  can be `-1` for windows on the scratchpad workspace; `is_single_workspace`
  computes wrong, causing spurious repositioning. Early-return when
  `focused.monitor < 0` so scratchpad windows are ignored entirely.

- [ ] **chapter-direction-case-insensitive** — `chapter.rs:39` —
  `ChapterDirection::parse` uses literal match arms (`"next"`, `"Next"`,
  `"prev"`, `"Prev"`, `"previous"`, `"Previous"`). `Direction::parse` in
  `move_window.rs` uses `eq_ignore_ascii_case`. Migrate `ChapterDirection::parse`
  to `eq_ignore_ascii_case` for consistency. Update the `"NEXT" → None` test
  to assert the new accepting behavior.

- [ ] **focus-launch-shlex-no-shell** — `focus.rs:113` invokes `sh -c` on the
  user-supplied `--launch` string, exposing full shell metacharacter expansion.
  Refactor: `shlex::split(launch_cmd)` → `Command::new(args[0]).args(&args[1..])`.
  This eliminates the injection surface for non-shell-needing launches. If
  shlex returns `None` or empty vec, return `MediaControlError::InvalidArgument`.
  Add `shlex` as a workspace dep if not already present.

### Expected Outputs
- 5 files touched, each fix isolated
- New tests for each fix where feasible
- `cargo check --workspace` clean
- `cargo test --workspace` clean

### Dependencies
None.

### Notes
focus-launch story is a behavior change for users who relied on shell expansion
in --launch. Document in the next changelog entry. shlex tokenization is the
correct default; users wanting shell features can always invoke `sh -c "..."`
themselves as the launch command.
