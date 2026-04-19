//! Random subcommand — trigger random playback via mpv-shim IPC.
//!
//! Sends a `script-message random [type]` to the shim, which delegates
//! to the active store's `random_item()` implementation.
//!
//! Types are store-specific:
//! - Jellyfin: `show`, `series`, `movie`
//! - Twitch: (any or none — picks random live channel)
//! - Stash: `scene`, `performer`, `studio`

use super::{send_mpv_script_message, send_mpv_script_message_with_args};

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
/// - Returns `mpv_no_socket` if no mpv IPC socket is available.
/// - Returns an `mpv_connection_failed` error if `random_type` exceeds
///   `RANDOM_TYPE_MAX_LEN`.
pub async fn random(random_type: Option<&str>) -> crate::error::Result<()> {
    match random_type {
        Some(t) => {
            if t.len() > RANDOM_TYPE_MAX_LEN {
                return Err(crate::error::MediaControlError::mpv_connection_failed(
                    format!(
                        "random type too long: {} bytes (max {RANDOM_TYPE_MAX_LEN})",
                        t.len()
                    ),
                ));
            }
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
        use crate::error::{MediaControlError, MpvIpcErrorKind};
        let huge = "x".repeat(RANDOM_TYPE_MAX_LEN + 1);
        let err = random(Some(&huge)).await.expect_err("must reject");
        match err {
            MediaControlError::MpvIpc { kind, message } => {
                assert_eq!(kind, MpvIpcErrorKind::ConnectionFailed);
                assert!(
                    message.contains("too long"),
                    "message should mention overflow: {message}"
                );
            }
            other => panic!("expected MpvIpc length-check error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn random_accepts_max_len_type() {
        use crate::error::{MediaControlError, MpvIpcErrorKind};
        // Exactly at the limit: parse must accept; outcome depends on mpv
        // socket availability, but the length check itself must not fire.
        let max = "x".repeat(RANDOM_TYPE_MAX_LEN);
        if let Err(MediaControlError::MpvIpc { kind, message }) = random(Some(&max)).await
            && kind == MpvIpcErrorKind::ConnectionFailed
        {
            assert!(
                !message.contains("too long"),
                "length-check should not fire at the boundary: {message}"
            );
        }
    }
}
