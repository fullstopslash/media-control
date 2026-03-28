//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.

use super::{require_mpv_window, send_mpv_script_message, CommandContext};
use crate::error::Result;

/// Mark the current item as watched.
pub async fn mark_watched(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("mark-watched").await
}

/// Mark current item as watched and stop playback.
pub async fn mark_watched_and_stop(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("mark-watched").await?;
    send_mpv_script_message("stop-and-clear").await
}

/// Mark current item as watched and advance to next episode.
pub async fn mark_watched_and_next(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("mark-watched-next").await
}

/// Skip to next item via per-library strategy (no mark watched).
pub async fn skip_next(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("skip-next").await
}

/// Skip to previous item (no mark watched).
pub async fn skip_prev(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("skip-prev").await
}
