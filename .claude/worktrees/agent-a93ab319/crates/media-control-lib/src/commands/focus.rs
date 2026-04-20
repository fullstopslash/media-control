//! Focus or launch media window.
//!
//! Focuses the media window if it exists, otherwise launches a command.
//! This is useful for keybindings that should either focus an existing media
//! player or start one if none exists.
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::commands::{CommandContext, focus::focus_or_launch};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = CommandContext::new()?;
//!
//! // Focus media window or launch Jellyfin
//! focus_or_launch(&ctx, Some("flatpak run com.github.iwalton3.jellyfin-media-player")).await?;
//!
//! // Just focus (no launch fallback)
//! focus_or_launch(&ctx, None).await?;
//! # Ok(())
//! # }
//! ```

use std::process::Stdio;

use tokio::process::Command;

use super::{
    CommandContext, clear_suppression, get_media_window, suppress_avoider,
};
use crate::error::Result;

/// Focus the media window, or launch a command if no media window exists.
///
/// This command:
/// 1. Searches for a media window matching the configured patterns
/// 2. If found, focuses it via Hyprland IPC
/// 3. If not found and a launch command is provided, spawns that command
/// 4. Suppresses the avoider to prevent repositioning during focus
///
/// # Arguments
///
/// * `ctx` - The command context
/// * `launch_cmd` - Optional command to run if no media window is found.
///   The command is split using POSIX-shell quoting rules (`shlex`) and
///   executed directly — **not** via `sh -c` — to avoid shell injection.
///
/// # Returns
///
/// - `Ok(true)` if a media window was focused
/// - `Ok(false)` if no media window was found (launch command may have been spawned)
/// - `Err(...)` if Hyprland IPC fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, focus::focus_or_launch};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
///
/// // Focus or launch Jellyfin Media Player
/// let focused = focus_or_launch(
///     &ctx,
///     Some("flatpak run com.github.iwalton3.jellyfin-media-player")
/// ).await?;
///
/// if focused {
///     println!("Focused existing media window");
/// } else {
///     println!("No media window found, launching...");
/// }
/// # Ok(())
/// # }
/// ```
pub async fn focus_or_launch(ctx: &CommandContext, launch_cmd: Option<&str>) -> Result<bool> {
    // Suppress avoider BEFORE the focus dispatch — the activewindow event
    // arrives within the daemon's debounce window, so we must beat it.
    suppress_avoider().await;

    // Try to find a media window
    if let Some(window) = get_media_window(ctx).await? {
        // Focus the window (dispatch prepends "dispatch", so pass bare command)
        ctx.hyprland.dispatch(&format!("focuswindow address:{}", window.address)).await?;

        return Ok(true);
    }

    // No media window — clear the suppression we set above so the next
    // legitimate event isn't dropped. Launch command (if any) won't generate
    // the events we were guarding against.
    clear_suppression().await;

    // Launch command if provided.
    //
    // Split with `shlex` (POSIX-shell quoting rules) instead of passing the
    // raw string to `sh -c`, which would allow shell metacharacters in the
    // keybind config to execute arbitrary shell code. `shlex::split` handles
    // quoted arguments and escaped characters but never invokes a shell.
    if let Some(cmd) = launch_cmd {
        let parts = shlex::split(cmd).unwrap_or_else(|| {
            // Fallback: treat as a single executable with no arguments.
            vec![cmd.to_owned()]
        });
        if let Some((exe, args)) = parts.split_first() {
            Command::new(exe)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[tokio::test]
    async fn focus_existing_media_window() {
        let mock = MockHyprland::start().await;

        let clients = vec![
            make_test_client_full(
                "0xfirefox",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                0,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xmpv",
                "mpv",
                "video.mp4",
                true,
                true,
                0,
                1,
                0,
                1,
                [1272, 712],
                [640, 360],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        let result = focus_or_launch(&ctx, None).await.unwrap();
        assert!(result, "should return true when media window found");

        let cmds = mock.captured_commands().await;
        let has_focus = cmds.iter().any(|c| c.contains("focuswindow address:0xmpv"));
        assert!(has_focus, "should dispatch focuswindow: {cmds:?}");
    }

    #[tokio::test]
    async fn focus_no_media_returns_false() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xfirefox",
            "firefox",
            "Browser",
            false,
            false,
            0,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        let result = focus_or_launch(&ctx, None).await.unwrap();
        assert!(!result, "should return false when no media window");
    }
}
