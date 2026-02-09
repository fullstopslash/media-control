//! Jellyfin integration for marking items as watched.
//!
//! Provides commands to mark the current item as watched, optionally
//! stopping playback or advancing to the next item in the queue.

use super::{get_media_window, CommandContext};
use crate::error::{MediaControlError, Result};
use crate::jellyfin::{JellyfinClient, JellyfinError};

/// Convert a Jellyfin error to a MediaControlError.
fn convert_jellyfin_error(e: JellyfinError) -> MediaControlError {
    match e {
        JellyfinError::CredentialsNotFound(_) | JellyfinError::InvalidCredentials(_) => {
            MediaControlError::jellyfin_credentials()
        }
        JellyfinError::NoMpvSession => MediaControlError::jellyfin_session_not_found(),
        JellyfinError::NoPlayingItem => MediaControlError::jellyfin_session_not_found(),
        JellyfinError::Http(e) => MediaControlError::jellyfin_api(e),
        JellyfinError::CredentialsParsing(e) => MediaControlError::jellyfin_api(e),
        JellyfinError::HostnameError => MediaControlError::jellyfin_api("hostname lookup failed"),
        JellyfinError::Io(e) => MediaControlError::Io(e),
    }
}

/// Mark the current Jellyfin session item as watched.
///
/// This command finds the active mpv media window, loads Jellyfin credentials,
/// and marks the currently playing item as watched on the Jellyfin server.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
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

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;
    jellyfin
        .mark_current_watched()
        .await
        .map_err(convert_jellyfin_error)?;

    Ok(())
}

/// Mark current item as watched and stop playback.
///
/// Marks the current Jellyfin item as watched and stops both the Jellyfin
/// session and local mpv playback via playerctl.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
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

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;
    jellyfin
        .mark_watched_and_stop()
        .await
        .map_err(convert_jellyfin_error)?;

    // Also try playerctl stop (best effort, ignore errors)
    let _ = tokio::process::Command::new("playerctl")
        .args(["--player=mpv", "stop"])
        .output()
        .await;

    Ok(())
}

/// Mark current item as watched and advance to next in queue.
///
/// Marks the current Jellyfin item as watched and advances playback to
/// the next item in the queue.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
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

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;
    jellyfin
        .mark_watched_and_next()
        .await
        .map_err(convert_jellyfin_error)?;

    Ok(())
}
