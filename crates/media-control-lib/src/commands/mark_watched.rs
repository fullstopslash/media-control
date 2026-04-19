//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.
//!
//! No Hyprland window check — mpv socket failure is sufficient guard.
//! Skipping the Hyprland IPC saves ~15-30ms per command.
//!
//! # Errors (module-wide)
//!
//! Every public function in this module ultimately calls
//! [`super::send_mpv_script_message`] and so returns
//! [`crate::error::MediaControlError::MpvIpc`] (kind `NoSocket`) when no mpv
//! IPC socket is available.
//!
//! The two-step variant [`mark_watched_and_stop`] sends `mark-watched` and
//! then `stop-and-clear` as separate IPC calls. If the first call fails it
//! propagates immediately. If the second call fails after the first
//! succeeded, the failure is logged with a `warn!` so operators know the
//! item was in fact marked watched even though the caller saw an error.
//!
//! The remaining variants ([`mark_watched`], [`mark_watched_and_next`],
//! [`next`], [`prev`], [`next_series`], [`prev_series`]) are *single*
//! script-message calls — the shim performs any multi-step orchestration on
//! the server side, so there is no partial-success window to worry about.

use super::send_mpv_script_message;
use crate::error::Result;

/// Mark the current item as watched.
///
/// # Errors
///
/// See module-level docs.
pub async fn mark_watched() -> Result<()> {
    send_mpv_script_message("mark-watched").await
}

/// Mark current item as watched and stop playback.
///
/// Two separate IPC calls: `mark-watched` followed by `stop-and-clear`.
/// If `mark-watched` fails the `stop-and-clear` step is skipped (fail-fast).
/// If `mark-watched` succeeds but `stop-and-clear` fails — typically because
/// mpv exited between the two calls — we still return the error to the
/// caller, but emit a `warn!` first so the partial success is visible in
/// the log. Without it, the item would have been silently marked watched
/// while the caller only sees a generic IPC failure.
///
/// # Errors
///
/// See module-level docs.
pub async fn mark_watched_and_stop() -> Result<()> {
    send_mpv_script_message("mark-watched").await?;
    if let Err(e) = send_mpv_script_message("stop-and-clear").await {
        tracing::warn!(
            "mark-watched succeeded but stop-and-clear failed (mpv may have closed): {e}"
        );
        return Err(e);
    }
    Ok(())
}

/// Mark current item as watched and advance to next episode.
///
/// # Errors
///
/// See module-level docs.
pub async fn mark_watched_and_next() -> Result<()> {
    send_mpv_script_message("mark-watched-next").await
}

/// Next item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-next" to avoid collision with uosc's "next" binding.
///
/// # Errors
///
/// See module-level docs.
pub async fn next() -> Result<()> {
    send_mpv_script_message("shim-next").await
}

/// Previous item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-prev" to avoid collision with uosc's "prev" binding.
///
/// # Errors
///
/// See module-level docs.
pub async fn prev() -> Result<()> {
    send_mpv_script_message("shim-prev").await
}

/// Jump to next series/collection (series-level navigation).
///
/// # Errors
///
/// See module-level docs.
pub async fn next_series() -> Result<()> {
    send_mpv_script_message("next-series").await
}

/// Return to previous series/collection (series-level navigation).
///
/// # Errors
///
/// See module-level docs.
pub async fn prev_series() -> Result<()> {
    send_mpv_script_message("prev-series").await
}
