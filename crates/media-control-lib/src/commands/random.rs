//! Random subcommand — trigger random playback via mpv-shim IPC.
//!
//! Sends a `script-message random [type]` to the shim, which delegates
//! to the active store's `random_item()` implementation.
//!
//! Types are store-specific:
//! - Jellyfin: `show`, `series`, `movie`
//! - Twitch: (any or none — picks random live channel)
//! - Stash: `scene`, `performer`, `studio`

use super::{send_mpv_script_message, send_mpv_script_message_with_args};

/// Trigger random playback via mpv-shim IPC.
///
/// If `random_type` is provided, it's passed as an argument to the
/// `random` script-message. The active store interprets the type.
pub async fn random(
    random_type: Option<&str>,
) -> crate::error::Result<()> {
    match random_type {
        Some(t) => {
            send_mpv_script_message_with_args("random", &[t]).await?;
        }
        None => {
            send_mpv_script_message("random").await?;
        }
    }

    Ok(())
}
