//! Tag the currently-playing item as "keep" to prevent auto-deletion.
//!
//! Sends `script-message keep` to the shim via the standard IPC path.
//! The Rust shim routes the command to the active store plugin internally.

use super::{CommandContext, require_mpv_window, send_mpv_script_message};
use crate::error::Result;

/// Tag the current item as "keep".
pub async fn keep(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("keep").await
}

/// Toggle favorite on the current item.
pub async fn favorite(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("favorite").await
}

/// Delete the current item.
pub async fn delete(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("delete").await
}

/// Increment o-counter on the current item.
pub async fn add_o(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    send_mpv_script_message("add-o").await
}
