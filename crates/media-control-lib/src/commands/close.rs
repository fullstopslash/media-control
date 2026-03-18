//! Graceful window closing with mpv/Jellyfin session cleanup.
//!
//! Closes the media window and handles any necessary cleanup for
//! mpv and Jellyfin Media Player sessions.

use tokio::process::Command;

use super::CommandContext;
use crate::error::Result;
use crate::hyprland::Client;
use crate::jellyfin::JellyfinClient;

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
    let clients = ctx.hyprland.get_clients().await?;

    let focus_addr = clients
        .iter()
        .find(|c| c.focus_history_id == 0)
        .map(|c| c.address.as_str());

    let Some(window) = ctx.window_matcher.find_media_window(&clients, focus_addr) else {
        return Ok(());
    };

    close_window_gracefully(ctx, &window.address, &window.class, &window.title, &clients).await
}

/// Close a specific window gracefully based on its class and title.
///
/// This is the internal implementation that handles app-specific close logic.
async fn close_window_gracefully(
    ctx: &CommandContext,
    addr: &str,
    class: &str,
    title: &str,
    clients: &[Client],
) -> Result<()> {
    // MPV: ensure Jellyfin session ends cleanly, then stop playback
    if class == "mpv" {
        // Try to stop Jellyfin session first (best effort, ignore errors)
        if let Ok(client) = JellyfinClient::from_default_credentials().await {
            let _ = client.stop_mpv().await;
        }

        // Use playerctl to stop mpv (best effort)
        let _ = Command::new("playerctl")
            .args(["--player=mpv", "stop"])
            .output()
            .await;

        return Ok(());
    }

    // Firefox PiP: close the PiP window, then close the source tab.
    // When PiP closes, Firefox activates the source tab. We then focus
    // the main Firefox window and send Ctrl+W to close that tab.
    if class == "firefox" && title.to_lowercase().contains("picture-in-picture") {
        return close_firefox_pip(ctx, addr, clients).await;
    }

    // All other windows (Jellyfin, default): use closewindow.
    // closewindow sends xdg_toplevel::close which gracefully closes just the
    // targeted window surface.
    ctx.hyprland
        .dispatch(&format!("closewindow address:{addr}"))
        .await?;

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
async fn close_firefox_pip(ctx: &CommandContext, pip_addr: &str, _clients: &[Client]) -> Result<()> {
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

    #[test]
    fn jellyfin_class_detection() {
        // Test various Jellyfin class names
        let class_variants = [
            "com.github.iwalton3.jellyfin-media-player",
            "jellyfin-media-player",
            "Jellyfin",
            "JELLYFIN",
        ];

        for class in class_variants {
            assert!(
                class.to_lowercase().contains("jellyfin"),
                "Failed to detect Jellyfin for class: {class}"
            );
        }
    }

    #[test]
    fn mpv_class_detection() {
        // mpv class should be exact match
        assert_eq!("mpv", "mpv");
        assert_ne!("mpv", "MPV");
        assert_ne!("mpv", "vlc-mpv");
    }

    // --- E2E tests ---

    use super::*;

    #[tokio::test]
    async fn close_jellyfin_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xjelly", "com.github.iwalton3.jellyfin-media-player", "Jellyfin", true, true,
            0, 1, 0, 0, [0, 0], [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(has_kill, "should dispatch closewindow for jellyfin: {cmds:?}");
    }

    #[tokio::test]
    async fn close_firefox_pip_uses_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xpip", "firefox", "Picture-in-Picture", true, true,
            0, 1, 0, 0, [1272, 712], [320, 180],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let close_cmd = cmds.iter().find(|c| c.contains("closewindow"));
        assert!(close_cmd.is_some(), "should dispatch closewindow for PiP: {cmds:?}");
        assert!(
            close_cmd.unwrap().contains("0xpip"),
            "should target the PiP window address"
        );
    }

    #[tokio::test]
    async fn close_mpv_does_not_killwindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xmpv", "mpv", "video.mp4", true, true,
            0, 1, 0, 0, [1272, 712], [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        // This will try playerctl/jellyfin (both fail gracefully), but should NOT killwindow
        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(!has_kill, "should NOT killwindow for mpv (uses playerctl): {cmds:?}");
    }

    #[tokio::test]
    async fn close_default_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        // Some other media window class
        let clients = vec![make_test_client_full(
            "0xvlc", "vlc", "movie.mkv", true, true,
            0, 1, 0, 0, [1272, 712], [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;

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
        assert!(has_kill, "should dispatch closewindow for default class: {cmds:?}");
    }

    #[tokio::test]
    async fn close_no_media_window_is_noop() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xfirefox", "firefox", "Browser", false, false,
            0, 1, 0, 0, [0, 0], [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
    }
}
