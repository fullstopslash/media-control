//! Toggle pinned floating mode with positioning.
//!
//! Pins or unpins the media window and applies appropriate positioning
//! based on the current state and configuration.

use super::CommandContext;
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
    // Get all clients
    let clients = ctx.hyprland.get_clients().await?;

    // Find the focused window from the clients list itself to avoid race conditions.
    // The focused window is the one with focusHistoryID == 0 (most recently focused).
    let focus_addr = clients
        .iter()
        .filter(|c| c.focus_history_id == 0)
        .map(|c| c.address.as_str())
        .next();

    // Find media window
    let Some(media) = ctx.window_matcher.find_media_window(&clients, focus_addr) else {
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

    // Position to configured default corner
    let pos = &ctx.config.positions;
    let positioning = &ctx.config.positioning;
    let target_x = ctx
        .config
        .resolve_position(&positioning.default_x)
        .unwrap_or(pos.x_right);
    let target_y = ctx
        .config
        .resolve_position(&positioning.default_y)
        .unwrap_or(pos.y_bottom);
    ctx.hyprland
        .batch(&[
            &format!(
                "dispatch resizewindowpixel exact {} {},address:{}",
                pos.width, pos.height, media.address
            ),
            &format!(
                "dispatch movewindowpixel exact {} {},address:{}",
                target_x, target_y, media.address
            ),
        ])
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Integration tests would require mocking HyprlandClient
    // Unit tests for the logic are covered by the window module tests
}
