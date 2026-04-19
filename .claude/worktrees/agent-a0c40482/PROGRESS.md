# Media Control Rust Rewrite - Progress Tracker

## All Phases Complete!

### Phase 1: Foundation
- [x] Milestone 1: Workspace Cargo.toml <!-- tw:e5c72892-6260-4f2b-a0e3-757b62869fd7 -->
- [x] Milestone 2: Library Crate Manifest <!-- tw:d8bb2390-371d-4526-a386-c41281f6d611 -->
- [x] Milestone 3: Library Root (lib.rs with module stubs) <!-- tw:b04cd473-fe82-4517-a113-9764d0407548 -->

### Phase 2: Core Infrastructure
- [x] Milestone 4: Error Types (error.rs) <!-- tw:146cd772-e6ea-4be0-8471-ecf80cc6af46 -->
- [x] Milestone 5: Configuration Module (config.rs) <!-- tw:659a6e66-15fd-4254-b544-3410d54a1ffa -->
- [x] Milestone 6: Hyprland IPC Module (hyprland.rs) <!-- tw:4a9facf4-69a6-4a34-b0d8-bd97b59d87fa -->
- [x] Milestone 7: Window Types and Matching (window.rs) <!-- tw:3118a81b-0f4c-4196-ba26-e16d9ed17d5f -->
- [x] Milestone 8: Jellyfin Client (jellyfin.rs) <!-- tw:ab2b79e8-346c-49c6-996e-1c4c0259bb27 -->

### Phase 3: Commands
- [x] Milestone 9: Commands Module Root (commands/mod.rs) <!-- tw:8b31b1b7-6017-43ac-8df0-943400c45d60 -->
- [x] Milestone 10: Fullscreen Command (commands/fullscreen.rs) <!-- tw:bd88a717-8f05-46f5-806b-74b80704d144 -->
- [x] Milestone 11: Move Command (commands/move_window.rs) <!-- tw:eba5f3cd-76fe-4999-99fe-993b9c3cb420 -->
- [x] Milestone 12: Close Command (commands/close.rs) <!-- tw:cb1a27f4-65ad-4c78-bddb-fc05cc2984e8 -->
- [x] Milestone 13: Avoid Command (commands/avoid.rs) <!-- tw:a033710c-be3d-4440-9c37-38ee27711748 -->
- [x] Milestone 14: Pin and Float Command (commands/pin.rs) <!-- tw:c1187cc6-d3f3-44c9-b23a-b5116dbf4fb5 -->
- [x] Milestone 15: Chapter Command (commands/chapter.rs) <!-- tw:87e30020-c4cf-4f00-9136-10c52c128613 -->
- [x] Milestone 16: Mark Watched Commands (commands/mark_watched.rs) <!-- tw:7bf530e7-527d-4f97-b516-3b7fb5f319cb -->

### Phase 4: CLI Binary
- [x] Milestone 17: CLI Binary Manifest (Cargo.toml) <!-- tw:af7f5ac6-ef7f-4055-8da7-42c1a31691b9 -->
- [x] Milestone 17b: Shell Completions (runtime generation via `completions` subcommand) <!-- tw:9e5b4c00-4dff-46bd-91da-01c85cb6d0d7 -->
- [x] Milestone 18: CLI Main (main.rs with clap subcommands) <!-- tw:a41d21fa-8c99-4d5a-83d6-df2177afb676 -->

### Phase 5: Daemon Binary
- [x] Milestone 19: Daemon Binary Manifest (Cargo.toml with nix crate) <!-- tw:7e092f6d-6ca1-46ca-9073-0d1d41fb47f4 -->
- [x] Milestone 20: Daemon Main (event loop, PID management, start/stop/status) <!-- tw:a4fa9692-ef53-4b24-bcc0-a460275e3c46 -->
- [x] Milestone 21: Systemd Service Files (service unit + install script) <!-- tw:ecfcd09a-85a2-4656-85a2-57a6d85b0473 -->

### Phase 6: Packaging
- [x] Milestone 22: Nix Flake (with home-manager module) <!-- tw:e5bda067-485e-4866-a573-fbc47f14352e -->

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
