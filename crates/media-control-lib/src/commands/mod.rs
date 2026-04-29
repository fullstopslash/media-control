//! Command implementations for media window control.
//!
//! This module groups commands into three subnamespaces:
//!
//! - [`window`] — Hyprland-window-management commands (avoider-relevant)
//! - [`workflow`] — mpv/Jellyfin workflow commands (CLI-only)
//! - [`shared`] — items used by both groups
//!
//! Top-level re-exports preserve the legacy
//! `media_control_lib::commands::<name>` paths used by the binaries and
//! external doc-tests.
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::commands::CommandContext;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = CommandContext::new().await?;
//!
//! // Find the current media window
//! if let Some(window) = media_control_lib::commands::get_media_window(&ctx).await? {
//!     println!("Found media window: {} ({})", window.title, window.address);
//! }
//! # Ok(())
//! # }
//! ```

pub mod shared;
pub mod window;
#[cfg(feature = "cli")]
pub mod workflow;

// Per-command back-compat re-exports. Preserve the legacy
// `media_control_lib::commands::<name>::<func>` paths the binaries and
// the per-file doc-tests use.
pub use window::{avoid, close, focus, fullscreen, minify, move_window, pin};
#[cfg(feature = "cli")]
pub use workflow::{chapter, keep, mark_watched, play, random, seek, status};

// Item-level back-compat re-exports for things the binaries reach for
// directly (`commands::CommandContext`, `commands::runtime_dir`, etc.) and
// for the top-of-file doc-test that calls `commands::get_media_window`.
pub use shared::{CommandContext, runtime_dir};
pub use window::{
    clear_suppression, effective_dimensions, get_media_window, get_media_window_with_clients,
    get_minify_state_path, get_suppress_file_path, is_minified, resolve_effective_position,
    restore_focus, suppress_avoider, toggle_minified,
};
#[cfg(feature = "cli")]
pub use workflow::{
    query_mpv_property, send_mpv_ipc_command, send_mpv_script_message,
    send_mpv_script_message_with_args,
};
