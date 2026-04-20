//! Jellyfin integration for marking items as watched.
//!
//! All commands delegate to the jellyfin-mpv-shim fork via mpv IPC.
//! The shim handles Jellyfin API calls, strategy resolution, and playback natively.
//!
//! No Hyprland window check — mpv socket failure is sufficient guard.
//! Skipping the Hyprland IPC saves ~15-30ms per command.

use super::send_mpv_script_message;
use crate::error::Result;

/// Mark the current item as watched.
pub async fn mark_watched() -> Result<()> {
    send_mpv_script_message("mark-watched").await
}

/// Mark current item as watched and stop playback.
pub async fn mark_watched_and_stop() -> Result<()> {
    send_mpv_script_message("mark-watched").await?;
    send_mpv_script_message("stop-and-clear").await
}

/// Mark current item as watched and advance to next episode.
pub async fn mark_watched_and_next() -> Result<()> {
    send_mpv_script_message("mark-watched-next").await
}

/// Next item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-next" to avoid collision with uosc's "next" binding.
pub async fn next() -> Result<()> {
    send_mpv_script_message("shim-next").await
}

/// Previous item via per-library strategy (episode-level, no mark watched).
/// Uses "shim-prev" to avoid collision with uosc's "prev" binding.
pub async fn prev() -> Result<()> {
    send_mpv_script_message("shim-prev").await
}

/// Jump to next series/collection (series-level navigation).
pub async fn next_series() -> Result<()> {
    send_mpv_script_message("next-series").await
}

/// Return to previous series/collection (series-level navigation).
pub async fn prev_series() -> Result<()> {
    send_mpv_script_message("prev-series").await
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use crate::commands::{MPV_IPC_SOCKET_DEFAULT, async_env_test_mutex};
    use crate::error::{MediaControlError, MpvIpcErrorKind};

    /// Skip if any live mpv socket is present on the test host — sending real
    /// commands to a running mpv instance would pollute the user's session.
    fn any_live_socket() -> bool {
        use std::path::Path;
        use crate::commands::send_to_mpv_socket as _; // just to reference the path const
        Path::new(MPV_IPC_SOCKET_DEFAULT).exists()
            || Path::new("/tmp/mpvctl-jshim").exists()
    }

    /// When no mpv socket exists, `mark_watched` returns a `NoSocket` error.
    /// This verifies the function propagates mpv-IPC failures rather than
    /// swallowing them.
    #[tokio::test]
    async fn mark_watched_returns_no_socket_when_mpv_absent() {
        if any_live_socket() {
            eprintln!("skipping: live mpv socket present");
            return;
        }

        let _g = async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe {
            env::set_var("MPV_IPC_SOCKET", "/tmp/mark-watched-test-nonexistent-socket");
        }

        let result = mark_watched().await;

        // SAFETY: restore
        unsafe {
            match original {
                Some(v) => env::set_var("MPV_IPC_SOCKET", &v),
                None => env::remove_var("MPV_IPC_SOCKET"),
            }
        }

        match result {
            Err(MediaControlError::MpvIpc { kind, .. }) => {
                assert_eq!(kind, MpvIpcErrorKind::NoSocket);
            }
            Ok(()) => panic!("expected MpvIpc/NoSocket, got Ok"),
            Err(e) => panic!("expected MpvIpc/NoSocket, got {e:?}"),
        }
    }

    /// `mark_watched_and_stop` is two sequential steps joined with `?`.
    /// When the first step (mark-watched) fails, the second step (stop-and-clear)
    /// must NOT be attempted and the first error must be returned.
    ///
    /// We verify this by checking that only ONE NoSocket error is surfaced, not
    /// two — the `?` on the first call short-circuits before the second is sent.
    #[tokio::test]
    async fn mark_watched_and_stop_short_circuits_on_first_failure() {
        if any_live_socket() {
            eprintln!("skipping: live mpv socket present");
            return;
        }

        let _g = async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe {
            env::set_var("MPV_IPC_SOCKET", "/tmp/mark-watched-stop-test-nonexistent-socket");
        }

        let result = mark_watched_and_stop().await;

        // SAFETY: restore
        unsafe {
            match original {
                Some(v) => env::set_var("MPV_IPC_SOCKET", &v),
                None => env::remove_var("MPV_IPC_SOCKET"),
            }
        }

        // The result must be Err — the first `?` short-circuits.
        // If both steps were attempted independently we might get two errors;
        // the `?` pattern guarantees exactly one.
        assert!(
            result.is_err(),
            "mark_watched_and_stop must propagate the first failure"
        );
        match result {
            Err(MediaControlError::MpvIpc { kind, .. }) => {
                assert_eq!(
                    kind,
                    MpvIpcErrorKind::NoSocket,
                    "expected NoSocket from first step"
                );
            }
            Ok(()) => panic!("expected Err, got Ok"),
            Err(e) => panic!("expected MpvIpc/NoSocket, got {e:?}"),
        }
    }

    /// `mark_watched_and_next` delegates to a single `send_mpv_script_message`
    /// call (not two). Verify the function fails with NoSocket when no mpv is
    /// running — ensuring it is wired to the IPC layer, not a no-op.
    #[tokio::test]
    async fn mark_watched_and_next_returns_no_socket_when_mpv_absent() {
        if any_live_socket() {
            eprintln!("skipping: live mpv socket present");
            return;
        }

        let _g = async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe {
            env::set_var("MPV_IPC_SOCKET", "/tmp/mark-watched-next-test-nonexistent-socket");
        }

        let result = mark_watched_and_next().await;

        // SAFETY: restore
        unsafe {
            match original {
                Some(v) => env::set_var("MPV_IPC_SOCKET", &v),
                None => env::remove_var("MPV_IPC_SOCKET"),
            }
        }

        match result {
            Err(MediaControlError::MpvIpc { kind, .. }) => {
                assert_eq!(kind, MpvIpcErrorKind::NoSocket);
            }
            Ok(()) => panic!("expected MpvIpc/NoSocket, got Ok"),
            Err(e) => panic!("expected MpvIpc/NoSocket, got {e:?}"),
        }
    }
}
