//! Toggle pinned floating mode with positioning.
//!
//! Pins or unpins the media window and applies appropriate positioning
//! based on the current state and configuration.

use super::{
    CommandContext, as_str_refs, focus_window_action, pin_action, suppress_avoider,
    toggle_floating_action,
};
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

    let addr = &media.address;

    // Suppress at the top — every dispatch below (pin/unpin/float/move) will
    // generate Hyprland events that arrive within the daemon's debounce
    // window. We have to beat all of them to the suppress file, not just the
    // final reposition.
    suppress_avoider().await;

    // State matrix — four entry combinations, two outcomes:
    //
    // | floating | pinned | branch                                          |
    // |----------|--------|-------------------------------------------------|
    // | true     | true   | DISABLE: unpin + unfloat (return early)         |
    // | true     | false  | ENABLE: focus + pin            → reposition     |
    // | false    | true   | ENABLE: focus + float          → reposition     |
    // | false    | false  | ENABLE: focus + float + pin    → reposition     |
    //
    // The toggle is "all-on ↔ all-off": only the (floating + pinned) cell
    // disables; every other cell drives the window TO the (floating + pinned)
    // state and then repositions to the configured corner. The two `if !was_*`
    // checks below skip the dispatcher action when that bit is already set,
    // since `togglefloating` / `dispatch pin` would otherwise *flip* it the
    // wrong direction.
    if was_floating && was_pinned {
        ctx.hyprland
            .dispatch_batch(&[&pin_action(addr), &toggle_floating_action(addr)])
            .await?;
        return Ok(());
    }

    // Enable pinned+floating mode. Focus + state changes go in a single
    // batch so Hyprland processes them atomically (no intermediate render
    // where the window is e.g. floated-but-unfocused) and we save a socket
    // round-trip vs dispatching focus separately.
    let mut cmds: Vec<String> = Vec::with_capacity(3);
    cmds.push(focus_window_action(addr));
    if !was_floating {
        cmds.push(toggle_floating_action(addr));
    }
    if !was_pinned {
        cmds.push(pin_action(addr));
    }
    ctx.hyprland.dispatch_batch(&as_str_refs(&cmds)).await?;

    // Refresh suppression before the reposition — the prior batch produced
    // activewindow + (maybe) floating + (maybe) pin events that consumed
    // debounce cycles; we want a fresh timestamp covering the move/resize
    // batch issued by `reposition_to_default`.
    suppress_avoider().await;

    // Position to configured default corner (adjusted for minified mode)
    super::reposition_to_default(ctx, addr).await?;

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
            "0xd1",
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

        pin_and_float(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
    }
}
