//! Runtime context helpers: XDG_RUNTIME_DIR resolution and avoider suppression.
//!
//! This module provides:
//! - [`runtime_dir`] — sanitised `$XDG_RUNTIME_DIR` (or `/tmp` fallback)
//! - [`get_suppress_file_path`] — path to the avoider suppression file
//! - [`suppress_avoider`] / [`clear_suppression`] — write/clear the suppress timestamp

use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::fs;

/// Get the runtime directory (`$XDG_RUNTIME_DIR` or `/tmp` fallback).
///
/// Sanitizes the env value to defend against path-traversal injection:
/// the path must be absolute, contain no `..` components, and exist as a
/// directory. On any failure, falls back to `/tmp` and emits a one-shot
/// warning since `/tmp` is world-writable on most systems.
pub fn runtime_dir() -> PathBuf {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);

    fn sanitize(raw: &str) -> Option<PathBuf> {
        let p = PathBuf::from(raw);
        if !p.is_absolute() {
            return None;
        }
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return None;
        }
        // Existence check defends against typo'd or hostile values.
        if !p.is_dir() {
            return None;
        }
        Some(p)
    }

    if let Some(dir) = env::var("XDG_RUNTIME_DIR").ok().and_then(|v| sanitize(&v)) {
        return dir;
    }
    if !FALLBACK_WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "XDG_RUNTIME_DIR unset or invalid; falling back to /tmp (world-writable, less secure)"
        );
    }
    PathBuf::from("/tmp")
}

/// Get the path to the avoider suppress file.
///
/// The suppress file is located at `$XDG_RUNTIME_DIR/media-avoider-suppress`.
/// When this file exists and contains a recent timestamp, the avoider daemon
/// will skip repositioning to prevent feedback loops.
pub fn get_suppress_file_path() -> PathBuf {
    runtime_dir().join("media-avoider-suppress")
}

/// Write a value to the suppress file. Logs on failure.
async fn write_suppress_file(content: &str) {
    if let Err(e) = fs::write(get_suppress_file_path(), content).await {
        tracing::debug!("failed to write suppress file: {e}");
    }
}

/// Write a timestamp to the suppress file to prevent avoider repositioning.
///
/// The avoider daemon checks this file before repositioning. If the timestamp
/// is recent (within the configured timeout), it skips the reposition operation.
/// This prevents feedback loops when commands intentionally move windows.
pub async fn suppress_avoider() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    write_suppress_file(&timestamp.to_string()).await;
}

/// Clear the avoider suppression to allow the next avoid trigger to run.
///
/// This writes a timestamp of 0 (epoch) which will always appear as stale
/// to the avoider daemon, allowing it to run on the next event.
pub async fn clear_suppression() {
    write_suppress_file("0").await;
}

/// Test-only mutex serializing access to process-wide state used by the
/// suppress file and runtime-dir resolution: `$XDG_RUNTIME_DIR`,
/// `$HYPRLAND_INSTANCE_SIGNATURE`, and the on-disk suppress file path.
/// Single process-wide async mutex serialising ALL tests that touch shared
/// global state: `XDG_RUNTIME_DIR`, `HYPRLAND_INSTANCE_SIGNATURE`,
/// `MPV_IPC_SOCKET`, or the on-disk suppress file.
///
/// Using ONE lock domain eliminates the inter-domain race that previously
/// existed between sync env-mutation tests and async suppress-file tests.
/// All callers hold this with `let _g = async_env_test_mutex().lock().await`
/// for the full test body.
#[cfg(test)]
pub(crate) fn async_env_test_mutex() -> &'static tokio::sync::Mutex<()> {
    use std::sync::OnceLock;
    static M: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    M.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to safely set an environment variable in tests.
    ///
    /// # Safety
    ///
    /// This is only safe in single-threaded test contexts.
    unsafe fn set_env(key: &str, value: &str) {
        // SAFETY: Caller guarantees single-threaded context
        unsafe { env::set_var(key, value) };
    }

    /// Helper to safely remove an environment variable in tests.
    ///
    /// # Safety
    ///
    /// This is only safe in single-threaded test contexts.
    unsafe fn remove_env(key: &str) {
        // SAFETY: Caller guarantees single-threaded context
        unsafe { env::remove_var(key) };
    }

    #[tokio::test]
    async fn suppress_file_path_uses_xdg_runtime_dir() {
        let _g = async_env_test_mutex().lock().await;
        // Save original value
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: Test is single-threaded and restores the original value
        unsafe {
            // Test with XDG_RUNTIME_DIR set
            set_env("XDG_RUNTIME_DIR", "/run/user/1000");
            let path = get_suppress_file_path();
            assert_eq!(path, PathBuf::from("/run/user/1000/media-avoider-suppress"));

            // Restore original value
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    #[tokio::test]
    async fn suppress_file_path_fallback() {
        let _g = async_env_test_mutex().lock().await;
        // Save original value
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: Test is single-threaded and restores the original value
        unsafe {
            // Test without XDG_RUNTIME_DIR
            remove_env("XDG_RUNTIME_DIR");
            let path = get_suppress_file_path();
            assert_eq!(path, PathBuf::from("/tmp/media-avoider-suppress"));

            // Restore original value
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            }
        }
    }

    #[tokio::test]
    async fn suppress_avoider_writes_file() {
        let _g = async_env_test_mutex().lock().await;
        suppress_avoider().await;
        let path = get_suppress_file_path();
        assert!(path.exists(), "suppress file should exist at {path:?}");
    }

    #[tokio::test]
    async fn clear_suppression_writes_file() {
        let _g = async_env_test_mutex().lock().await;
        clear_suppression().await;
        let path = get_suppress_file_path();
        assert!(path.exists(), "suppress file should exist at {path:?}");
    }

    /// Security: ensure `runtime_dir()` rejects relative XDG_RUNTIME_DIR
    /// (would otherwise resolve to CWD-relative paths).
    #[tokio::test]
    async fn runtime_dir_rejects_relative_path() {
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded test
        unsafe {
            set_env("XDG_RUNTIME_DIR", "tmp/runtime");
            let dir = runtime_dir();
            assert_eq!(dir, PathBuf::from("/tmp"), "relative path must be rejected");
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    /// Security: ensure `runtime_dir()` rejects paths containing `..`.
    #[tokio::test]
    async fn runtime_dir_rejects_parent_dir_traversal() {
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded
        unsafe {
            set_env("XDG_RUNTIME_DIR", "/run/user/1000/../../etc");
            let dir = runtime_dir();
            assert_eq!(dir, PathBuf::from("/tmp"), "parent-dir traversal must be rejected");
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    /// Security: ensure `runtime_dir()` rejects nonexistent paths
    /// (defends against typo'd or hostile values pointing to attacker-controlled
    /// future locations).
    #[tokio::test]
    async fn runtime_dir_rejects_nonexistent() {
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded
        unsafe {
            set_env(
                "XDG_RUNTIME_DIR",
                "/definitely/does/not/exist/runtime-dir-12345",
            );
            let dir = runtime_dir();
            assert_eq!(dir, PathBuf::from("/tmp"));
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }
}
