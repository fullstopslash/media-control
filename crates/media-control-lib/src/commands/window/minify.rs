//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use std::io;

use tokio::fs;

use super::{
    CommandContext, get_media_window, get_minify_state_path, is_minified,
    reposition_to_default_with_minified,
};
use crate::error::Result;

/// Atomically transition the on-disk minified flag to `target`.
///
/// Returns `Ok(true)` when *this* call performed the transition, `Ok(false)`
/// when another caller (or a prior run) had already left the file in the
/// desired state. The "no-op because already-correct" outcome is intentional:
/// it lets two racing `minify` invocations both observe `was_minified=false`,
/// both dispatch the minified geometry, and still converge on a consistent
/// flag without one of them deleting the other's marker.
///
/// # Atomicity
///
/// - `target = true`: `OpenOptions::create_new(true)` issues `O_CREAT | O_EXCL`
///   in a single syscall — kernel-enforced atomicity. `AlreadyExists` means
///   another writer beat us to the create.
/// - `target = false`: `remove_file` is a single `unlink` syscall. `NotFound`
///   means another writer beat us to the delete.
///
/// All other I/O errors propagate so callers (and operators reading the log)
/// can distinguish "harmless race" from "permission denied / disk full".
async fn try_set_minified(target: bool) -> Result<bool> {
    let path = get_minify_state_path()?;
    if target {
        // O_CREAT | O_EXCL: atomic check-and-create. The kernel guarantees
        // that exactly one concurrent caller observes Ok(_); the rest see
        // AlreadyExists. We don't need (or want) to write any payload — the
        // file's *presence* is the signal.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    } else {
        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

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
///
/// # Concurrency
///
/// Two simultaneous invocations can both observe `was_minified=false`, both
/// dispatch the minified geometry (idempotent — same target coordinates),
/// then race to create the marker file. The atomic `try_set_minified`
/// helper guarantees exactly one wins; the loser logs a warn and returns
/// `Ok(())` rather than rolling back the (already-applied) dispatch. The
/// previous non-atomic `is_minified() → toggle_minified()` pair allowed
/// the loser to *remove* the winner's marker, leaving the flag out of
/// sync with the on-screen geometry.
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

    // Dispatch succeeded — now safe to flip persistent state. Use the
    // atomic helper so a concurrent `minify` invocation that already
    // performed the same transition is treated as a successful no-op
    // rather than an error. Other I/O failures (read-only $XDG_RUNTIME_DIR,
    // permission denied) still propagate — those are real bugs, not races.
    let we_flipped = try_set_minified(target_minified).await?;

    if we_flipped {
        tracing::debug!(
            "minify: {}",
            if target_minified {
                "minified"
            } else {
                "restored"
            },
        );
    } else {
        // Another caller already brought the flag to the desired state
        // between our `is_minified()` read and our `try_set_minified` call.
        // The dispatch we issued is still correct (same geometry as the
        // racing caller), so this is benign — but worth logging at warn
        // so operators see the contention if it becomes frequent.
        tracing::warn!(
            "minify: flag already at target={target_minified} (concurrent toggle); dispatch applied"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    /// RAII guard that pins `XDG_RUNTIME_DIR` to a per-test tempdir while
    /// holding the shared async env-mutex. Drop order restores the prior
    /// env value before releasing the mutex, so parallel tests never
    /// observe a partially-restored state.
    struct EnvGuard {
        // Keep the mutex guard alive for the whole scope. Tokio's
        // MutexGuard is not Send across await points in some cases, but
        // we only hold it across synchronous env mutations and file I/O
        // inside a single test — never handing it to another task.
        _g: tokio::sync::MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
        original: Option<String>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: env-mutex held until self drops, so no parallel
            // tests can observe the partially-restored state.
            unsafe {
                if let Some(v) = self.original.take() {
                    std::env::set_var("XDG_RUNTIME_DIR", v);
                } else {
                    std::env::remove_var("XDG_RUNTIME_DIR");
                }
            }
        }
    }

    async fn isolated_runtime_dir() -> EnvGuard {
        let g = crate::commands::shared::async_env_test_mutex().lock().await;
        let original = std::env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: env-mutex held by `g` for the lifetime of EnvGuard.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", dir.path().to_str().unwrap());
        }
        EnvGuard {
            _g: g,
            _dir: dir,
            original,
        }
    }

    /// `try_set_minified(true)` is atomic: a second call targeting the
    /// same state observes AlreadyExists and returns `Ok(false)` without
    /// clobbering the marker.
    #[tokio::test]
    async fn try_set_minified_true_is_atomic() {
        let _env = isolated_runtime_dir().await;

        // First call wins.
        assert!(try_set_minified(true).await.unwrap());
        assert!(is_minified(), "marker should exist after first set");

        // Second call is a no-op (file already there).
        assert!(!try_set_minified(true).await.unwrap());
        assert!(is_minified(), "marker must still exist after no-op");
    }

    /// `try_set_minified(false)` is atomic: a second call after the first
    /// removed the marker observes NotFound and returns `Ok(false)`.
    #[tokio::test]
    async fn try_set_minified_false_is_atomic() {
        let _env = isolated_runtime_dir().await;

        // Seed the marker so we can clear it.
        try_set_minified(true).await.unwrap();
        assert!(is_minified());

        // First clear wins.
        assert!(try_set_minified(false).await.unwrap());
        assert!(!is_minified(), "marker should be gone after clear");

        // Second clear is a no-op (file already gone).
        assert!(!try_set_minified(false).await.unwrap());
        assert!(!is_minified(), "marker must remain gone after no-op");
    }

    /// Fullscreen media windows are excluded from minify — pressing the
    /// keybind while watching fullscreen video must not resize the window
    /// or flip the on-disk flag.
    #[tokio::test]
    async fn minify_noop_when_fullscreen() {
        let _env = isolated_runtime_dir().await;
        let mock = MockHyprland::start().await;

        // mpv, fullscreen=1
        let clients = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            true,
            true,
            1, // fullscreen
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config_no_suppress());
        minify(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // No resize or move dispatch should have hit the wire.
        assert!(
            !cmds.iter().any(|c| c.contains("resizewindowpixel")),
            "fullscreen window must not be resized: {cmds:?}"
        );
        assert!(
            !cmds.iter().any(|c| c.contains("movewindowpixel")),
            "fullscreen window must not be moved: {cmds:?}"
        );
        // Flag must remain unchanged (false → still false).
        assert!(!is_minified(), "flag must not flip on fullscreen no-op");
    }

    /// With no media-window match in the client list, minify is a silent
    /// no-op — same convention as close/fullscreen.
    #[tokio::test]
    async fn minify_noop_when_no_media_window() {
        let _env = isolated_runtime_dir().await;
        let mock = MockHyprland::start().await;

        // Only a non-media window present.
        let clients = vec![make_test_client_full(
            "0xb1",
            "firefox",
            "Browser",
            false,
            false,
            0,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config_no_suppress());
        minify(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert!(
            !cmds.iter().any(|c| c.contains("resizewindowpixel")),
            "no-media should not dispatch resize: {cmds:?}"
        );
        assert!(!is_minified(), "flag must not flip when no media window");
    }

    /// Happy path: starting unminified, a single `minify` call dispatches
    /// the resize + move and creates the marker file.
    #[tokio::test]
    async fn minify_toggles_on_from_clean_state() {
        let _env = isolated_runtime_dir().await;
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config_no_suppress());
        assert!(!is_minified(), "precondition: clean state");

        minify(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds
            .iter()
            .find(|c| c.contains("resizewindowpixel") && c.contains("movewindowpixel"))
            .unwrap_or_else(|| panic!("expected batched resize+move: {cmds:?}"));
        assert!(
            batch.contains("0xd1"),
            "dispatch must target media window: {batch}"
        );
        assert!(
            is_minified(),
            "marker file must exist after successful toggle-on"
        );
    }

    /// Toggling off after a previous toggle-on returns the flag to the
    /// non-minified state and still issues a reposition dispatch.
    #[tokio::test]
    async fn minify_toggles_off_after_on() {
        let _env = isolated_runtime_dir().await;
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config_no_suppress());

        // First press: minified.
        minify(&ctx).await.unwrap();
        assert!(is_minified(), "first press should set marker");
        mock.clear_commands().await;

        // Second press: restored. The fresh `j/clients` snapshot still
        // reports fullscreen=0, so we exercise the toggle path rather
        // than the fullscreen-guard bail.
        minify(&ctx).await.unwrap();
        assert!(!is_minified(), "second press should clear marker");

        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter()
                .any(|c| c.contains("resizewindowpixel") && c.contains("movewindowpixel")),
            "second press should still dispatch a reposition: {cmds:?}"
        );
    }

    /// Regression for the "dispatch FIRST, flag SECOND" ordering: when
    /// the dispatch fails (mock returns a non-"ok" response), the marker
    /// file MUST NOT be created. Otherwise a failed toggle would leave
    /// the on-disk state out of sync with the actual window geometry.
    #[tokio::test]
    async fn minify_dispatch_failure_leaves_flag_unchanged() {
        let _env = isolated_runtime_dir().await;
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        // Force the batched dispatch to fail by returning a non-"ok" body.
        // The mock's prefix-search matches `[[BATCH]]` against the outgoing
        // `[[BATCH]]dispatch …` envelope that `dispatch_batch` builds.
        mock.set_response("[[BATCH]]", "error: simulated dispatch failure")
            .await;

        let ctx = mock.context(test_config_no_suppress());
        assert!(!is_minified(), "precondition: clean state");

        let result = minify(&ctx).await;
        assert!(
            result.is_err(),
            "dispatch failure must propagate as Err, got: {result:?}"
        );
        assert!(
            !is_minified(),
            "marker must NOT be created when dispatch fails — \
             this is the load-bearing 'dispatch first, flag second' invariant"
        );
    }
}
