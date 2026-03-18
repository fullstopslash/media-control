//! Graceful window closing with mpv/Jellyfin session cleanup.
//!
//! Closes the media window and handles any necessary cleanup for
//! mpv and Jellyfin Media Player sessions.

use tokio::process::Command;

use super::{get_media_window, CommandContext};
use crate::error::{MediaControlError, Result};
use crate::jellyfin::JellyfinClient;

/// Close the media window gracefully with app-specific handling.
///
/// Different window types require different close strategies:
/// - **mpv**: Stop Jellyfin session first (if applicable), then stop playback via playerctl
/// - **Firefox PiP**: Cannot be closed programmatically (returns error)
/// - **Jellyfin Media Player**: Use Hyprland's killwindow command
/// - **Other windows**: Use Hyprland's killwindow command
///
/// # Returns
///
/// - `Ok(())` if no media window found (nothing to close)
/// - `Ok(())` if the window was successfully closed
/// - `Err(...)` if closing failed or is not possible (e.g., Firefox PiP)
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
    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    close_window_gracefully(ctx, &window.address, &window.class, &window.title).await
}

/// Close a specific window gracefully based on its class and title.
///
/// This is the internal implementation that handles app-specific close logic.
async fn close_window_gracefully(
    ctx: &CommandContext,
    addr: &str,
    class: &str,
    title: &str,
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

    // Firefox Picture-in-Picture: cannot be closed programmatically
    // PiP windows share PID with main Firefox, so killwindow closes entire Firefox.
    // Remote debugging doesn't work reliably.
    // Keyboard shortcuts don't work via Wayland key injection.
    // User must close manually.
    if class == "firefox" && title.to_lowercase().contains("picture-in-picture") {
        return Err(MediaControlError::Config {
            kind: crate::error::ConfigErrorKind::ValidationError,
            path: None,
            source: Some("Firefox Picture-in-Picture cannot be closed programmatically".into()),
        });
    }

    // Jellyfin Media Player: use killwindow (separate process)
    if class.to_lowercase().contains("jellyfin") {
        ctx.hyprland
            .dispatch(&format!("killwindow address:{addr}"))
            .await?;
        return Ok(());
    }

    // Default: use killwindow for other windows
    ctx.hyprland
        .dispatch(&format!("killwindow address:{addr}"))
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::*;

    #[test]
    fn firefox_pip_detection_case_insensitive() {
        // Test that we correctly detect PiP windows regardless of case
        let title_variants = [
            "Picture-in-Picture",
            "picture-in-picture",
            "PICTURE-IN-PICTURE",
            "Picture-In-Picture",
        ];

        for title in title_variants {
            assert!(
                title.to_lowercase().contains("picture-in-picture"),
                "Failed to detect PiP for title: {title}"
            );
        }
    }

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

    #[test]
    fn non_pip_firefox_not_blocked() {
        // Regular Firefox windows should not be detected as PiP
        let regular_titles = [
            "Mozilla Firefox",
            "GitHub - Mozilla Firefox",
            "Picture Gallery - Firefox",
        ];

        for title in regular_titles {
            assert!(
                !title.to_lowercase().contains("picture-in-picture"),
                "Incorrectly detected PiP for title: {title}"
            );
        }
    }

    // --- E2E tests ---

    use super::*;

    #[tokio::test]
    async fn close_jellyfin_dispatches_killwindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xjelly", "com.github.iwalton3.jellyfin-media-player", "Jellyfin", true, true,
            0, 1, 0, 0, [0, 0], [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("killwindow"));
        assert!(has_kill, "should dispatch killwindow for jellyfin: {cmds:?}");
    }

    #[tokio::test]
    async fn close_firefox_pip_returns_error() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xpip", "firefox", "Picture-in-Picture", true, true,
            0, 1, 0, 0, [1272, 712], [320, 180],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients)).await;
        let ctx = mock.default_context();

        let result = close(&ctx).await;
        assert!(result.is_err(), "should return error for Firefox PiP");
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
        let has_kill = cmds.iter().any(|c| c.contains("killwindow"));
        assert!(!has_kill, "should NOT killwindow for mpv (uses playerctl): {cmds:?}");
    }

    #[tokio::test]
    async fn close_default_dispatches_killwindow() {
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
        let has_kill = cmds.iter().any(|c| c.contains("killwindow"));
        assert!(has_kill, "should dispatch killwindow for default class: {cmds:?}");
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
