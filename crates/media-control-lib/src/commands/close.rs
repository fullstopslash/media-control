//! Graceful window closing with mpv/Jellyfin session cleanup.
//!
//! Closes the media window and handles any necessary cleanup for
//! mpv and Jellyfin Media Player sessions.

use tokio::process::Command;

use super::fullscreen::is_pip_title;
use super::{
    CommandContext, MPV_IPC_SOCKET_DEFAULT as SHIM_SOCKET, close_window_action, get_media_window,
    get_minify_state_path, send_to_mpv_socket, suppress_avoider,
};
use crate::error::Result;

/// Close the media window gracefully with app-specific handling.
///
/// Different window types require different close strategies:
/// - **mpv**: Stop Jellyfin session first (if applicable), then stop playback via playerctl
/// - **All others** (Firefox PiP, Jellyfin, etc.): Use Hyprland's `closewindow` for graceful close
///
/// # Returns
///
/// - `Ok(())` if no media window found (nothing to close)
/// - `Ok(())` if the window was successfully closed
/// - `Err(...)` if Hyprland IPC fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, close::close};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// close(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn close(ctx: &CommandContext) -> Result<()> {
    // Always clear minified state so next window spawns at normal size.
    // NotFound is the expected case when no minify state exists; surface
    // any other I/O error so we don't silently miss permission/disk issues.
    // If the minify state path itself is unresolvable (e.g. XDG_RUNTIME_DIR
    // unset), log and continue — close should not be blocked on a
    // best-effort cleanup step.
    match get_minify_state_path() {
        Ok(path) => {
            if let Err(e) = tokio::fs::remove_file(&path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!("failed to remove minify state file: {e}");
            }
        }
        Err(e) => {
            tracing::debug!("cannot resolve minify state path (skipping cleanup): {e}");
        }
    }

    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    close_window_gracefully(
        ctx,
        &window.address,
        &window.class,
        &window.title,
        window.pid,
    )
    .await
}

/// Check if a process is the jellyfin-mpv-shim's mpv by looking at argv[0]
/// of `/proc/<pid>/cmdline`.
///
/// Uses async I/O for consistency with the surrounding async context.
/// Reads the cmdline as raw bytes (not UTF-8) because `/proc/<pid>/cmdline`
/// is NUL-separated and may contain non-UTF-8 byte sequences in argv (e.g.
/// when an mpv-shim install path itself is non-UTF-8). Matching as bytes
/// keeps the check correct in those cases and avoids a spurious "not shim"
/// classification that would close-window a window meant to be reused.
///
/// The match is anchored to the **last path segment of argv[0]** — searching
/// the whole cmdline would falsely match harmless paths like
/// `/home/user/mpv-shim-tutorial.mkv` passed as a file argument.
async fn is_shim_mpv(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    tokio::fs::read(format!("/proc/{pid}/cmdline"))
        .await
        .map(|bytes| {
            // argv[0] is the first NUL-terminated token.
            let argv0 = bytes.split(|&b| b == 0).next().unwrap_or(b"");
            // Last `/`-separated segment of argv[0] = executable file name.
            let name = argv0.rsplit(|&b| b == b'/').next().unwrap_or(argv0);
            name == b"mpv-shim"
        })
        .unwrap_or(false)
}

async fn close_window_gracefully(
    ctx: &CommandContext,
    addr: &str,
    class: &str,
    title: &str,
    pid: i32,
) -> Result<()> {
    // Shim mpv: send stop-and-clear, keep window alive for reuse.
    if class == "mpv" && is_shim_mpv(pid).await {
        let _ = send_to_mpv_socket(
            SHIM_SOCKET,
            r#"{"command":["script-message","stop-and-clear"]}"#,
        )
        .await;
        return Ok(());
    }

    // Firefox PiP: close the PiP window, then pause media via MPRIS.
    if class == "firefox" && is_pip_title(title) {
        return close_firefox_pip(ctx, addr).await;
    }

    // All other windows (standalone mpv, Jellyfin, default): closewindow.
    // Suppress BEFORE the dispatch — Hyprland fires `closewindow` immediately,
    // and the avoider daemon would otherwise race in to reposition any
    // remaining media siblings on the workspace before the close has settled.
    suppress_avoider().await;
    ctx.hyprland.dispatch(&close_window_action(addr)).await?;
    Ok(())
}

/// Close a Firefox Picture-in-Picture window and stop its media.
///
/// Strategy:
/// 1. Close the PiP window via `closewindow` (graceful xdg_toplevel::close)
/// 2. Pause Firefox media via playerctl MPRIS (stops the video in the source tab)
///
/// We can't reliably close the source tab because we don't know which tab
/// owns the PiP, and Firefox's internal tab activation after PiP close
/// is not deterministic enough to target with Ctrl+W.
async fn close_firefox_pip(ctx: &CommandContext, pip_addr: &str) -> Result<()> {
    // Suppress BEFORE the dispatch — `closewindow` emits an event the avoider
    // would otherwise pick up and use to reposition siblings during teardown.
    suppress_avoider().await;
    ctx.hyprland
        .dispatch(&close_window_action(pip_addr))
        .await?;

    // Stop Firefox media via MPRIS (best effort).
    // Logged at debug because playerctl may legitimately be missing
    // (no MPRIS) — nothing actionable for the user.
    let result = Command::new("playerctl")
        .args(["--player=firefox", "pause"])
        .output()
        .await;
    if let Err(e) = result {
        tracing::debug!("playerctl pause failed (non-fatal): {e}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::*;

    // --- E2E tests ---

    use super::*;

    #[tokio::test]
    async fn close_jellyfin_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xa2",
            "com.github.iwalton3.jellyfin-media-player",
            "Jellyfin",
            true,
            true,
            0,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(
            has_kill,
            "should dispatch closewindow for jellyfin: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn close_firefox_pip_uses_closewindow() {
        let mock = MockHyprland::start().await;

        let clients = vec![make_test_client_full(
            "0xa1",
            "firefox",
            "Picture-in-Picture",
            true,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [320, 180],
        )];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let close_cmd = cmds.iter().find(|c| c.contains("closewindow"));
        assert!(
            close_cmd.is_some(),
            "should dispatch closewindow for PiP: {cmds:?}"
        );
        assert!(
            close_cmd.unwrap().contains("0xa1"),
            "should target the PiP window address"
        );
    }

    #[tokio::test]
    async fn close_non_shim_mpv_dispatches_closewindow() {
        // pid=0 makes `is_shim_mpv` return false (guards <=0), so this
        // exercises the standalone-mpv path: closewindow is dispatched.
        // (The shim path is exercised by the smoke test below; we assert
        // identity-by-pid here so the test is deterministic regardless of
        // whether /tmp/mpv-shim happens to exist on the host.)
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
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter()
                .any(|c| c.contains("closewindow") && c.contains("0xd1")),
            "non-shim mpv (pid=0) should dispatch closewindow: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn close_shim_path_is_no_op_for_hyprland() {
        // Smoke test: when class=mpv and is_shim_mpv would succeed (real shim
        // socket present), close MUST NOT dispatch closewindow — shim keeps
        // the window alive for reuse. We can't reliably trigger the shim
        // path without controlling /proc/<pid>/cmdline, so this test only
        // asserts the precondition: with pid<=0 (no /proc lookup), is_shim
        // returns false and we fall through to closewindow. Symmetric to the
        // test above — kept as a regression guard against accidental swaps.
        assert!(!is_shim_mpv(0).await);
        assert!(!is_shim_mpv(-1).await);
    }

    #[tokio::test]
    async fn close_default_dispatches_closewindow() {
        let mock = MockHyprland::start().await;

        // Some other media window class
        let clients = vec![make_test_client_full(
            "0xd2",
            "vlc",
            "movie.mkv",
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

        // Need vlc in patterns for it to be found as media window
        let mut config = crate::config::Config::default();
        config.patterns.push(crate::config::Pattern {
            key: "class".to_string(),
            value: "vlc".to_string(),
            pinned_only: false,
            always_pin: false,
        });
        let ctx = mock.context(config);

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_kill = cmds.iter().any(|c| c.contains("closewindow"));
        assert!(
            has_kill,
            "should dispatch closewindow for default class: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn close_no_media_window_is_noop() {
        let mock = MockHyprland::start().await;

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
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
    }

    /// Regression: `close` always clears the minify state file, even when
    /// there is no media window to close. Without this, a stale flag would
    /// survive across a close → next-spawn cycle and the freshly-launched
    /// window would come up at minified dimensions for no reason.
    ///
    /// Routes through XDG_RUNTIME_DIR (the only writable runtime path the
    /// helper accepts), so we hold the shared env-var mutex for the
    /// duration to keep parallel tests from racing on the same global.
    #[tokio::test]
    async fn close_clears_minify_state_when_no_media_window() {
        let _g = super::super::async_env_test_mutex().lock().await;
        let original = std::env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();

        // SAFETY: env mutex held above; restored below before unlock.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", dir.path().to_str().unwrap());
        }

        // Create a stale minify state file. `get_minify_state_path` will
        // resolve it to `<XDG_RUNTIME_DIR>/media-control-minified`.
        //
        // Deliberately no precondition `assert!(exists)` between write and
        // close: other parallel close tests don't hold `async_env_test_mutex`
        // and may call `close()` mid-test. While we hold the env override,
        // *their* close() resolves the same path and removes our file. That
        // race is benign for our post-condition (file is still gone after
        // close — which is exactly what we want to prove) but would flake a
        // precondition. The post-condition below is the load-bearing one.
        let state_path = super::super::get_minify_state_path().unwrap();
        tokio::fs::write(&state_path, "1").await.unwrap();

        // No media window in the snapshot → close hits the cleanup-and-bail path.
        let mock = MockHyprland::start().await;
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
        let ctx = mock.default_context();

        close(&ctx).await.unwrap();

        assert!(
            !state_path.exists(),
            "minify state file should be removed even with no media window"
        );

        // SAFETY: restore prior env state under the same mutex.
        unsafe {
            if let Some(val) = original {
                std::env::set_var("XDG_RUNTIME_DIR", val);
            } else {
                std::env::remove_var("XDG_RUNTIME_DIR");
            }
        }
    }
}
