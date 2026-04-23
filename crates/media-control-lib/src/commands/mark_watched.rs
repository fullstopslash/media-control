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

#[cfg(test)]
mod tests {
    //! Tests cover the four production entry points whose behaviour is
    //! distinguishable at the IPC layer:
    //!
    //! 1. `mark_watched` with no live mpv socket -> returns
    //!    `MediaControlError::MpvIpc { kind: NoSocket, .. }`.
    //! 2. `mark_watched_and_stop` partial failure (mark ok, stop fails) ->
    //!    returns the second-call error so callers see the failure even
    //!    though the watched flag was set server-side.
    //! 3. `mark_watched_and_next` happy path -> single IPC call, `Ok(())`.
    //! 4. `mark_watched_and_stop` happy path -> both calls land, `Ok(())`.
    //!
    //! All tests that mutate `MPV_IPC_SOCKET` hold the process-wide
    //! [`crate::commands::async_env_test_mutex`] for their full body so
    //! they don't race with the other tests in this crate that touch the
    //! same env var (see `commands/mod.rs`).
    //!
    //! Note on fallback paths: `send_mpv_ipc_command` also tries the
    //! hard-coded `/tmp/mpv-shim` and `/tmp/mpvctl-jshim` sockets after
    //! the env-supplied path. Tests that need the "no socket" outcome
    //! therefore skip themselves when those legacy sockets happen to be
    //! live on the host (developer machines often have one running).
    //! This keeps the tests deterministic on CI and sandbox hosts where
    //! the legacy paths are absent, without flaking locally for someone
    //! who happens to have mpv-shim attached.
    use std::env;
    use std::path::Path;

    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::UnixListener;

    use super::{mark_watched, mark_watched_and_next, mark_watched_and_stop};
    use crate::commands::{MPV_IPC_SOCKET_DEFAULT, async_env_test_mutex};
    use crate::error::{MediaControlError, MpvIpcErrorKind};

    /// Legacy fallback socket path mirrored from [`crate::commands`]
    /// (kept private there); reproduced here so the host-skip check
    /// matches the production candidate list exactly.
    const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl-jshim";

    /// Returns `true` when one of the hard-coded fallback sockets exists
    /// on this host as a live socket. When that happens
    /// `send_mpv_ipc_command` can succeed even after our env-supplied
    /// listener is gone, so any test that asserts a "no socket" outcome
    /// must opt out.
    fn fallback_sockets_present() -> bool {
        use std::os::unix::fs::FileTypeExt;
        for p in [MPV_IPC_SOCKET_DEFAULT, MPV_IPC_SOCKET_FALLBACK] {
            if let Ok(meta) = std::fs::symlink_metadata(Path::new(p))
                && meta.file_type().is_socket()
            {
                return true;
            }
        }
        false
    }

    /// RAII guard that snapshots `MPV_IPC_SOCKET`, lets the test mutate
    /// it, and restores the prior value on drop. Centralised so each
    /// test isn't re-implementing the same save/restore dance under
    /// `unsafe`.
    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = env::var(key).ok();
            // SAFETY: Caller holds `async_env_test_mutex` for the full
            // test body, so no other test thread is reading or writing
            // env vars concurrently.
            unsafe { env::set_var(key, value) };
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see `EnvGuard::set` -- same mutex still held.
            unsafe {
                if let Some(ref val) = self.original {
                    env::set_var(self.key, val);
                } else {
                    env::remove_var(self.key);
                }
            }
        }
    }

    /// Read one full `script-message` payload line off `listener` and
    /// return its first script-message argument (the "verb"). Used by
    /// the happy-path tests to verify the right command was sent.
    async fn accept_one_and_read_verb(listener: &UnixListener) -> String {
        let (stream, _) = listener.accept().await.expect("accept failed");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .expect("read_line failed");
        // Payload shape:
        //   {"command":["script-message","<verb>", ...]}
        // The verb is at command[1]; command[0] is the literal
        // "script-message" prefix added by `send_mpv_script_message`.
        let parsed: serde_json::Value =
            serde_json::from_str(&line).expect("payload is not valid JSON");
        parsed
            .get("command")
            .and_then(|c| c.get(1))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .expect("payload missing script-message verb")
    }

    /// `mark_watched` returns `MpvIpc { NoSocket }` when no listener is
    /// reachable. We point `MPV_IPC_SOCKET` at a tempdir path that
    /// doesn't exist; the production code's `is_unix_socket` check
    /// quietly skips it and falls through to the hard-coded fallbacks,
    /// which also fail (in test environments).
    #[tokio::test]
    async fn mark_watched_no_socket_returns_no_socket_error() {
        let _g = async_env_test_mutex().lock().await;
        if fallback_sockets_present() {
            eprintln!(
                "skipping mark_watched_no_socket_returns_no_socket_error: \
                 {MPV_IPC_SOCKET_DEFAULT} or {MPV_IPC_SOCKET_FALLBACK} is a live \
                 socket on this host, which would let the production fallback \
                 path succeed and invalidate the assertion."
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("does-not-exist");
        let _env = EnvGuard::set("MPV_IPC_SOCKET", bogus.to_str().unwrap());

        let result = mark_watched().await;
        match result {
            Err(MediaControlError::MpvIpc {
                kind: MpvIpcErrorKind::NoSocket,
                ..
            }) => {}
            other => panic!("expected MpvIpc{{NoSocket}}, got {other:?}"),
        }
    }

    /// `mark_watched_and_next` happy path: one connection, one
    /// `mark-watched-next` script-message, `Ok(())` returned.
    #[tokio::test]
    async fn mark_watched_and_next_single_ipc_call_succeeds() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("mpv-mark-next");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let _env = EnvGuard::set("MPV_IPC_SOCKET", socket_path.to_str().unwrap());

        let server = tokio::spawn(async move {
            let verb = accept_one_and_read_verb(&listener).await;
            assert_eq!(verb, "mark-watched-next");
        });

        let result = mark_watched_and_next().await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        server.await.unwrap();
    }

    /// `mark_watched_and_stop` happy path: BOTH calls reach the listener
    /// in order (`mark-watched` then `stop-and-clear`) and the function
    /// returns `Ok(())`.
    #[tokio::test]
    async fn mark_watched_and_stop_happy_path_sends_both_messages() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("mpv-mark-stop-ok");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let _env = EnvGuard::set("MPV_IPC_SOCKET", socket_path.to_str().unwrap());

        let server = tokio::spawn(async move {
            let first = accept_one_and_read_verb(&listener).await;
            assert_eq!(first, "mark-watched");
            let second = accept_one_and_read_verb(&listener).await;
            assert_eq!(second, "stop-and-clear");
        });

        let result = mark_watched_and_stop().await;
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        server.await.unwrap();
    }

    /// Partial-failure path for `mark_watched_and_stop`: the first IPC
    /// call lands, then the listener is dropped. The second call finds
    /// the bind path still on disk (`UnixListener::drop` closes the FD
    /// but does not unlink) so the production `is_unix_socket` gate
    /// passes; the actual `connect` then fails with `ECONNREFUSED`,
    /// exhausts retries, and `send_mpv_ipc_command` returns
    /// `MpvIpc { NoSocket }` -- which is exactly the surface the caller
    /// observes.
    ///
    /// We assert:
    ///   * an error is returned (the partial success doesn't mask the
    ///     stop failure), and
    ///   * the error is `MpvIpc { NoSocket }` so callers can route it
    ///     to the same recovery branch as a fully-cold mpv.
    ///
    /// We deliberately don't assert on the `tracing::warn!` payload
    /// emitted between the two calls -- the value of capturing it is
    /// low and pulling in `tracing-test` (or hand-rolling a
    /// `tracing::Subscriber`) would dwarf the test it gates.
    #[tokio::test]
    async fn mark_watched_and_stop_partial_failure_propagates_stop_error() {
        let _g = async_env_test_mutex().lock().await;
        if fallback_sockets_present() {
            eprintln!(
                "skipping mark_watched_and_stop_partial_failure_propagates_stop_error: \
                 {MPV_IPC_SOCKET_DEFAULT} or {MPV_IPC_SOCKET_FALLBACK} is a live socket \
                 on this host; the dropped listener would silently fail over to it."
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("mpv-mark-stop-partial");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let _env = EnvGuard::set("MPV_IPC_SOCKET", socket_path.to_str().unwrap());

        // Accept exactly one connection, then drop the listener so the
        // second `send_mpv_script_message` call has nothing to talk to.
        let server = tokio::spawn(async move {
            let verb = accept_one_and_read_verb(&listener).await;
            assert_eq!(verb, "mark-watched");
            // Listener dropped here at end of scope.
        });

        let result = mark_watched_and_stop().await;
        server.await.unwrap();

        match result {
            Err(MediaControlError::MpvIpc {
                kind: MpvIpcErrorKind::NoSocket,
                ..
            }) => {}
            other => panic!(
                "expected partial-failure MpvIpc{{NoSocket}} (stop call after \
                 listener dropped), got {other:?}"
            ),
        }
    }
}
