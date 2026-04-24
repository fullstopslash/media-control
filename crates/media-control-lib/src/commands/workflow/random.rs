//! Random subcommand — trigger random playback via mpv-shim IPC.
//!
//! Sends a `script-message random [type]` to the shim, which delegates
//! to the active store's `random_item()` implementation.
//!
//! Types are store-specific:
//! - Jellyfin: `show`, `series`, `movie`
//! - Twitch: (any or none — picks random live channel)
//! - Stash: `scene`, `performer`, `studio`

use super::{send_mpv_script_message, send_mpv_script_message_with_args, validate_ipc_token_len};

/// Maximum accepted length for a random-type token.
///
/// Real values are short tokens like `show`, `series`, `movie`, `scene`.
/// Cap to defend the IPC path against unbounded CLI input — the value is
/// embedded in a JSON `script-message random <type>` payload.
const RANDOM_TYPE_MAX_LEN: usize = 64;

/// Trigger random playback via mpv-shim IPC.
///
/// If `random_type` is provided, it's passed as an argument to the
/// `random` script-message. The active store interprets the type.
///
/// # Errors
///
/// - Returns [`crate::error::MediaControlError::MpvIpc`] with kind `NoSocket`
///   if no mpv IPC socket is available.
/// - Returns [`crate::error::MediaControlError::InvalidArgument`] if
///   `random_type` exceeds [`RANDOM_TYPE_MAX_LEN`].
pub async fn random(random_type: Option<&str>) -> crate::error::Result<()> {
    match random_type {
        Some(t) => {
            validate_ipc_token_len("random type", t, RANDOM_TYPE_MAX_LEN)?;
            send_mpv_script_message_with_args("random", &[t]).await
        }
        None => send_mpv_script_message("random").await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behavioural verification of the IPC round-trip is covered by the IPC
    /// integration tests in `commands::tests`. Here we lock in the
    /// length-check defense for `random_type`.
    #[tokio::test]
    async fn random_rejects_overlong_type() {
        use crate::error::MediaControlError;
        let huge = "x".repeat(RANDOM_TYPE_MAX_LEN + 1);
        let err = random(Some(&huge)).await.expect_err("must reject");
        // Input validation must surface as InvalidArgument — never as a
        // misleading IPC connection failure.
        match err {
            MediaControlError::InvalidArgument(msg) => {
                assert!(
                    msg.contains("too long"),
                    "message should mention overflow: {msg}"
                );
            }
            other => panic!("expected InvalidArgument length-check error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn random_accepts_max_len_type() {
        use crate::error::MediaControlError;
        // Exactly at the limit: parse must accept; outcome depends on mpv
        // socket availability, but the length check itself must not fire —
        // so we must NOT see InvalidArgument for input at the boundary.
        let max = "x".repeat(RANDOM_TYPE_MAX_LEN);
        if let Err(MediaControlError::InvalidArgument(msg)) = random(Some(&max)).await {
            panic!("length-check should not fire at the boundary: {msg}");
        }
    }

    /// `Some("")` must be rejected: `script-message random ` (trailing
    /// empty arg) would silently no-op on the shim side. The empty-string
    /// guard inside `validate_ipc_token_len` catches this; locked in here
    /// so a regression at that layer is caught by this command's own
    /// test surface.
    #[tokio::test]
    async fn random_rejects_empty_type() {
        use crate::error::MediaControlError;
        let err = random(Some("")).await.expect_err("must reject empty type");
        match err {
            MediaControlError::InvalidArgument(msg) => {
                assert!(
                    msg.contains("empty"),
                    "message should mention emptiness: {msg}"
                );
            }
            other => panic!("expected InvalidArgument empty-check error, got: {other:?}"),
        }
    }
}
