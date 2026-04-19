# Tech Stack

## Overview
Rust-based CLI tool and daemon for managing media windows in Hyprland (Wayland compositor). Chosen for performance-critical IPC operations, strong typing, and zero-cost async abstractions.

## Languages
Rust (2024 edition)

Rust provides the safety guarantees and performance needed for a desktop utility that communicates via Unix sockets with sub-millisecond latency requirements. The 2024 edition enables modern language features like let chains.

## Framework
No application framework. Custom CLI built with:
- **clap** (v4) - CLI argument parsing with derive macros and shell completions
- **tokio** (v1) - Async runtime for concurrent socket I/O and event handling
- **reqwest** (v0.12) - Async HTTP client for Jellyfin API integration
- **serde** / **serde_json** / **toml** - Serialization for config (TOML) and IPC (JSON)

The project is structured as a Cargo workspace with three crates:
- `media-control` - CLI binary
- `media-control-daemon` - Event-driven daemon binary
- `media-control-lib` - Shared library (Hyprland IPC, config, window matching, commands)

## Authentication
N/A for the tool itself. Jellyfin API credentials are read from `~/.config/jellyfin-mpv-shim/cred.json` (shared with jellyfin-mpv-shim). Uses MediaBrowser token-based auth headers.

## Infrastructure & Deployment
Local Linux desktop application targeting Hyprland/Wayland.
- **Build**: Nix flake for reproducible builds and dev environment
- **Runtime**: systemd user service for the daemon (`media-control-daemon.service`)
- **Distribution**: Direct binary, no packaging beyond Nix

## Package Manager
Cargo (Rust's built-in package manager). Workspace-level dependency management with shared versions across crates.

## Key Dependencies
| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime (sockets, timers, signals, fs) |
| `clap` | CLI parsing with derive |
| `serde` / `serde_json` / `toml` | Serialization |
| `reqwest` | HTTP client (Jellyfin API, rustls-tls) |
| `regex` | Window pattern matching |
| `tracing` / `tracing-subscriber` | Structured logging |
| `thiserror` | Error type derivation |
| `nix` | POSIX APIs (signals, FIFO creation) |
| `gethostname` | Hostname for Jellyfin auth headers |

## Decision Relationships
- Rust + tokio chosen over the original bash+socat implementation for reliability and performance (66% faster IPC)
- reqwest with rustls-tls avoids OpenSSL dependency for simpler builds
- Workspace structure separates CLI, daemon, and library concerns cleanly
