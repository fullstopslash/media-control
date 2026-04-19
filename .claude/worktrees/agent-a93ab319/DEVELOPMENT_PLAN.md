# Media Control Rust Rewrite - Development Plan

## Overview

Rewrite of bash media-control scripts as a Rust application with:
- **Two separate binaries**: `media-control` (CLI) and `media-control-daemon` (event listener)
- **Async runtime**: Tokio for socket operations
- **Config location**: `~/.config/media-control/config.toml`

## Project Structure

```
media-control/
├── Cargo.toml                    # Workspace manifest
├── crates/
│   ├── media-control/            # Main CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   ├── media-control-daemon/     # Daemon binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   └── media-control-lib/        # Shared library
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── hyprland.rs
│           ├── config.rs
│           ├── window.rs
│           ├── jellyfin.rs
│           └── commands/
│               ├── mod.rs
│               ├── fullscreen.rs
│               ├── move_window.rs
│               ├── close.rs
│               ├── avoid.rs
│               ├── pin.rs
│               ├── chapter.rs
│               └── mark_watched.rs
└── flake.nix                     # NixOS packaging (final milestone)
```

---

## Phase 1: Foundation

### Milestone 1: Workspace Cargo.toml
**File**: `Cargo.toml`

Create workspace manifest defining the three crates.

**Contents**:
- Workspace members: `media-control`, `media-control-daemon`, `media-control-lib`
- Shared dependencies at workspace level
- Rust edition 2021

**Testing checkpoint**: `cargo check` succeeds with empty crates

---

### Milestone 2: Library Crate Manifest
**File**: `crates/media-control-lib/Cargo.toml`

Define shared library dependencies.

**Dependencies**:
- `tokio` (features: rt, net, io-util, sync, time)
- `serde` + `serde_json` (JSON parsing)
- `toml` (config parsing)
- `thiserror` (error types)
- `tracing` (logging)
- `regex` (pattern matching)

**Testing checkpoint**: `cargo build -p media-control-lib` succeeds

---

### Milestone 3: Library Root
**File**: `crates/media-control-lib/src/lib.rs`

Declare module structure and re-exports.

**Contents**:
- Module declarations
- Public re-exports of key types
- Crate-level error type definition

**Testing checkpoint**: Library compiles with module stubs

---

## Phase 2: Core Infrastructure

### Milestone 4: Error Types
**File**: `crates/media-control-lib/src/error.rs`

Define comprehensive error handling.

**Types**:
- `MediaControlError` enum covering: Hyprland IPC, Config, Jellyfin API, Window not found, IO errors
- `Result<T>` type alias

**Testing checkpoint**: Unit tests for error conversion traits

---

### Milestone 5: Configuration Module
**File**: `crates/media-control-lib/src/config.rs`

TOML configuration parsing from `~/.config/media-control/config.toml`.

**Structs**:
- `Config` - root config
- `Pattern` - window matching pattern (key, value, pinned_only, always_pin)
- `Positions` - corner coordinates (x_left, x_right, y_top, y_bottom, width, height)
- `Positioning` - avoidance settings (wide_window_threshold, workspace_switch_timeout, position_tolerance, overrides)
- `PositionOverride` - per-class position preferences

**Features**:
- Default config generation
- Config file watching (optional, for daemon)
- Validation

**Testing checkpoint**:
- Unit test: parse sample config
- Unit test: default values applied correctly
- Integration test: load from file path

---

### Milestone 6: Hyprland IPC Module
**File**: `crates/media-control-lib/src/hyprland.rs`

Direct Unix socket communication with Hyprland (replaces `hyprctl`).

**Structs**:
- `HyprlandClient` - async socket client
- `Client` - window/client data from Hyprland
- `Monitor` - monitor data
- `Workspace` - workspace data

**Methods**:
- `async fn command(&self, cmd: &str) -> Result<String>` - raw command
- `async fn dispatch(&self, action: &str) -> Result<()>` - dispatch commands
- `async fn batch(&self, commands: &[&str]) -> Result<()>` - batched commands
- `async fn get_clients(&self) -> Result<Vec<Client>>` - j/clients
- `async fn get_active_window(&self) -> Result<Option<Client>>` - j/activewindow
- `async fn get_monitors(&self) -> Result<Vec<Monitor>>` - j/monitors
- `async fn get_workspaces(&self) -> Result<Vec<Workspace>>` - j/workspaces

**Testing checkpoint**:
- Unit test: command formatting
- Integration test: connect to Hyprland socket (requires running Hyprland)
- Integration test: parse j/clients response

---

### Milestone 7: Window Types and Matching
**File**: `crates/media-control-lib/src/window.rs`

Window matching logic against config patterns.

**Structs**:
- `MediaWindow` - matched media window with metadata
- `WindowMatcher` - pattern matching engine

**Methods**:
- `fn matches(&self, client: &Client, patterns: &[Pattern]) -> Option<MatchResult>`
- `fn find_media_window(&self, clients: &[Client], focus: Option<&str>) -> Option<MediaWindow>`
- `fn find_media_windows(&self, clients: &[Client], monitor: i32) -> Vec<MediaWindow>`
- `fn find_previous_focus(&self, clients: &[Client], media_addr: &str, workspace: Option<&str>) -> Option<String>`

**Priority logic** (from bash):
1. Pinned window matching pattern
2. Focused window matching pattern
3. Any window matching pattern

**Testing checkpoint**:
- Unit test: pattern matching (class, title, regex)
- Unit test: priority ordering
- Unit test: previous focus selection

---

### Milestone 8: Jellyfin Client
**File**: `crates/media-control-lib/src/jellyfin.rs`

Jellyfin server API client.

**Structs**:
- `JellyfinClient` - API client with credentials
- `Session` - active session data
- `PlaybackInfo` - playback parameters
- `Credentials` - loaded from `~/.config/jellyfin-mpv-shim/cred.json`

**Methods**:
- `async fn load_credentials() -> Result<Credentials>`
- `async fn fetch_active_session(&self) -> Result<Option<Session>>`
- `async fn stop(&self, session_id: &str) -> Result<()>`
- `async fn mark_watched(&self, user_id: &str, item_id: &str) -> Result<()>`
- `async fn play_queue(&self, session_id: &str, item_ids: &[String]) -> Result<()>`
- `async fn get_remaining_queue(&self, session: &Session, current_id: &str) -> Vec<String>`

**Testing checkpoint**:
- Unit test: credential parsing
- Unit test: auth header construction
- Integration test: fetch session (requires Jellyfin server)

---

## Phase 3: Commands

### Milestone 9: Commands Module Root
**File**: `crates/media-control-lib/src/commands/mod.rs`

Command module structure and shared utilities.

**Contents**:
- Submodule declarations
- `CommandContext` struct (holds HyprlandClient, Config, JellyfinClient)
- Shared helper functions

**Testing checkpoint**: Module compiles

---

### Milestone 10: Fullscreen Command
**File**: `crates/media-control-lib/src/commands/fullscreen.rs`

Toggle fullscreen with focus restoration and pin state preservation.

**Function**: `async fn fullscreen(ctx: &CommandContext) -> Result<()>`

**Logic**:
1. Find media window
2. If not fullscreen and alwaysPin: pin instead of fullscreen
3. If exiting fullscreen: restore pin state, restore previous focus
4. If entering fullscreen: preserve pin state, dispatch fullscreen

**Testing checkpoint**:
- Unit test: state transition logic
- Integration test: fullscreen toggle (manual, requires Hyprland)

---

### Milestone 11: Move Command
**File**: `crates/media-control-lib/src/commands/move_window.rs`

Vim-style directional movement (h/j/k/l).

**Function**: `async fn move_window(ctx: &CommandContext, direction: Direction) -> Result<()>`

**Enum**: `Direction { Left, Down, Up, Right }`

**Logic**:
- h: move to x_left, keep y
- l: move to x_right, keep y
- k: keep x, move to y_top
- j: keep x, move to y_bottom

**Testing checkpoint**:
- Unit test: position calculation
- Integration test: window movement (manual)

---

### Milestone 12: Close Command
**File**: `crates/media-control-lib/src/commands/close.rs`

Graceful window closing with app-specific handling.

**Function**: `async fn close(ctx: &CommandContext) -> Result<()>`

**Logic**:
- mpv: call jellyfin stop, then playerctl stop
- Firefox PiP: return error (cannot close programmatically)
- Jellyfin Player: dispatch killwindow
- Default: dispatch killwindow

**Testing checkpoint**:
- Unit test: app detection logic
- Integration test: close behavior (manual)

---

### Milestone 13: Avoid Command
**File**: `crates/media-control-lib/src/commands/avoid.rs`

Smart window repositioning to avoid overlap.

**Function**: `async fn avoid(ctx: &CommandContext) -> Result<()>`

**Cases**:
1. Single-workspace: position media to preferred corner
2. Mouseover (focused media): move away, restore previous focus
3. Geometry overlap: move media out of focused window's way
4. Fullscreen app: move media windows aside

**Helper functions**:
- `fn rectangles_overlap(r1, r2) -> bool`
- `fn calculate_target_position(media_pos, focus_rect, positions) -> (i32, i32)`
- `fn get_preferred_position(focused_class, config) -> (i32, i32)`

**Testing checkpoint**:
- Unit test: overlap detection
- Unit test: position calculation (wide window threshold, screen center logic)
- Integration test: avoidance behavior (manual)

---

### Milestone 14: Pin and Float Command
**File**: `crates/media-control-lib/src/commands/pin.rs`

Toggle pinned floating mode with positioning.

**Function**: `async fn pin_and_float(ctx: &CommandContext) -> Result<()>`

**Logic**:
1. Find media window
2. If already pinned+floating: unpin and unfloat
3. Otherwise: enable floating, enable pin, position to configured corner

**Testing checkpoint**:
- Unit test: state toggle logic
- Integration test: pin toggle (manual)

---

### Milestone 15: Chapter Command
**File**: `crates/media-control-lib/src/commands/chapter.rs`

mpv chapter navigation via IPC socket.

**Function**: `async fn chapter(ctx: &CommandContext, direction: ChapterDirection) -> Result<()>`

**Enum**: `ChapterDirection { Next, Prev }`

**Logic**:
- Connect to mpv IPC socket (`/tmp/mpvctl-jshim` or configured)
- Send JSON-IPC command: `{"command":["add","chapter",1]}` or `-1`

**Testing checkpoint**:
- Unit test: IPC command formatting
- Integration test: chapter skip (requires mpv with IPC)

---

### Milestone 16: Mark Watched Commands
**File**: `crates/media-control-lib/src/commands/mark_watched.rs`

Jellyfin mark-watched variants.

**Functions**:
- `async fn mark_watched(ctx: &CommandContext) -> Result<()>`
- `async fn mark_watched_and_stop(ctx: &CommandContext) -> Result<()>`
- `async fn mark_watched_and_next(ctx: &CommandContext) -> Result<()>`

**Logic**:
- Verify media window is mpv
- Call Jellyfin API via JellyfinClient
- For "and_stop": also stop playback
- For "and_next": get remaining queue, play next items

**Testing checkpoint**:
- Integration test: mark watched (requires Jellyfin)

---

## Phase 4: CLI Binary

### Milestone 17: CLI Binary Manifest
**File**: `crates/media-control/Cargo.toml`

CLI binary dependencies.

**Dependencies**:
- `media-control-lib` (path)
- `clap` (derive feature) - CLI parsing
- `clap_complete` - shell completion generation
- `tokio` (rt-multi-thread, macros)
- `tracing-subscriber` - logging setup

**Testing checkpoint**: `cargo build -p media-control` succeeds

---

### Milestone 17b: CLI Build Script
**File**: `crates/media-control/build.rs`

Generate shell completions at build time.

**Features**:
- Generate completions for bash, zsh, fish
- Output to `$OUT_DIR/completions/`
- Nix flake will install these to appropriate locations

**Testing checkpoint**: Completions generated in target directory

---

### Milestone 18: CLI Main
**File**: `crates/media-control/src/main.rs`

CLI entry point with subcommand routing.

**Subcommands** (via clap):
- `fullscreen` - toggle fullscreen
- `move <direction>` - h/j/k/l movement
- `close` - close media window
- `avoid` - trigger avoidance (usually called by daemon)
- `pin-and-float` - toggle pin+float mode
- `mark-watched` - mark current item watched
- `mark-watched-and-stop` - mark watched, stop playback
- `mark-watched-and-next` - mark watched, advance queue
- `chapter <next|prev>` - chapter navigation

**Global flags**:
- `-v, --verbose` - Enable debug logging (silent by default)
- `-c, --config <path>` - Override config file path

**Setup**:
- Initialize tracing (off by default, enabled with -v)
- Load config
- Create CommandContext
- Route to appropriate command

**Testing checkpoint**:
- `media-control --help` shows all commands
- `media-control fullscreen` executes silently (integration)
- `media-control -v fullscreen` shows debug output

---

## Phase 5: Daemon Binary

### Milestone 19: Daemon Binary Manifest
**File**: `crates/media-control-daemon/Cargo.toml`

Daemon binary dependencies.

**Dependencies**:
- `media-control-lib` (path)
- `tokio` (full features)
- `tracing-subscriber`

**Testing checkpoint**: `cargo build -p media-control-daemon` succeeds

---

### Milestone 20: Daemon Main
**File**: `crates/media-control-daemon/src/main.rs`

Event-driven daemon listening to Hyprland socket2.

**Dependencies** (additional):
- `libsystemd` - systemd socket activation support

**Features**:
- PID file management (`$XDG_RUNTIME_DIR/media-control-daemon.pid`)
- Hyprland socket2 event stream (unbuffered)
- Debouncing (15ms window via tokio::time)
- Suppression file check (`$XDG_RUNTIME_DIR/media-avoider-suppress`)
- Workspace tracking for single-workspace detection
- **Systemd socket activation**: detect `LISTEN_FDS` env, use passed socket if available
- Graceful shutdown on SIGTERM/SIGINT

**Subcommands**:
- `start` - start daemon (fork to background)
- `stop` - stop running daemon
- `status` - check if running
- `foreground` - run in foreground (for systemd/debugging)

**Event handling** (from socket2):
- `workspace` - workspace change, trigger avoid
- `activewindow` - focus change, trigger avoid (with debounce)
- `movewindow` - window moved, trigger avoid
- `openwindow`/`closewindow` - window count changed
- Ignore: `windowtitle`, `urgent`, `minimize`, layer events

**Testing checkpoint**:
- `media-control-daemon start` launches daemon
- `media-control-daemon status` reports running
- `media-control-daemon stop` terminates cleanly
- Event handling triggers avoid command
- Systemd activation works (test with `systemd-socket-activate`)

---

### Milestone 21: Systemd Service Files
**File**: `systemd/media-control-daemon.service`

Systemd user service and socket unit files.

**Files to create**:
- `systemd/media-control-daemon.service` - service unit
- `systemd/media-control-daemon.socket` - socket activation unit

**Service unit features**:
- Type=notify (if using sd_notify) or Type=simple
- ExecStart pointing to daemon binary with `foreground`
- Restart=on-failure
- WantedBy=hyprland-session.target (or graphical-session.target)
- Environment for XDG_RUNTIME_DIR and HYPRLAND_INSTANCE_SIGNATURE

**Socket unit features**:
- ListenStream pointing to a notification socket (not Hyprland's socket)
- BindIPv6Only=both (if needed)

**Note**: Socket activation here is for on-demand daemon startup, not for replacing Hyprland's socket2.

**Testing checkpoint**:
- `systemctl --user start media-control-daemon` works
- `systemctl --user status media-control-daemon` shows running
- Service restarts on failure

---

## Phase 6: Packaging

### Milestone 22: Nix Flake
**File**: `flake.nix`

NixOS flake for building and installing.

**Outputs**:
- `packages.default` - both binaries
- `packages.media-control` - CLI only
- `packages.media-control-daemon` - daemon only
- `devShells.default` - development shell with rust-analyzer, cargo, etc.

**Features**:
- Use `crane` or `naersk` for Rust builds
- Include runtime dependencies (none expected beyond libc)
- Install shell completions to `$out/share/bash-completion/`, `$out/share/zsh/`, `$out/share/fish/`
- Install systemd units to `$out/lib/systemd/user/`
- Provide NixOS module option for easy integration

**Testing checkpoint**:
- `nix build` produces binaries
- `nix develop` provides working dev environment
- Binaries run correctly from Nix store
- Shell completions work after sourcing

---

## Testing Strategy

### Unit Tests
Each module should have inline `#[cfg(test)]` modules testing:
- Pure logic functions
- Parsing/serialization
- Error handling

Run: `cargo test -p media-control-lib`

### Integration Tests
Create `crates/media-control-lib/tests/` for tests requiring:
- File system access (config loading)
- Mock servers (Jellyfin API)

Run: `cargo test -p media-control-lib --test '*'`

### Manual Testing Checklist
After each command milestone:
1. Build: `cargo build`
2. Test in Hyprland session with media window open
3. Verify behavior matches bash script

### End-to-End Testing
After Phase 5:
1. Start daemon
2. Open mpv/PiP window
3. Test all commands
4. Verify avoidance triggers on focus change
5. Compare performance to bash version (target: <5ms response)

---

## Parallel Development Strategy

Milestones that can be developed in parallel (after dependencies complete):

**After Milestone 3 (lib.rs)**:
- Milestone 4 (error.rs)
- Milestone 5 (config.rs)
- Milestone 6 (hyprland.rs)
- Milestone 8 (jellyfin.rs)

**After Milestones 4-7 complete**:
- Milestone 9 (commands/mod.rs)
- All command milestones (10-16) can be developed in parallel once mod.rs is done

**After Milestone 9 (commands/mod.rs)**:
- Milestones 17, 17b, 18 (CLI binary) - parallel with commands
- Milestones 19, 20 (daemon) - parallel with commands

**After Milestone 20 (daemon main)**:
- Milestone 21 (systemd units)

**After all code milestones**:
- Milestone 22 (flake.nix)

### Dependency Graph

```
1 (Cargo.toml)
├── 2 (lib Cargo.toml)
│   └── 3 (lib.rs)
│       ├── 4 (error.rs)
│       ├── 5 (config.rs) ──┐
│       ├── 6 (hyprland.rs) ├──> 7 (window.rs)
│       └── 8 (jellyfin.rs) │
│                           │
│       ┌───────────────────┘
│       v
│       9 (commands/mod.rs)
│       ├── 10 (fullscreen.rs)
│       ├── 11 (move_window.rs)
│       ├── 12 (close.rs)
│       ├── 13 (avoid.rs)
│       ├── 14 (pin.rs)
│       ├── 15 (chapter.rs)
│       └── 16 (mark_watched.rs)
│
├── 17 (CLI Cargo.toml)
│   ├── 17b (build.rs)
│   └── 18 (CLI main.rs)
│
└── 19 (daemon Cargo.toml)
    └── 20 (daemon main.rs)
        └── 21 (systemd units)

22 (flake.nix) ← depends on all above
```

---

## Migration Path

1. Keep bash scripts functional during development
2. Test Rust version alongside bash version
3. Once feature-complete:
   - Update keybindings to use Rust binary
   - Run daemon via systemd user service
   - Deprecate bash scripts

## Performance Targets

- Command execution: <10ms (bash: ~50-100ms)
- Daemon event response: <5ms (bash: ~15-30ms)
- Memory footprint: <5MB resident (daemon)
