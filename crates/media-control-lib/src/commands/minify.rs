//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{
    CommandContext, get_media_window, is_minified, reposition_to_default_with_minified,
    toggle_minified,
};
use crate::error::Result;

/// Toggle minified mode, resize, and reposition the media window.
///
/// Order of operations is load-bearing: dispatch the move/resize FIRST and
/// only flip the persistent minified-state flag once the dispatch succeeds.
/// If we flipped the flag first and the dispatch then failed (window gone,
/// Hyprland down, IPC error) the on-disk state would desync — the user sees
/// no change on screen but the flag is flipped, requiring two more presses
/// to recover. By inverting the order we keep the flag in lockstep with the
/// last successfully-applied geometry: a failed dispatch propagates the
/// error without the state being partially committed.
pub async fn minify(ctx: &CommandContext) -> Result<()> {
    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    if window.fullscreen > 0 {
        return Ok(());
    }

    // Compute the *target* minified state (the inverse of current). The
    // reposition runs against this target geometry so the move lands the
    // window where the user expects after the toggle. The on-disk flag is
    // only flipped once the dispatch returns Ok.
    let was_minified = is_minified();
    let target_minified = !was_minified;

    // `reposition_to_default_with_minified` self-suppresses immediately
    // before its dispatch, so we don't need a redundant suppress here — the
    // contract is documented in commands/mod.rs::reposition_to_default.
    reposition_to_default_with_minified(ctx, &window.address, target_minified).await?;

    // Dispatch succeeded — now safe to flip persistent state. If
    // `toggle_minified` itself fails (e.g. read-only $XDG_RUNTIME_DIR), the
    // window is in the new geometry but the flag still reflects the old
    // state; we propagate the error so the caller sees the inconsistency.
    let now_minified = toggle_minified().await?;
    debug_assert_eq!(now_minified, target_minified);

    tracing::debug!(
        "minify: {}",
        if now_minified { "minified" } else { "restored" },
    );

    Ok(())
}
