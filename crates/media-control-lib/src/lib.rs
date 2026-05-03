//! Media Control library for Hyprland
//!
//! Provides shared functionality for managing media windows (mpv, Picture-in-Picture, Jellyfin)
//! with automatic avoidance, positioning, and Jellyfin server integration.

pub mod commands;
pub mod config;
pub mod error;
pub mod hyprland;
#[cfg(feature = "cli")]
pub mod jellyfin;
pub mod transport;
pub mod window;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;
