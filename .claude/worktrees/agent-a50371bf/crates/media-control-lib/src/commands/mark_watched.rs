//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.
//!
//! No Hyprland window check — mpv socket failure is sufficient guard.
//! Skipping the Hyprland IPC saves ~15-30ms per command.

use super::send_mpv_script_message;
use crate::error::Result;

/// Mark the current item as watched.
pub async fn mark_watched() -> Result<()> {
    send_mpv_script_message("mark-watched").await
}

/// Mark current item as watched and stop playback.
pub async fn mark_watched_and_stop() -> Result<()> {
    send_mpv_script_message("mark-watched").await?;
    send_mpv_script_message("stop-and-clear").await
}

/// Mark current item as watched and advance to next episode.
pub async fn mark_watched_and_next() -> Result<()> {
    send_mpv_script_message("mark-watched-next").await
}

/// Next item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-next" to avoid collision with uosc's "next" binding.
pub async fn next() -> Result<()> {
    send_mpv_script_message("shim-next").await
}

/// Previous item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-prev" to avoid collision with uosc's "prev" binding.
pub async fn prev() -> Result<()> {
    send_mpv_script_message("shim-prev").await
}

/// Jump to next series/collection (series-level navigation).
pub async fn next_series() -> Result<()> {
    send_mpv_script_message("next-series").await
}

/// Return to previous series/collection (series-level navigation).
pub async fn prev_series() -> Result<()> {
    send_mpv_script_message("prev-series").await
}
