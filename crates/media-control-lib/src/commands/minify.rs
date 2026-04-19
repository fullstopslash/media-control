//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{CommandContext, get_media_window, reposition_to_default, toggle_minified};
use crate::error::Result;

/// Toggle minified mode, resize, and reposition the media window.
///
/// Performs the actionability checks (media window present, not fullscreen)
/// BEFORE flipping the persistent minified-state flag. Otherwise a no-op
/// invocation (no window, or fullscreen) would silently leave the on-disk
/// state out of sync with what's actually on screen.
pub async fn minify(ctx: &CommandContext) -> Result<()> {
    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    if window.fullscreen > 0 {
        return Ok(());
    }

    // Now safe to flip persistent state — we know the reposition will follow.
    let now_minified = toggle_minified().await?;

    // `reposition_to_default` self-suppresses immediately before its dispatch,
    // so we don't need a redundant suppress here — the contract is documented
    // in commands/mod.rs::reposition_to_default.
    reposition_to_default(ctx, &window.address).await?;

    tracing::debug!(
        "minify: {}",
        if now_minified { "minified" } else { "restored" },
    );

    Ok(())
}
