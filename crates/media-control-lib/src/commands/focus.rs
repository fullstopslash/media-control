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

use super::{CommandContext, focus_window_action, get_media_window, suppress_avoider};
use crate::error::{MediaControlError, Result};

/// Focus the media window, or launch a command if no media window exists.
///
/// This command:
/// 1. Searches for a media window matching the configured patterns
/// 2. If found, suppresses the avoider then focuses it via Hyprland IPC
///    (suppression is scoped to the actual dispatch so a transient lookup
///    failure does not silence the next legitimate avoid event)
/// 3. If not found and a launch command is provided, spawns that command
///
/// # Arguments
///
/// * `ctx` - The command context
/// * `launch_cmd` - Optional command to run if no media window is found.
///   The command is executed via `sh -c` for shell expansion.
///
/// # Safety
///
/// `launch_cmd` is executed via `/bin/sh -c` with full shell expansion.
/// Never pass untrusted input — this is only safe with hardcoded config values.
/// Any unescaped metacharacters in `launch_cmd` (`;`, `|`, `$(...)`, backticks,
/// redirections) will be interpreted by the shell.
///
/// **Concrete failure mode**: callers wrapping `media-control` (Hyprland
/// keybinds, helper scripts, IPC daemons, GUI launchers) MUST NOT thread
/// user-controlled data into this argument. Examples of unsafe sources include
/// browser/window titles (an attacker-controlled page title becomes shell
/// input), clipboard contents, filenames from the filesystem, environment
/// variables sourced from untrusted contexts, and anything read from a
/// socket. A title like `Picture-in-Picture; rm -rf ~` would execute the
/// trailing command. Build the launch string from a fixed allowlist of
/// commands defined in your own config, never from runtime data.
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
    // Look up the media window FIRST. Suppressing before the lookup means a
    // transient `get_media_window` failure leaks the suppression for the
    // configured window (worst case: the avoider misses its next event). By
    // suppressing only when we know we're about to dispatch we keep
    // suppression scoped to operations that actually move/focus windows.
    if let Some(window) = get_media_window(ctx).await? {
        // Suppress avoider BEFORE the focus dispatch — the activewindow event
        // arrives within the daemon's debounce window, so we must beat it.
        suppress_avoider().await;

        // Focus the window (dispatch prepends "dispatch", so pass bare command)
        ctx.hyprland
            .dispatch(&focus_window_action(&window.address))
            .await?;

        return Ok(true);
    }

    // Launch command if provided. No suppression needed — `sh -c <cmd>`
    // doesn't itself generate Hyprland events; the launched app's `openwindow`
    // is precisely the event we want the avoider to react to.
    if let Some(cmd) = launch_cmd {
        // Spawn in background (don't wait for it). `process_group(0)` puts the
        // child in its own process group so it survives if `media-control`
        // exits — without it, a SIGHUP/SIGINT delivered to our process group
        // (e.g. when launched from a terminal that closes) would kill the
        // newly-launched media app too.
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        command.spawn()?;
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
                "0xb1",
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
                "0xd1",
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
        let has_focus = cmds.iter().any(|c| c.contains("focuswindow address:0xd1"));
        assert!(has_focus, "should dispatch focuswindow: {cmds:?}");
    }

    #[tokio::test]
    async fn focus_no_media_returns_false() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xb1",
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
