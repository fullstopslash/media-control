//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{CommandContext, get_media_window, reposition_to_default, suppress_avoider, toggle_minified};
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

    reposition_to_default(ctx, &window.address).await?;
    suppress_avoider().await.ok();

    tracing::debug!(
        "minify: {}",
        if now_minified { "minified" } else { "restored" },
    );

    Ok(())
}
