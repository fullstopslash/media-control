//! Jellyfin integration for marking items as watched.
//!
//! Provides commands to mark the current item as watched, optionally
//! stopping playback or advancing to the next item via the shim.

use std::path::Path;

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use super::{get_media_window, CommandContext};
use crate::error::{MediaControlError, Result};
use crate::jellyfin::{JellyfinClient, JellyfinError};

/// Default mpv IPC socket path (jellyfin-mpv-shim).
const MPV_IPC_SOCKET_DEFAULT: &str = "/tmp/mpvctl-jshim";

/// Fallback mpv IPC socket path.
const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl0";

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

/// Mark current item as watched and advance to next episode.
///
/// Delegates to the jellyfin-mpv-shim fork by sending a `ctrl+n` keypress
/// via mpv's IPC socket. The shim handles strategy resolution, Jellyfin API
/// calls, and playback advancement natively.
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

    send_mpv_keypress("ctrl+n").await
}

/// Send a keypress command to mpv via IPC socket.
///
/// Tries multiple socket paths in order:
/// 1. `$MPV_IPC_SOCKET` environment variable (if set)
/// 2. `/tmp/mpvctl-jshim` (jellyfin-mpv-shim default)
/// 3. `/tmp/mpvctl0` (common fallback)
async fn send_mpv_keypress(key: &str) -> Result<()> {
    let payload = format!(r#"{{"command":["keypress","{key}"]}}"#);

    let env_socket = std::env::var("MPV_IPC_SOCKET").ok();
    let sockets = [
        env_socket.as_deref(),
        Some(MPV_IPC_SOCKET_DEFAULT),
        Some(MPV_IPC_SOCKET_FALLBACK),
    ];

    for socket_path in sockets.into_iter().flatten() {
        let path = Path::new(socket_path);
        if !path.exists() {
            continue;
        }

        match UnixStream::connect(path).await {
            Ok(mut stream) => {
                stream.write_all(payload.as_bytes()).await?;
                stream.write_all(b"\n").await?;
                return Ok(());
            }
            Err(_) => continue,
        }
    }

    Err(MediaControlError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no mpv IPC socket found",
    )))
}
