//! Toggle minified mode for the media window.
//!
//! Minified mode scales the media window to a fraction of its normal size
//! (configurable via `positioning.minified_scale`). All positioning and
//! avoidance rules still apply — just with smaller dimensions.

use super::{
    CommandContext, get_media_window, reposition_to_default, suppress_avoider, toggle_minified,
};
use crate::error::Result;

/// Toggle minified mode, resize, and reposition the media window.
pub async fn minify(ctx: &CommandContext) -> Result<()> {
    let now_minified = toggle_minified().await?;

    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    if window.fullscreen > 0 {
        return Ok(());
    }

    // Suppress BEFORE the reposition batch — the movewindow event would
    // otherwise race the daemon and bounce the window back.
    suppress_avoider().await;
    reposition_to_default(ctx, &window.address).await?;

    tracing::debug!(
        "minify: {}",
        if now_minified { "minified" } else { "restored" },
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::super::{
        async_env_test_mutex, get_minify_state_path, is_minified, toggle_minified,
    };

    /// Set `XDG_RUNTIME_DIR` to a real temp directory so `runtime_dir()` accepts it.
    unsafe fn set_xdg(dir: &std::path::Path) {
        // SAFETY: caller holds async_env_test_mutex for the full test body
        unsafe { env::set_var("XDG_RUNTIME_DIR", dir) };
    }

    unsafe fn restore_xdg(original: Option<String>) {
        // SAFETY: caller holds async_env_test_mutex for the full test body
        unsafe {
            match original {
                Some(v) => env::set_var("XDG_RUNTIME_DIR", &v),
                None => env::remove_var("XDG_RUNTIME_DIR"),
            }
        }
    }

    /// `get_minify_state_path` returns a path inside the current XDG_RUNTIME_DIR.
    #[tokio::test]
    async fn minify_state_path_is_under_runtime_dir() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe { set_xdg(dir.path()) };

        let path = get_minify_state_path();
        assert!(
            path.starts_with(dir.path()),
            "minify state path {path:?} must be under XDG_RUNTIME_DIR {:?}",
            dir.path()
        );
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("media-control-minified"),
            "file name must be 'media-control-minified', got {path:?}"
        );

        // SAFETY: held under async_env_test_mutex
        unsafe { restore_xdg(original) };
    }

    /// When the state file does not exist, `is_minified()` returns `false`.
    #[tokio::test]
    async fn minify_default_state_is_off() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe { set_xdg(dir.path()) };

        // Ensure the file doesn't exist in the fresh temp dir
        let path = get_minify_state_path();
        let _ = std::fs::remove_file(&path); // ignore error if absent

        assert!(!is_minified(), "should default to off when state file absent");

        // SAFETY: held under async_env_test_mutex
        unsafe { restore_xdg(original) };
    }

    /// `toggle_minified` creates the state file on the first call (→ on),
    /// then removes it on the second call (→ off).
    #[tokio::test]
    async fn minify_toggle_on_then_off() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe { set_xdg(dir.path()) };

        let path = get_minify_state_path();
        let _ = std::fs::remove_file(&path); // start clean

        // First toggle: off → on
        let now_on = toggle_minified().await.unwrap();
        assert!(now_on, "first toggle must return true (now minified)");
        assert!(path.exists(), "state file must exist after toggling on");
        assert!(is_minified(), "is_minified() must agree after toggling on");

        // Second toggle: on → off
        let now_off = toggle_minified().await.unwrap();
        assert!(!now_off, "second toggle must return false (now restored)");
        assert!(!path.exists(), "state file must be absent after toggling off");
        assert!(!is_minified(), "is_minified() must agree after toggling off");

        // SAFETY: held under async_env_test_mutex
        unsafe { restore_xdg(original) };
    }

    /// `toggle_minified` called three times ends in the on state (odd = on).
    #[tokio::test]
    async fn minify_toggle_three_times_ends_on() {
        let _g = async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: held under async_env_test_mutex
        unsafe { set_xdg(dir.path()) };

        let path = get_minify_state_path();
        let _ = std::fs::remove_file(&path); // start clean

        toggle_minified().await.unwrap(); // on
        toggle_minified().await.unwrap(); // off
        let result = toggle_minified().await.unwrap(); // on

        assert!(result, "third toggle (odd) must return true");
        assert!(is_minified(), "must be minified after odd number of toggles");

        // SAFETY: held under async_env_test_mutex
        unsafe { restore_xdg(original) };
    }
}
