//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{effective_dimensions, get_media_window, resolve_effective_position, suppress_avoider, toggle_minified, CommandContext};
use crate::error::Result;

/// Toggle minified mode, resize, and reposition the media window.
pub async fn minify(ctx: &CommandContext) -> Result<()> {
    let now_minified = toggle_minified().await?;

    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    if window.fullscreen != 0 {
        return Ok(());
    }

    let (w, h) = effective_dimensions(ctx);

    // Reposition to default corner for the new size
    let positioning = &ctx.config.positioning;
    let target_x = resolve_effective_position(ctx, &positioning.default_x)
        .unwrap_or(ctx.config.positions.x_right);
    let target_y = resolve_effective_position(ctx, &positioning.default_y)
        .unwrap_or(ctx.config.positions.y_bottom);

    ctx.hyprland
        .batch(&[
            &format!("dispatch resizewindowpixel exact {w} {h},address:{}", window.address),
            &format!("dispatch movewindowpixel exact {target_x} {target_y},address:{}", window.address),
        ])
        .await?;

    suppress_avoider().await.ok();

    tracing::debug!(
        "minify: {} ({}x{} at {},{}))",
        if now_minified { "minified" } else { "restored" },
        w, h, target_x, target_y
    );

    Ok(())
}
