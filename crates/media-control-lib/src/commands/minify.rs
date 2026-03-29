//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{effective_dimensions, get_media_window, suppress_avoider, toggle_minified, CommandContext};
use crate::error::Result;

/// Toggle minified mode and immediately resize the media window.
pub async fn minify(ctx: &CommandContext) -> Result<()> {
    let now_minified = toggle_minified().await?;

    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    // Don't resize fullscreen windows
    if window.fullscreen != 0 {
        return Ok(());
    }

    let (w, h) = effective_dimensions(ctx);

    ctx.hyprland
        .batch(&[&format!(
            "dispatch resizewindowpixel exact {w} {h},address:{}",
            window.address
        )])
        .await?;

    suppress_avoider().await.ok();

    tracing::debug!(
        "minify: {} ({}x{})",
        if now_minified { "minified" } else { "restored" },
        w,
        h
    );

    Ok(())
}
