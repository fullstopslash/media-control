//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.

use super::{CommandContext, require_mpv_window, send_mpv_script_message};
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

/// Next item via per-library strategy (episode-level, no mark watched).
pub async fn next(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("next").await
}

/// Previous item via per-library strategy (episode-level, no mark watched).
pub async fn prev(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("prev").await
}

/// Jump to next series/collection (series-level navigation).
pub async fn next_series(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("next-series").await
}

/// Return to previous series/collection (series-level navigation).
pub async fn prev_series(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("prev-series").await
}
