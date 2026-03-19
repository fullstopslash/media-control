//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.

use super::{get_media_window, send_mpv_script_message, CommandContext};
use crate::error::Result;

/// Mark the current item as watched.
///
/// Delegates to the shim via IPC `mark-watched` script-message.
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    send_mpv_script_message("mark-watched").await
}

/// Mark current item as watched and stop playback.
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched_and_stop(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched_and_stop(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    let _ = send_mpv_script_message("mark-watched").await;
    send_mpv_script_message("stop-and-clear").await
}

/// Mark current item as watched and advance to next episode.
///
/// Delegates to the shim via IPC `mark-watched-next` script-message.
/// The shim handles per-library strategy resolution natively.
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched_and_next(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched_and_next(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    send_mpv_script_message("mark-watched-next").await
}

/// Skip to next item via per-library strategy (no mark watched).
pub async fn skip_next(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    send_mpv_script_message("skip-next").await
}

/// Skip to previous item (no mark watched).
pub async fn skip_prev(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    send_mpv_script_message("skip-prev").await
}
