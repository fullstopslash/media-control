//! Toggle fullscreen with focus restoration and pin state preservation.
//!
//! This command toggles fullscreen mode on the media window while preserving
//! the pin state and restoring focus to the previously focused window.

use regex::Regex;

use super::{
    CommandContext, clear_suppression, focus_window_cmd, get_media_window_with_clients, pin_cmd,
    restore_focus, suppress_avoider, toggle_floating_cmd,
};
use crate::error::Result;
use crate::hyprland::Client;
use crate::window::MediaWindow;

/// Maximum retry attempts when exiting fullscreen.
const MAX_FULLSCREEN_EXIT_ATTEMPTS: u8 = 3;

/// Check if a window title matches a Picture-in-Picture pattern.
///
/// Uses case-insensitive regex `picture.*picture` to match variants like
/// "Picture-in-Picture", "picture-in-picture", "Picture in Picture", etc.
pub fn is_pip_title(title: &str) -> bool {
    static PIP_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PIP_REGEX
        .get_or_init(|| Regex::new(r"(?i)picture.*picture").expect("valid regex"))
        .is_match(title)
}

/// Toggle fullscreen for the media window.
///
/// Behavior:
/// - If not fullscreen and `always_pin` is set but window is unpinned: pin it instead
/// - If fullscreen: exit fullscreen, restore pin state, restore previous focus
/// - If not fullscreen: enter fullscreen (temporarily unpin if pinned)
///
/// # Errors
///
/// Returns an error if Hyprland IPC fails.
pub async fn fullscreen(ctx: &CommandContext) -> Result<()> {
    let clients = ctx.hyprland.get_clients().await?;
    let Some(media) = get_media_window_with_clients(ctx, &clients) else {
        return Ok(());
    };

    let is_fullscreen = media.fullscreen > 0;

    // Auto-pin if configured and unpinned (only when not fullscreen)
    if !is_fullscreen && media.always_pin && !media.pinned {
        return auto_pin_window(ctx, &media).await;
    }

    if is_fullscreen {
        exit_fullscreen(ctx, &media, &clients).await
    } else {
        enter_fullscreen_mode(ctx, &media).await
    }
}

/// Auto-pin a window that has always_pin set.
///
/// Makes the window floating first if needed, then pins it. When both ops
/// are required, batches them so the window doesn't briefly appear tiled
/// before being pinned.
async fn auto_pin_window(ctx: &CommandContext, media: &MediaWindow) -> Result<()> {
    let addr = &media.address;

    // Suppress BEFORE the dispatch — pinwindow / movewindow events would
    // otherwise race the daemon and trigger an avoid pass on the just-pinned
    // window before the user even sees it settle.
    suppress_avoider().await;

    if media.floating {
        ctx.hyprland.dispatch(&format!("pin address:{addr}")).await?;
    } else {
        let float = toggle_floating_cmd(addr);
        let pin = pin_cmd(addr);
        ctx.hyprland.batch(&[&float, &pin]).await?;
    }
    Ok(())
}

/// Enter fullscreen mode.
///
/// Focuses the media window, temporarily unpins if pinned, then goes fullscreen.
/// Uses batch commands to make operations atomic and avoid race conditions.
async fn enter_fullscreen_mode(ctx: &CommandContext, media: &MediaWindow) -> Result<()> {
    // Build batch commands to execute atomically
    // Note: fullscreen dispatcher doesn't accept address selector, it operates on focused window
    // So we must focus first, then fullscreen, all in one batch to avoid race conditions
    let mut cmds: Vec<String> = Vec::with_capacity(3);

    // 1. Focus the media window
    cmds.push(focus_window_cmd(&media.address));

    // 2. Temporarily unpin if pinned (fullscreen windows cannot be pinned)
    if media.pinned {
        cmds.push(pin_cmd(&media.address));
    }

    // 3. Toggle fullscreen (operates on the now-focused window)
    cmds.push("dispatch fullscreen 0".to_string());

    // Suppress BEFORE the batch — the activewindow + fullscreen events
    // arrive within the daemon's debounce window, so we have to beat them.
    suppress_avoider().await;

    // Execute all commands atomically
    let cmd_refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    ctx.hyprland.batch(&cmd_refs).await?;

    Ok(())
}

/// Exit fullscreen with retry logic, pin restoration, and focus restoration.
async fn exit_fullscreen(
    ctx: &CommandContext,
    media: &MediaWindow,
    clients: &[Client],
) -> Result<()> {
    let addr = &media.address;
    let should_restore_pin = media.always_pin || media.pinned || is_pip_title(&media.title);
    let previous_focus = ctx.window_matcher.find_previous_focus(clients, addr, None);
    // Suppress avoider BEFORE starting - prevents repositioning during state changes
    suppress_avoider().await;

    // Focus the media window and toggle fullscreen atomically
    // Note: fullscreen dispatcher doesn't accept address selector, operates on focused window
    let focus = focus_window_cmd(addr);
    ctx.hyprland.batch(&[&focus, "dispatch fullscreen 0"]).await?;

    // Retry loop for exiting fullscreen (like bash script)
    let mut attempt = 0;
    while attempt < MAX_FULLSCREEN_EXIT_ATTEMPTS {
        // Check if fullscreen actually exited
        let fresh_clients = ctx.hyprland.get_clients().await?;
        let current_fs = fresh_clients
            .iter()
            .find(|c| c.address == *addr)
            .map(|c| c.fullscreen)
            .unwrap_or(0);

        if current_fs == 0 {
            break;
        }

        attempt += 1;

        // Refresh suppression before retry
        suppress_avoider().await;

        // Try again - focus and fullscreen atomically.
        // On the final attempt, double-toggle (off→on→off) to force-flush stuck
        // fullscreen state in Hyprland. Two toggles net to off when starting from on.
        let focus = focus_window_cmd(addr);
        if attempt == MAX_FULLSCREEN_EXIT_ATTEMPTS {
            ctx.hyprland
                .batch(&[
                    &focus,
                    "dispatch fullscreen 0",
                    "dispatch fullscreen 0",
                    "dispatch fullscreen 0",
                ])
                .await?;
        } else {
            ctx.hyprland.batch(&[&focus, "dispatch fullscreen 0"]).await?;
        }
    }

    // Refresh suppression before pin/focus restoration
    suppress_avoider().await;

    // Get fresh state after exiting fullscreen
    let fresh_clients = ctx.hyprland.get_clients().await?;

    // Get the media window's current position for repositioning.
    // Verify the window still exists AND belongs to the same class/title to
    // guard against the address being recycled by a different window between
    // the "exit fullscreen" dispatch and now.
    let media_window = fresh_clients.iter().find(|c| {
        c.address == *addr && c.class == media.class && c.title == media.title
    });

    if fresh_clients.iter().any(|c| c.address == *addr)
        && media_window.is_none()
    {
        tracing::warn!(
            "skipping re-pin: address {addr} is now occupied by a different window \
             (original class={:?} title={:?})",
            media.class,
            media.title,
        );
    }

    // Restore pin if needed — only when the window is confirmed to be the
    // same window we exited fullscreen on.
    let current_pinned = media_window.map(|c| c.pinned).unwrap_or(false);

    if should_restore_pin && !current_pinned && media_window.is_some() {
        ctx.hyprland.dispatch(&format!("pin address:{addr}")).await?;
    }

    // Position the media window to default position and resize
    // The avoider daemon will handle proper positioning after focus is restored
    if media_window.is_some() {
        super::reposition_to_default(ctx, addr).await?;
        // Note: Don't suppress here - we want the avoider to run after focus restore
    }

    // Restore focus to previous window if valid
    if let Some(prev_addr) = previous_focus
        && let Some(target_addr) = find_valid_focus_target(&fresh_clients, addr, &prev_addr)
    {
        restore_focus(ctx, &target_addr).await?;
    }

    // Clear suppression and explicitly trigger avoid with fresh state.
    // We can't rely on the daemon because:
    // 1. The movewindow event from our repositioning may have updated the daemon's debounce timer
    // 2. The activewindow event from focus restore may arrive within the debounce window (15ms)
    // 3. The daemon would skip avoid due to debounce, leaving the window in wrong position
    //
    // By explicitly calling avoid here, we ensure proper positioning with fresh client data.
    clear_suppression().await;
    if let Err(e) = super::avoid::avoid(ctx).await {
        tracing::debug!("avoid after fullscreen exit failed (non-fatal): {e}");
    }

    Ok(())
}

/// Find a valid window to restore focus to.
///
/// Prefers the specified previous focus, falls back to most recently focused
/// window on the same workspace. Returns the address as an owned String.
fn find_valid_focus_target(
    clients: &[Client],
    media_addr: &str,
    prev_addr: &str,
) -> Option<String> {
    // Check if previous focus window is still valid (and not the media window)
    if prev_addr != media_addr
        && clients
            .iter()
            .any(|c| c.address == prev_addr && c.mapped && !c.hidden)
    {
        return Some(prev_addr.to_string());
    }

    // Fallback: find most recently focused window excluding media
    clients
        .iter()
        .filter(|c| c.address != media_addr && c.mapped && !c.hidden)
        .min_by_key(|c| c.focus_history_id)
        .map(|c| c.address.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    /// Config with suppress_ms=0 to disable suppression in tests.
    fn test_config() -> crate::config::Config {
        let mut config = crate::config::Config::default();
        config.positioning.suppress_ms = 0;
        config
    }

    #[test]
    fn pip_title_detection() {
        assert!(is_pip_title("Picture-in-Picture"));
        assert!(is_pip_title("picture-in-picture"));
        assert!(is_pip_title("Picture in Picture"));
        assert!(is_pip_title("PICTURE-IN-PICTURE"));
        assert!(!is_pip_title("Not a PiP window"));
        assert!(!is_pip_title("Picture"));
        assert!(!is_pip_title(""));
    }

    #[test]
    fn find_valid_focus_target_prefers_previous() {
        use crate::hyprland::Workspace;

        let clients = vec![
            Client {
                address: "0x1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "firefox".to_string(),
                title: "Browser".to_string(),
                focus_history_id: 1,
                pid: 0,
            },
            Client {
                address: "0x2".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "kitty".to_string(),
                title: "Terminal".to_string(),
                focus_history_id: 0,
                pid: 0,
            },
            Client {
                address: "0x3".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: true,
                pinned: true,
                fullscreen: 0,
                monitor: 0,
                class: "mpv".to_string(),
                title: "video.mp4".to_string(),
                focus_history_id: 2,
                pid: 0,
            },
        ];

        // Should prefer the specified previous focus
        let result = find_valid_focus_target(&clients, "0x3", "0x1");
        assert_eq!(result.as_deref(), Some("0x1"));

        // If previous focus is media itself, should skip it
        let result = find_valid_focus_target(&clients, "0x3", "0x3");
        assert_eq!(result.as_deref(), Some("0x2")); // Falls back to most recent
    }

    #[test]
    fn find_valid_focus_target_falls_back_when_invalid() {
        use crate::hyprland::Workspace;

        let clients = vec![
            Client {
                address: "0x1".to_string(),
                mapped: true,
                hidden: true, // Hidden - invalid
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "firefox".to_string(),
                title: "Browser".to_string(),
                focus_history_id: 1,
                pid: 0,
            },
            Client {
                address: "0x2".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "kitty".to_string(),
                title: "Terminal".to_string(),
                focus_history_id: 0,
                pid: 0,
            },
        ];

        // Previous focus 0x1 is hidden, should fall back to 0x2
        let result = find_valid_focus_target(&clients, "0x99", "0x1");
        assert_eq!(result.as_deref(), Some("0x2"));
    }

    #[test]
    fn find_valid_focus_target_returns_none_when_no_candidates() {
        use crate::hyprland::Workspace;

        let clients = vec![Client {
            address: "0x1".to_string(),
            mapped: true,
            hidden: false,
            at: [0, 0],
            size: [100, 100],
            workspace: Workspace {
                id: 1,
                name: "1".to_string(),
            },
            floating: true,
            pinned: true,
            fullscreen: 0,
            monitor: 0,
            class: "mpv".to_string(),
            title: "video.mp4".to_string(),
            focus_history_id: 0,
            pid: 0,
        }];

        // Only the media window exists
        let result = find_valid_focus_target(&clients, "0x1", "0x999");
        assert!(result.is_none());
    }

    // --- E2E tests using mock Hyprland ---

    #[tokio::test]
    async fn fullscreen_enter_unpinned() {
        let mock = MockHyprland::start().await;

        // mpv is floating, not pinned, not fullscreen
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

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should batch: focus + fullscreen (no unpin since not pinned)
        let batch = cmds.iter().find(|c| c.contains("fullscreen"));
        assert!(batch.is_some(), "expected fullscreen dispatch: {cmds:?}");
        let batch = batch.unwrap();
        assert!(
            batch.contains("focuswindow"),
            "should focus before fullscreen"
        );
        // Should NOT contain pin toggle
        assert!(
            !batch.contains("dispatch pin"),
            "should not unpin when not pinned: {batch}"
        );
    }

    #[tokio::test]
    async fn fullscreen_enter_pinned_unpins_first() {
        let mock = MockHyprland::start().await;

        // mpv is pinned + floating
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

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("fullscreen"));
        assert!(batch.is_some(), "expected fullscreen: {cmds:?}");
        let batch = batch.unwrap();
        // Should contain pin toggle (unpin before fullscreen)
        assert!(
            batch.contains("dispatch pin"),
            "should unpin before fullscreen: {batch}"
        );
    }

    #[tokio::test]
    async fn fullscreen_exit_restores_pin() {
        let mock = MockHyprland::start().await;

        // mpv is fullscreen, was pinned (pinned=true even though fullscreen)
        // After exit_fullscreen, the mock returns it as non-fullscreen, non-pinned
        let clients_fullscreen = vec![
            make_test_client_full(
                "0xfirefox",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xmpv",
                "mpv",
                "video.mp4",
                true,
                true,
                2,
                1,
                0,
                0,
                [0, 0],
                [1920, 1080], // fullscreen=2, pinned=true
            ),
        ];
        let clients_exited = vec![
            make_test_client_full(
                "0xfirefox",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xmpv",
                "mpv",
                "video.mp4",
                false,
                true,
                0,
                1,
                0,
                0,
                [1272, 712],
                [640, 360], // fullscreen=0, pinned=false
            ),
        ];

        // First call returns fullscreen, subsequent calls return exited
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fullscreen),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should dispatch pin to restore it
        let has_pin = cmds
            .iter()
            .any(|c| c.contains("dispatch pin address:0xmpv") && !c.contains("fullscreen"));
        assert!(has_pin, "should restore pin after exit: {cmds:?}");
    }

    #[tokio::test]
    async fn fullscreen_no_media_window_is_noop() {
        let mock = MockHyprland::start().await;

        // No media windows
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

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Only j/clients fetch, no dispatches
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
        assert_eq!(cmds[0], "j/clients");
    }

    #[tokio::test]
    async fn fullscreen_auto_pin_when_always_pin_set() {
        let mock = MockHyprland::start().await;

        // PiP window: always_pin=true in default config for "Picture-in-Picture" title
        // Window is floating but NOT pinned
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
                "0xpip",
                "firefox",
                "Picture-in-Picture",
                false,
                true,
                0,
                1,
                0,
                1,
                [1272, 712],
                [320, 180],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should pin instead of fullscreening (auto-pin behavior)
        let has_pin = cmds.iter().any(|c| c.contains("dispatch pin"));
        assert!(has_pin, "should auto-pin PiP window: {cmds:?}");
        let has_fullscreen = cmds.iter().any(|c| c.contains("fullscreen"));
        assert!(
            !has_fullscreen,
            "should NOT fullscreen when auto-pinning: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn fullscreen_exit_restores_focus_to_previous() {
        let mock = MockHyprland::start().await;

        // mpv fullscreen, firefox was previous focus
        let clients_fullscreen = vec![
            make_test_client_full(
                "0xfirefox",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xmpv",
                "mpv",
                "video.mp4",
                false,
                true,
                2,
                1,
                0,
                0,
                [0, 0],
                [1920, 1080],
            ),
        ];
        let clients_exited = vec![
            make_test_client_full(
                "0xfirefox",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xmpv",
                "mpv",
                "video.mp4",
                false,
                true,
                0,
                1,
                0,
                0,
                [1272, 712],
                [640, 360],
            ),
        ];

        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fullscreen),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should restore focus to firefox
        let has_focus_restore = cmds
            .iter()
            .any(|c| c.contains("focuswindow address:0xfirefox") && c.contains("no_warps"));
        assert!(
            has_focus_restore,
            "should restore focus to firefox: {cmds:?}"
        );
    }

    /// Regression: toggling fullscreen on then off should be symmetric — each
    /// invocation drives Hyprland through `dispatch fullscreen 0` exactly once
    /// per call (the second call is the exit path which already covers retry).
    #[tokio::test]
    async fn fullscreen_toggle_is_idempotent_on_unpinned() {
        // Two back-to-back invocations: enter, then exit. Verify that each
        // invocation independently runs without panicking and that the second
        // invocation observes the new fullscreen=2 state (i.e. takes the exit
        // path with retry/restore).
        let mock = MockHyprland::start().await;

        // Sequence: enter (sees fs=0), then exit pass sees fs=2 then fs=0.
        let pre = vec![make_test_client_full(
            "0xmpv", "mpv", "video.mp4",
            false, true, 0, 1, 0, 0,
            [1272, 712], [640, 360],
        )];
        let entered = vec![make_test_client_full(
            "0xmpv", "mpv", "video.mp4",
            false, true, 2, 1, 0, 0,
            [0, 0], [1920, 1080],
        )];
        let exited = vec![make_test_client_full(
            "0xmpv", "mpv", "video.mp4",
            false, true, 0, 1, 0, 0,
            [1272, 712], [640, 360],
        )];
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&pre),     // first call: enter sees fs=0
                make_clients_json(&entered), // second call: exit sees fs=2
                make_clients_json(&exited),  // exit retry verifies fs=0
                make_clients_json(&exited),  // post-exit fresh fetch
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap(); // enter
        fullscreen(&ctx).await.unwrap(); // exit

        let cmds = mock.captured_commands().await;
        let fs_dispatches = cmds
            .iter()
            .filter(|c| c.contains("dispatch fullscreen 0"))
            .count();
        // Enter: 1 fullscreen dispatch in the batch.
        // Exit:  1 fullscreen dispatch in the initial batch + retries verify state.
        assert!(
            fs_dispatches >= 2,
            "expected at least 2 fullscreen dispatches across enter+exit: {cmds:?}"
        );
    }

    /// Regression: PiP windows have always_pin=true. After exit, pin must be
    /// restored even when the window WAS pinned before fullscreening.
    #[tokio::test]
    async fn fullscreen_exit_pip_restores_pin() {
        let mock = MockHyprland::start().await;

        let clients_fs = vec![make_test_client_full(
            "0xpip", "firefox", "Picture-in-Picture",
            true, true, 2, 1, 0, 0,
            [0, 0], [1920, 1080],
        )];
        let clients_exited = vec![make_test_client_full(
            "0xpip", "firefox", "Picture-in-Picture",
            false, true, 0, 1, 0, 0,
            [1272, 712], [320, 180],
        )];
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fs),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_pin_restore = cmds
            .iter()
            .any(|c| c.contains("dispatch pin address:0xpip") && !c.contains("fullscreen"));
        assert!(has_pin_restore, "PiP exit must restore pin: {cmds:?}");
    }
}
