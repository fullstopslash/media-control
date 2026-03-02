# Media Control Rust Rewrite - Progress Tracker

## All Phases Complete!

### Phase 1: Foundation
- [x] Milestone 1: Workspace Cargo.toml
- [x] Milestone 2: Library Crate Manifest
- [x] Milestone 3: Library Root (lib.rs with module stubs)

### Phase 2: Core Infrastructure
- [x] Milestone 4: Error Types (error.rs)
- [x] Milestone 5: Configuration Module (config.rs)
- [x] Milestone 6: Hyprland IPC Module (hyprland.rs)
- [x] Milestone 7: Window Types and Matching (window.rs)
- [x] Milestone 8: Jellyfin Client (jellyfin.rs)

### Phase 3: Commands
- [x] Milestone 9: Commands Module Root (commands/mod.rs)
- [x] Milestone 10: Fullscreen Command (commands/fullscreen.rs)
- [x] Milestone 11: Move Command (commands/move_window.rs)
- [x] Milestone 12: Close Command (commands/close.rs)
- [x] Milestone 13: Avoid Command (commands/avoid.rs)
- [x] Milestone 14: Pin and Float Command (commands/pin.rs)
- [x] Milestone 15: Chapter Command (commands/chapter.rs)
- [x] Milestone 16: Mark Watched Commands (commands/mark_watched.rs)

### Phase 4: CLI Binary
- [x] Milestone 17: CLI Binary Manifest (Cargo.toml)
- [x] Milestone 17b: Shell Completions (runtime generation via `completions` subcommand)
- [x] Milestone 18: CLI Main (main.rs with clap subcommands)

### Phase 5: Daemon Binary
- [x] Milestone 19: Daemon Binary Manifest (Cargo.toml with nix crate)
- [x] Milestone 20: Daemon Main (event loop, PID management, start/stop/status)
- [x] Milestone 21: Systemd Service Files (service unit + install script)

### Phase 6: Packaging
- [x] Milestone 22: Nix Flake (with home-manager module)

---

## Test Results
- **100+ tests passing** across all modules
- All crates build successfully
- CLI and daemon binaries functional

## CLI Usage
```bash
# Media control commands
media-control fullscreen           # Toggle fullscreen
media-control move left|right|up|down  # Move to screen edge
media-control move h|j|k|l         # Vim-style (h=left, j=down, k=up, l=right)
media-control close                # Close media window
media-control focus                # Focus media window
media-control focus --launch "cmd" # Focus or launch if not found
media-control avoid                # Trigger avoidance
media-control pin-and-float        # Toggle pin+float mode
media-control mark-watched         # Mark Jellyfin item watched
media-control mark-watched-and-stop
media-control mark-watched-and-next
media-control chapter next|prev    # mpv chapter navigation
media-control completions bash|zsh|fish  # Generate shell completions

# Daemon commands
media-control-daemon start         # Start daemon in background
media-control-daemon stop          # Stop running daemon
media-control-daemon status        # Check daemon status
media-control-daemon foreground    # Run in foreground (for systemd)
```

## Installation

### Cargo (manual)
```bash
cargo install --path crates/media-control
cargo install --path crates/media-control-daemon
```

### Nix
```bash
nix build
nix profile install .#default
```

### Home-manager
```nix
{
  inputs.media-control.url = "github:rain/media-control";

  # In home configuration:
  imports = [ inputs.media-control.homeManagerModules.default ];
  services.media-control.enable = true;
}
```

### Systemd (manual)
```bash
cd systemd && ./install.sh
systemctl --user enable --now media-control-daemon.service
```

## Configuration
Create `~/.config/media-control/config.toml`:
```toml
# Window matching patterns
[[patterns]]
key = "class"
value = "mpv"
always_pin = true

[[patterns]]
key = "title"
value = "Picture-in-Picture"
always_pin = true

[[patterns]]
key = "class"
value = "com.github.iwalton3.jellyfin-media-player"
pinned_only = true

# Corner coordinates
[positions]
x_left = 8
x_right = 1272
y_top = 36
y_bottom = 712
width = 640
height = 360

# Avoidance behavior
[positioning]
wide_window_threshold = 90
workspace_switch_timeout = 2
position_tolerance = 5
default_x = "x_right"
default_y = "y_bottom"

# Per-class overrides
[[positioning.overrides]]
focused_class = "cursor"
pref_x = "x_left"
pref_y = "y_bottom"
```

## Build Verification
- **111 tests passing** (94 unit + 3 integration + 14 doc tests)
- Release binaries: `media-control` (9.8 MB), `media-control-daemon` (5.5 MB)
- Config file created and verified at `~/.config/media-control/config.toml`
