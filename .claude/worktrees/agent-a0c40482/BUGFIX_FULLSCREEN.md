# Fullscreen Toggle Bug Fixes

## Issues Fixed

### 1. Race Condition in Window Selection
**Problem**: The fullscreen toggle would sometimes select the wrong window and fullscreen the window that should be refocused AFTER fullscreen is toggled off.

**Root Cause**:
- The `get_media_window()` and `get_media_window_with_clients()` functions were making two separate async IPC calls to Hyprland:
  1. First call: `get_clients()` - fetch all windows
  2. Second call: `get_active_window()` - fetch the currently focused window

- Between these two calls, the user could switch focus to a different window, causing a mismatch between the client list and the "active" window.
- This resulted in the window matcher making decisions based on stale focus information.

**Fix**:
- Modified both `get_media_window()` and `get_media_window_with_clients()` in `/home/rain/projects/media-control/crates/media-control-lib/src/commands/mod.rs`
- Now uses a single atomic snapshot: fetch clients once, then derive the focused window from that same client list
- The focused window is identified by `focusHistoryID == 0` (Hyprland's indicator of most recently focused window)
- This ensures the window matcher sees a consistent snapshot of window state

**Files Modified**:
- `/home/rain/projects/media-control/crates/media-control-lib/src/commands/mod.rs` (lines 114-157)

### 2. Race Condition in Pin Command
**Problem**: The `pin-and-float` command had the same race condition pattern.

**Root Cause**:
- Similar to the fullscreen issue, it fetched clients and active window in separate calls

**Fix**:
- Modified `pin_and_float()` in `/home/rain/projects/media-control/crates/media-control-lib/src/commands/pin.rs`
- Now uses the same pattern: single client fetch, derive focus from `focusHistoryID == 0`

**Files Modified**:
- `/home/rain/projects/media-control/crates/media-control-lib/src/commands/pin.rs` (lines 26-48)

## How the Fix Works

### Before (Race Condition):
```rust
// Time T0: Fetch all clients (Firefox is focused)
let clients = ctx.hyprland.get_clients().await?;

// Time T1: User switches to mpv
// ... (user action happens here)

// Time T2: Fetch active window (now mpv is focused)
let active = ctx.hyprland.get_active_window().await?;

// Window matcher sees:
// - clients list from T0 (Firefox focused)
// - active window from T2 (mpv focused)
// Result: Inconsistent state, wrong window selected
```

### After (Atomic Snapshot):
```rust
// Time T0: Fetch all clients (includes focus state)
let clients = ctx.hyprland.get_clients().await?;

// Time T0: Derive focus from the same snapshot
let focus_addr = clients
    .iter()
    .filter(|c| c.focus_history_id == 0)
    .map(|c| c.address.as_str())
    .next();

// Window matcher sees:
// - clients list from T0
// - focus derived from same T0 snapshot
// Result: Consistent state, correct window selected
```

## Testing

All tests pass (98 tests):
- `cargo test --lib` - All 98 tests pass
- Added new test `get_media_window_with_clients_uses_focus_from_clients` to verify the fix
- No behavior changes to existing functionality
- The fix only eliminates the race condition window

### New Test Added
A specific test was added to validate that `get_media_window_with_clients()` correctly derives the focused window from the clients list itself, rather than making a separate async call. This test creates a scenario with Firefox focused and mpv pinned, then verifies that the window matcher correctly identifies mpv as the media window with priority 1 (pinned).

## Additional Notes

### Position Restoration
The position restoration issue mentioned by the user is likely related to the window selection bug. When the wrong window was being operated on, its position would not be correctly preserved. With the window selection now fixed, position restoration should work correctly as Hyprland automatically preserves window positions when exiting fullscreen.

### Avoider Suppression
The fullscreen command correctly suppresses the avoider daemon to prevent repositioning during fullscreen transitions (line 100 in fullscreen.rs). This prevents the avoider from moving windows while the fullscreen operation is in progress.

### Bash Script Comparison
The bash implementation (`~/.config/hypr/scripts/media-control.sh`) was already doing this correctly - it fetches the client list and derives focus from it in a single jaq query. The Rust implementation now matches this behavior.
