//! Toggle pinned floating mode with positioning.
//!
//! Pins or unpins the media window and applies appropriate positioning
//! based on the current state and configuration.

use super::{CommandContext, suppress_avoider};
use crate::error::Result;

/// Toggle pinned floating mode for the media window.
///
/// This command toggles between pinned+floating mode and normal mode:
/// - If the window is already pinned AND floating, it disables both
/// - Otherwise, it enables both and positions the window to the configured corner
///
/// # Behavior
///
/// 1. Finds the media window (returns Ok(()) if not found)
/// 2. If fullscreen, returns immediately without changes
/// 3. If already in pinned+floating mode, disables both
/// 4. Otherwise, enables pinned+floating and positions to corner
///
/// # Errors
///
/// Returns an error if Hyprland IPC communication fails.
pub async fn pin_and_float(ctx: &CommandContext) -> Result<()> {
    let Some(media) = super::get_media_window(ctx).await? else {
        return Ok(());
    };

    // Don't modify geometry if fullscreen
    if media.fullscreen > 0 {
        return Ok(());
    }

    let was_floating = media.floating;
    let was_pinned = media.pinned;

    // If both enabled, disable them (unpin then unfloat)
    if was_floating && was_pinned {
        ctx.hyprland
            .batch(&[
                &format!("dispatch pin address:{}", media.address),
                &format!("dispatch togglefloating address:{}", media.address),
            ])
            .await?;
        return Ok(());
    }

    // Enable pinned+floating mode
    // Focus the window first
    ctx.hyprland
        .dispatch(&format!("focuswindow address:{}", media.address))
        .await?;

    // Build batch commands for state changes
    let mut cmds: Vec<String> = Vec::with_capacity(2);
    if !was_floating {
        cmds.push(format!("dispatch togglefloating address:{}", media.address));
    }
    if !was_pinned {
        cmds.push(format!("dispatch pin address:{}", media.address));
    }

    // Execute state changes if any
    if !cmds.is_empty() {
        let cmd_refs: Vec<&str> = cmds.iter().map(String::as_str).collect();
        ctx.hyprland.batch(&cmd_refs).await?;
    }

    // Position to configured default corner (adjusted for minified mode)
    super::reposition_to_default(ctx, &media.address).await?;
    suppress_avoider().await.ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[tokio::test]
    async fn pin_toggle_on_unpinned_unfloated() {
        let mock = MockHyprland::start().await;

        // mpv is not floating, not pinned
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
                false,
                false,
                0,
                1,
                0,
                1,
                [100, 100],
                [640, 360],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        pin_and_float(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should float + pin + position
        let has_float = cmds.iter().any(|c| c.contains("togglefloating"));
        let has_pin = cmds.iter().any(|c| c.contains("dispatch pin"));
        let has_resize = cmds.iter().any(|c| c.contains("resizewindowpixel"));
        assert!(has_float, "should toggle floating: {cmds:?}");
        assert!(has_pin, "should pin: {cmds:?}");
        assert!(has_resize, "should position: {cmds:?}");
    }

    #[tokio::test]
    async fn pin_toggle_off_pinned_floating() {
        let mock = MockHyprland::start().await;

        // mpv is floating + pinned → toggle off
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

        pin_and_float(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should unpin + unfloat in a batch
        let batch = cmds.iter().find(|c| c.contains("dispatch pin"));
        assert!(batch.is_some(), "should unpin: {cmds:?}");
        let batch = batch.unwrap();
        assert!(
            batch.contains("togglefloating"),
            "should also unfloat: {batch}"
        );
        // Should NOT have resize (no positioning when toggling off)
        let has_resize = cmds.iter().any(|c| c.contains("resizewindowpixel"));
        assert!(
            !has_resize,
            "should not position when toggling off: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn pin_fullscreen_is_noop() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xmpv",
            "mpv",
            "video.mp4",
            false,
            false,
            2,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080], // fullscreen
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        pin_and_float(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should only fetch clients, no dispatches
        assert_eq!(
            cmds.len(),
            1,
            "should only fetch clients for fullscreen: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn pin_no_media_window_is_noop() {
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

        pin_and_float(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
    }
}
