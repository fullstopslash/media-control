//! Toggle fullscreen with focus restoration and pin state preservation.
//!
//! This command toggles fullscreen mode on the media window while preserving
//! the pin state and restoring focus to the previously focused window.

use regex::Regex;

use super::{clear_suppression, get_media_window_with_clients, restore_focus, suppress_avoider, CommandContext};
use crate::error::Result;
use crate::hyprland::Client;
use crate::window::MediaWindow;

/// Maximum retry attempts when exiting fullscreen.
const MAX_FULLSCREEN_EXIT_ATTEMPTS: u8 = 3;

/// Regex pattern for detecting Picture-in-Picture windows.
fn is_pip_title(title: &str) -> bool {
    // Match "[Pp]icture.*[Pp]icture" pattern from bash
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
        exit_fullscreen_mode(ctx, &media, &clients).await
    } else {
        enter_fullscreen_mode(ctx, &media).await
    }
}

/// Auto-pin a window that has always_pin set.
///
/// Makes the window floating first if needed, then pins it.
async fn auto_pin_window(ctx: &CommandContext, media: &MediaWindow) -> Result<()> {
    if !media.floating {
        ctx.hyprland
            .dispatch(&format!("togglefloating address:{}", media.address))
            .await
            ?;
    }
    ctx.hyprland
        .dispatch(&format!("pin address:{}", media.address))
        .await
        ?;
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
    cmds.push(format!("dispatch focuswindow address:{}", media.address));

    // 2. Temporarily unpin if pinned (fullscreen windows cannot be pinned)
    if media.pinned {
        cmds.push(format!("dispatch pin address:{}", media.address));
    }

    // 3. Toggle fullscreen (operates on the now-focused window)
    cmds.push("dispatch fullscreen 0".to_string());

    // Execute all commands atomically
    let cmd_refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    ctx.hyprland.batch(&cmd_refs).await?;

    // Suppress avoider to prevent repositioning
    let _ = suppress_avoider().await;

    Ok(())
}

/// Exit fullscreen mode with pin restoration and focus restoration.
async fn exit_fullscreen_mode(
    ctx: &CommandContext,
    media: &MediaWindow,
    clients: &[Client],
) -> Result<()> {
    // Determine if we should restore pin state
    let should_restore_pin =
        media.always_pin || media.pinned || is_pip_title(&media.title);

    // Find previous focus window before exiting fullscreen
    let previous_focus = ctx
        .window_matcher
        .find_previous_focus(clients, &media.address, None);

    exit_fullscreen(
        ctx,
        &media.address,
        should_restore_pin,
        media.pinned,
        previous_focus,
        clients,
    )
    .await
}

/// Exit fullscreen with retry logic and state restoration.
///
/// # Arguments
///
/// * `ctx` - Command context
/// * `addr` - Window address
/// * `should_restore_pin` - Whether to restore pin state after exiting fullscreen
/// * `was_pinned` - Whether the window was pinned before entering fullscreen
/// * `previous_focus` - Address of window to restore focus to
/// * `_clients` - Current client list (unused, kept for API compatibility)
#[allow(clippy::too_many_arguments)]
async fn exit_fullscreen(
    ctx: &CommandContext,
    addr: &str,
    should_restore_pin: bool,
    was_pinned: bool,
    previous_focus: Option<String>,
    _clients: &[Client],
) -> Result<()> {
    // Suppress avoider BEFORE starting - prevents repositioning during state changes
    let _ = suppress_avoider().await;

    // Focus the media window and toggle fullscreen atomically
    // Note: fullscreen dispatcher doesn't accept address selector, operates on focused window
    ctx.hyprland
        .batch(&[
            &format!("dispatch focuswindow address:{addr}"),
            "dispatch fullscreen 0",
        ])
        .await
        ?;

    // Retry loop for exiting fullscreen (like bash script)
    let mut attempt = 0;
    while attempt < MAX_FULLSCREEN_EXIT_ATTEMPTS {
        // Check if fullscreen actually exited
        let fresh_clients = ctx.hyprland.get_clients().await?;
        let current_fs = fresh_clients
            .iter()
            .find(|c| c.address == addr)
            .map(|c| c.fullscreen)
            .unwrap_or(0);

        if current_fs == 0 {
            break;
        }

        attempt += 1;

        // Refresh suppression before retry
        let _ = suppress_avoider().await;

        // Try again - focus and fullscreen atomically
        ctx.hyprland
            .batch(&[
                &format!("dispatch focuswindow address:{addr}"),
                "dispatch fullscreen 0",
            ])
            .await
            ?;

        // Aggressive double-toggle on final attempt
        if attempt == MAX_FULLSCREEN_EXIT_ATTEMPTS {
            ctx.hyprland
                .batch(&[
                    &format!("dispatch focuswindow address:{addr}"),
                    "dispatch fullscreen 0",
                    "dispatch fullscreen 0",
                ])
                .await
                ?;
        }
    }

    // Refresh suppression before pin/focus restoration
    let _ = suppress_avoider().await;

    // Get fresh state after exiting fullscreen
    let fresh_clients = ctx.hyprland.get_clients().await?;

    // Get the media window's current position for repositioning
    let media_window = fresh_clients.iter().find(|c| c.address == addr);

    // Restore pin if needed
    let current_pinned = media_window.map(|c| c.pinned).unwrap_or(false);

    if (should_restore_pin || was_pinned) && !current_pinned {
        ctx.hyprland
            .dispatch(&format!("pin address:{addr}"))
            .await
            ?;
    }

    // Position the media window to default position and resize
    // The avoider daemon will handle proper positioning after focus is restored
    if media_window.is_some() {
        let positions = &ctx.config.positions;
        let positioning = &ctx.config.positioning;

        // Use default position - avoider will adjust if needed after focus restore
        let target_x = ctx
            .config
            .resolve_position(&positioning.default_x)
            .unwrap_or(positions.x_right);
        let target_y = ctx
            .config
            .resolve_position(&positioning.default_y)
            .unwrap_or(positions.y_bottom);

        // Move to default position
        ctx.hyprland
            .batch(&[
                &format!(
                    "dispatch movewindowpixel exact {} {},address:{}",
                    target_x, target_y, addr
                ),
                &format!(
                    "dispatch resizewindowpixel exact {} {},address:{}",
                    positions.width, positions.height, addr
                ),
            ])
            .await
            .ok(); // Ignore move errors

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
    let _ = clear_suppression().await;
    let _ = super::avoid::avoid(ctx).await;

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
        }];

        // Only the media window exists
        let result = find_valid_focus_target(&clients, "0x1", "0x999");
        assert!(result.is_none());
    }
}
