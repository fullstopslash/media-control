//! Tag the currently-playing item as "keep" to prevent auto-deletion.
//!
//! Sends `script-message keep` to mpv IPC. Both jellyfin-mpv-shim and
//! stash-integration.lua listen for this message — the correct handler
//! acts based on playback context.

use super::{get_media_window, send_mpv_script_message, CommandContext};
use crate::error::Result;

/// Tag the current item as "keep".
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, keep};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// keep::keep(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn keep(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    send_mpv_script_message("keep").await
}
