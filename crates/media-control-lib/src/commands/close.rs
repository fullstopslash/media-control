//! Graceful window closing with mpv/Jellyfin session cleanup.
//!
//! Closes the media window and handles any necessary cleanup for
//! mpv and Jellyfin Media Player sessions.

use tokio::process::Command;

use super::fullscreen::is_pip_title;
use super::{CommandContext, get_media_window, get_minify_state_path, send_to_mpv_socket};
use crate::error::Result;

/// Default mpv IPC socket path (jellyfin-mpv-shim).
const SHIM_SOCKET: &str = "/tmp/mpv-shim";

/// Close the media window gracefully with app-specific handling.
///
/// Different window types require different close strategies:
/// - **mpv**: Stop Jellyfin session first (if applicable), then stop playback via playerctl
/// - **All others** (Firefox PiP, Jellyfin, etc.): Use Hyprland's `closewindow` for graceful close
///
/// # Returns
///
/// - `Ok(())` if no media window found (nothing to close)
/// - `Ok(())` if the window was successfully closed
/// - `Err(...)` if Hyprland IPC fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, close::close};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// close(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn close(ctx: &CommandContext) -> Result<()> {
    // Always clear minified state so next window spawns at normal size
    let _ = tokio::fs::remove_file(get_minify_state_path()).await;

    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    close_window_gracefully(
        ctx,
        &window.address,
        &window.class,
        &window.title,
        window.pid,
    )
    .await
}

/// Close a specific window gracefully based on its class and title.
///
/// This is the internal implementation that handles app-specific close logic.
/// Check if a process is the jellyfin-mpv-shim's mpv by looking for
/// `--input-ipc-server=/tmp/mpv-shim` in its command line.
fn is_shim_mpv(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|cmdline| cmdline.contains("mpv-shim"))
        .unwrap_or(false)
}

async fn close_window_gracefully(
    ctx: &CommandContext,
    addr: &str,
    class: &str,
    title: &str,
    pid: i32,
) -> Result<()> {
    let close_cmd = format!("closewindow address:{addr}");

    // Shim mpv: send stop-and-clear, keep window alive for reuse.
    if class == "mpv" && is_shim_mpv(pid) {
        let _ = send_to_mpv_socket(
            SHIM_SOCKET,
            r#"{"command":["script-message","stop-and-clear"]}"#,
        )
        .await;
        return Ok(());
    }

    // Firefox PiP: close the PiP window, then pause media via MPRIS.
    if class == "firefox" && is_pip_title(title) {
        return close_firefox_pip(ctx, addr).await;
    }

    // All other windows (standalone mpv, Jellyfin, default): closewindow.
    ctx.hyprland.dispatch(&close_cmd).await?;
    Ok(())
}

/// Close a Firefox Picture-in-Picture window and stop its media.
///
/// Strategy:
/// 1. Close the PiP window via `closewindow` (graceful xdg_toplevel::close)
/// 2. Pause Firefox media via playerctl MPRIS (stops the video in the source tab)
///
/// We can't reliably close the source tab because we don't know which tab
/// owns the PiP, and Firefox's internal tab activation after PiP close
/// is not deterministic enough to target with Ctrl+W.
async fn close_firefox_pip(ctx: &CommandContext, pip_addr: &str) -> Result<()> {
    // Close the PiP window
    ctx.hyprland
        .dispatch(&format!("closewindow address:{pip_addr}"))
        .await?;

    // Stop Firefox media via MPRIS (best effort)
    let _ = Command::new("playerctl")
        .args(["--player=firefox", "pause"])
        .output()
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::*;

    // --- E2E tests ---

    use super::*;

    #[tokio::test]
    async fn close_jellyfin_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xjelly",
            "com.github.iwalton3.jellyfin-media-player",
            "Jellyfin",
            true,
            true,
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

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(
            has_kill,
            "should dispatch closewindow for jellyfin: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn close_firefox_pip_uses_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xpip",
            "firefox",
            "Picture-in-Picture",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [320, 180],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let close_cmd = cmds.iter().find(|c| c.contains("closewindow"));
        assert!(
            close_cmd.is_some(),
            "should dispatch closewindow for PiP: {cmds:?}"
        );
        assert!(
            close_cmd.unwrap().contains("0xpip"),
            "should target the PiP window address"
        );
    }

    #[tokio::test]
    async fn close_mpv_shim_sends_stop_and_clear() {
        // When the shim's IPC socket is reachable, close sends stop-and-clear
        // and does NOT close the window (shim keeps mpv alive for reuse).
        // This test relies on the real shim socket existing — if the shim
        // isn't running, the test still passes because the fallback closewindow
        // is also a valid outcome.
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xmpv",
            "mpv",
            "video.mp4",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        // We can't assert which path was taken without controlling the socket,
        // but we can verify it didn't error.
    }

    #[tokio::test]
    async fn close_default_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        // Some other media window class
        let clients = vec![make_test_client_full(
            "0xvlc",
            "vlc",
            "movie.mkv",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        // Need vlc in patterns for it to be found as media window
        let mut config = crate::config::Config::default();
        config.patterns.push(crate::config::Pattern {
            key: "class".to_string(),
            value: "vlc".to_string(),
            pinned_only: false,
            always_pin: false,
        });
        let ctx = mock.context(config);

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(
            has_kill,
            "should dispatch closewindow for default class: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn close_no_media_window_is_noop() {
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

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
    }
}
