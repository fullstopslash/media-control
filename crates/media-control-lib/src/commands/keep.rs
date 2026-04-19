//! Tag the currently-playing item as "keep" to prevent auto-deletion.
//!
//! Sends `script-message keep` to the shim via the standard IPC path.
//! The Rust shim routes the command to the active store plugin internally.
//!
//! No Hyprland window check — mpv socket failure is sufficient guard.
//!
//! # Errors (module-wide)
//!
//! Every public function in this module delegates to
//! [`super::send_mpv_script_message`] and so returns
//! [`crate::error::MediaControlError::MpvIpc`] (kind `NoSocket`) when no mpv
//! IPC socket is available, or kind `ConnectionFailed` when all candidate
//! socket paths reject the connection.

use super::send_mpv_script_message;
use crate::error::Result;

/// Tag the current item as "keep".
///
/// # Errors
///
/// See module-level docs.
pub async fn keep() -> Result<()> {
    send_mpv_script_message("keep").await
}

/// Toggle favorite on the current item.
///
/// # Errors
///
/// See module-level docs.
pub async fn favorite() -> Result<()> {
    send_mpv_script_message("favorite").await
}

/// Delete the current item.
///
/// # Errors
///
/// See module-level docs.
pub async fn delete() -> Result<()> {
    send_mpv_script_message("delete").await
}

/// Increment o-counter on the current item.
///
/// # Errors
///
/// See module-level docs.
pub async fn add_o() -> Result<()> {
    send_mpv_script_message("add-o").await
}
