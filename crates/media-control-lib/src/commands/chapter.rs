//! mpv chapter navigation.
//!
//! Provides chapter navigation commands (next/prev) for mpv playback
//! using direct IPC socket communication.

use std::path::Path;

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use super::{get_media_window, CommandContext};
use crate::error::{MediaControlError, Result};

/// Default mpv IPC socket path (jellyfin-mpv-shim).
const MPV_IPC_SOCKET_DEFAULT: &str = "/tmp/mpvctl-jshim";

/// Fallback mpv IPC socket path.
const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl0";

/// Direction for chapter navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterDirection {
    /// Navigate to the next chapter.
    Next,
    /// Navigate to the previous chapter.
    Prev,
}

impl ChapterDirection {
    /// Get the chapter offset value for mpv IPC.
    const fn offset(self) -> i8 {
        match self {
            Self::Next => 1,
            Self::Prev => -1,
        }
    }
}

/// Navigate to the next or previous chapter in mpv.
///
/// This command only operates on mpv windows. If no media window is found,
/// or if the media window is not mpv, the command silently succeeds.
///
/// # Arguments
///
/// * `ctx` - Command context with Hyprland client and configuration
/// * `direction` - Whether to navigate to the next or previous chapter
///
/// # Errors
///
/// Returns an error if:
/// - Hyprland IPC fails when fetching windows
/// - No mpv IPC socket is available
/// - Socket communication fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, chapter::{chapter, ChapterDirection}};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// chapter(&ctx, ChapterDirection::Next).await?;
/// # Ok(())
/// # }
/// ```
pub async fn chapter(ctx: &CommandContext, direction: ChapterDirection) -> Result<()> {
    // Get media window, silently succeed if none found
    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    // Only operate on mpv windows
    if window.class != "mpv" {
        return Ok(());
    }

    // Build the mpv IPC command
    let payload = format!(r#"{{"command":["add","chapter",{}]}}"#, direction.offset());

    send_mpv_command(&payload).await
}

/// Send a command to mpv via IPC socket.
///
/// Tries multiple socket paths in order:
/// 1. `$MPV_IPC_SOCKET` environment variable (if set)
/// 2. `/tmp/mpvctl-jshim` (jellyfin-mpv-shim default)
/// 3. `/tmp/mpvctl0` (common fallback)
async fn send_mpv_command(payload: &str) -> Result<()> {
    // Build list of sockets to try
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

    // No working socket found
    Err(MediaControlError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no mpv IPC socket found (tried $MPV_IPC_SOCKET, /tmp/mpvctl-jshim, /tmp/mpvctl0)",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_direction_offset() {
        assert_eq!(ChapterDirection::Next.offset(), 1);
        assert_eq!(ChapterDirection::Prev.offset(), -1);
    }

    #[test]
    fn mpv_command_format() {
        let next_cmd = format!(
            r#"{{"command":["add","chapter",{}]}}"#,
            ChapterDirection::Next.offset()
        );
        assert_eq!(next_cmd, r#"{"command":["add","chapter",1]}"#);

        let prev_cmd = format!(
            r#"{{"command":["add","chapter",{}]}}"#,
            ChapterDirection::Prev.offset()
        );
        assert_eq!(prev_cmd, r#"{"command":["add","chapter",-1]}"#);
    }
}
