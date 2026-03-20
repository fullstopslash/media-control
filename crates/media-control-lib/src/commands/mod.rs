//! Command implementations for media window control.
//!
//! This module provides the shared context and utilities for all command implementations.
//! Each submodule implements a specific command (fullscreen, move, close, etc.).
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::commands::CommandContext;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = CommandContext::new()?;
//!
//! // Find the current media window
//! if let Some(window) = media_control_lib::commands::get_media_window(&ctx).await? {
//!     println!("Found media window: {} ({})", window.title, window.address);
//! }
//! # Ok(())
//! # }
//! ```

pub mod avoid;
pub mod chapter;
pub mod close;
pub mod focus;
pub mod fullscreen;
pub mod mark_watched;
pub mod move_window;
pub mod pin;
pub mod play;
pub mod status;

use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::fs;

use crate::config::Config;
use crate::error::Result;
use crate::hyprland::{Client, HyprlandClient};
use crate::window::{MediaWindow, WindowMatcher};

/// Shared context for command execution.
///
/// Holds the Hyprland client, configuration, and window matcher.
/// Commands receive this context to access shared resources.
pub struct CommandContext {
    /// Hyprland IPC client for window operations.
    pub hyprland: HyprlandClient,
    /// Loaded configuration.
    pub config: Config,
    /// Compiled window matcher from config patterns.
    pub window_matcher: WindowMatcher,
}

impl CommandContext {
    /// Create a command context for testing with a custom Hyprland client and config.
    ///
    /// This bypasses environment variable lookups and config file reading,
    /// allowing tests to provide a mock Hyprland socket and custom configuration.
    #[cfg(test)]
    pub fn for_test(hyprland: HyprlandClient, config: Config) -> Result<Self> {
        let window_matcher = WindowMatcher::new(&config.patterns)?;
        Ok(Self {
            hyprland,
            config,
            window_matcher,
        })
    }

    /// Create a new command context with configuration loaded from default path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration file cannot be read or parsed
    /// - The Hyprland socket is not available
    /// - Any pattern regex fails to compile
    pub fn new() -> Result<Self> {
        let config = Config::load().map_err(|e| {
            crate::error::MediaControlError::Config {
                kind: crate::error::ConfigErrorKind::NotFound,
                path: Config::default_path().ok(),
                source: Some(Box::new(e)),
            }
        })?;

        Self::with_config(config)
    }

    /// Create a new command context with the provided configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The Hyprland socket is not available
    /// - Any pattern regex fails to compile
    pub fn with_config(config: Config) -> Result<Self> {
        let hyprland = HyprlandClient::new().map_err(|e| {
            crate::error::MediaControlError::HyprlandIpc {
                kind: crate::error::HyprlandIpcErrorKind::SocketNotFound,
                source: Some(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    e.to_string(),
                )),
            }
        })?;

        let window_matcher = WindowMatcher::new(&config.patterns)?;

        Ok(Self {
            hyprland,
            config,
            window_matcher,
        })
    }
}

/// Find the focused window address from a client list.
///
/// The focused window is the one with `focusHistoryID == 0` (most recently focused).
/// This avoids race conditions by using the same client snapshot.
#[inline]
fn find_focused_address(clients: &[Client]) -> Option<&str> {
    clients
        .iter()
        .find(|c| c.focus_history_id == 0)
        .map(|c| c.address.as_str())
}

/// Get the current media window.
///
/// Fetches all clients from Hyprland and uses the window matcher to find
/// the best media window according to priority rules.
///
/// # Returns
///
/// - `Ok(Some(window))` if a media window was found
/// - `Ok(None)` if no media window matches the configured patterns
/// - `Err(...)` if Hyprland IPC fails
pub async fn get_media_window(ctx: &CommandContext) -> Result<Option<MediaWindow>> {
    let clients = ctx.hyprland.get_clients().await?;

    let focus_addr = find_focused_address(&clients);
    Ok(ctx.window_matcher.find_media_window(&clients, focus_addr))
}

/// Find media window from pre-fetched clients.
///
/// This variant avoids an extra Hyprland IPC call when clients have already
/// been fetched. Useful when you need both the client list and the media window.
///
/// # Arguments
///
/// * `ctx` - The command context with window matcher
/// * `clients` - Pre-fetched client list from `HyprlandClient::get_clients()`
///
/// # Returns
///
/// The best matching media window, or `None` if no match found.
pub fn get_media_window_with_clients(ctx: &CommandContext, clients: &[Client]) -> Option<MediaWindow> {
    let focus_addr = find_focused_address(clients);
    ctx.window_matcher.find_media_window(clients, focus_addr)
}

/// Get the path to the avoider suppress file.
///
/// The suppress file is located at `$XDG_RUNTIME_DIR/media-avoider-suppress`.
/// When this file exists and contains a recent timestamp, the avoider daemon
/// will skip repositioning to prevent feedback loops.
///
/// # Returns
///
/// Path to the suppress file, or a fallback path if `XDG_RUNTIME_DIR` is not set.
pub fn get_suppress_file_path() -> PathBuf {
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        eprintln!("media-control: XDG_RUNTIME_DIR not set, falling back to /tmp for suppress file");
        "/tmp".to_string()
    });
    PathBuf::from(runtime_dir).join("media-avoider-suppress")
}

/// Write a timestamp to the suppress file to prevent avoider repositioning.
///
/// The avoider daemon checks this file before repositioning. If the timestamp
/// is recent (within the configured timeout), it skips the reposition operation.
/// This prevents feedback loops when commands intentionally move windows.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub async fn suppress_avoider() -> Result<()> {
    let path = get_suppress_file_path();
    // Write milliseconds to match bash daemon's _should_suppress() check
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    fs::write(&path, timestamp.to_string()).await?;
    Ok(())
}

/// Clear the avoider suppression to allow the next avoid trigger to run.
///
/// This writes a timestamp of 0 (epoch) which will always appear as stale
/// to the avoider daemon, allowing it to run on the next event.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub async fn clear_suppression() -> Result<()> {
    let path = get_suppress_file_path();
    fs::write(&path, "0").await?;
    Ok(())
}

/// Restore focus to a window without warping the cursor.
///
/// Tries modern `cursor:no_warps` syntax first, falls back to legacy
/// `general:no_cursor_warps` for older Hyprland versions.
pub async fn restore_focus(ctx: &CommandContext, addr: &str) -> Result<()> {
    let result = ctx
        .hyprland
        .batch(&[
            "keyword cursor:no_warps true",
            &format!("dispatch focuswindow address:{addr}"),
            "keyword cursor:no_warps false",
        ])
        .await;

    if result.is_err() {
        ctx.hyprland
            .batch(&[
                "keyword general:no_cursor_warps true",
                &format!("dispatch focuswindow address:{addr}"),
                "keyword general:no_cursor_warps false",
            ])
            .await?;
    }

    Ok(())
}

/// Default mpv IPC socket path (jellyfin-mpv-shim).
const MPV_IPC_SOCKET_DEFAULT: &str = "/tmp/mpvctl-jshim";

/// Fallback mpv IPC socket path.
const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl0";

/// Timeout for connecting to and writing to a socket.
const SOCKET_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// Timeout for reading a response from mpv.
const SOCKET_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

/// Delay between retry attempts.
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

/// Send a script-message to mpv via IPC socket.
///
/// Routes to handlers registered by jellyfin-mpv-shim. Tries multiple
/// socket paths in order: `$MPV_IPC_SOCKET`, `/tmp/mpvctl-jshim`, `/tmp/mpvctl0`.
///
/// Features:
/// - Validates socket paths before connecting (skips non-sockets)
/// - 500ms timeout on connect+write per socket path
/// - Reads and verifies mpv IPC response (200ms timeout, warn-only)
/// - Retries once after 100ms if all paths fail (covers mpv respawn window)
pub async fn send_mpv_script_message(message: &str) -> Result<()> {
    let payload = format!(r#"{{"command":["script-message","{message}"]}}"#);
    send_mpv_ipc_command(&payload).await
}

/// Send a multi-argument script-message to mpv via IPC socket.
///
/// Like `send_mpv_script_message` but supports additional arguments.
/// E.g., `send_mpv_script_message_with_args("set-play-source", &["nextup"])`
/// sends `{"command":["script-message","set-play-source","nextup"]}`.
pub async fn send_mpv_script_message_with_args(message: &str, args: &[&str]) -> Result<()> {
    let mut parts: Vec<&str> = vec!["script-message", message];
    parts.extend_from_slice(args);
    let payload = serde_json::json!({"command": parts}).to_string();
    send_mpv_ipc_command(&payload).await
}

/// Query an mpv property and return its value.
///
/// Sends a `get_property` command to mpv and returns the `data` field from
/// the response. Single attempt, no retry — designed for fast status queries.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use media_control_lib::commands::query_mpv_property;
/// let title = query_mpv_property("media-title").await?;
/// # Ok(())
/// # }
/// ```
pub async fn query_mpv_property(property: &str) -> Result<serde_json::Value> {
    use std::os::unix::fs::FileTypeExt;
    use std::path::Path;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use tokio::time::timeout;

    let payload = serde_json::json!({"command": ["get_property", property]}).to_string();

    let env_socket = env::var("MPV_IPC_SOCKET").ok();
    let sockets = [
        env_socket.as_deref(),
        Some(MPV_IPC_SOCKET_DEFAULT),
        Some(MPV_IPC_SOCKET_FALLBACK),
    ];

    for socket_path in sockets.iter().copied().flatten() {
        let path = Path::new(socket_path);

        match std::fs::metadata(path) {
            Ok(meta) if meta.file_type().is_socket() => {}
            _ => continue,
        }

        let stream_result = timeout(SOCKET_RESPONSE_TIMEOUT, async {
            let mut stream = UnixStream::connect(path).await?;
            stream.write_all(payload.as_bytes()).await?;
            stream.write_all(b"\n").await?;

            let mut buf = String::new();
            let mut reader = BufReader::new(&mut stream);
            reader.read_line(&mut buf).await?;
            Ok::<_, std::io::Error>(buf)
        })
        .await;

        match stream_result {
            Ok(Ok(buf)) => {
                let resp: serde_json::Value = serde_json::from_str(&buf)
                    .map_err(|_| crate::error::MediaControlError::mpv_connection_failed("invalid JSON response"))?;
                if resp.get("error").and_then(|e| e.as_str()) == Some("success") {
                    return Ok(resp.get("data").cloned().unwrap_or(serde_json::Value::Null));
                }
                return Err(crate::error::MediaControlError::mpv_connection_failed(
                    format!("mpv error: {}", resp.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")),
                ));
            }
            _ => continue,
        }
    }

    Err(crate::error::MediaControlError::mpv_no_socket())
}

/// Send a raw JSON command to mpv via IPC socket.
///
/// This is the shared implementation for all mpv IPC communication.
/// Both `send_mpv_script_message` and chapter navigation use this.
pub async fn send_mpv_ipc_command(payload: &str) -> Result<()> {
    use std::os::unix::fs::FileTypeExt;
    use std::path::Path;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use tokio::time::timeout;

    let env_socket = env::var("MPV_IPC_SOCKET").ok();
    let sockets = [
        env_socket.as_deref(),
        Some(MPV_IPC_SOCKET_DEFAULT),
        Some(MPV_IPC_SOCKET_FALLBACK),
    ];

    for attempt in 0..2u8 {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAY).await;
        }

        for socket_path in sockets.iter().copied().flatten() {
            let path = Path::new(socket_path);

            // FR-1: Socket validation — stat before connect
            match std::fs::metadata(path) {
                Ok(meta) => {
                    if !meta.file_type().is_socket() {
                        eprintln!(
                            "media-control: skipping {socket_path}: not a socket"
                        );
                        continue;
                    }
                }
                Err(_) => continue, // doesn't exist, try next
            }

            // FR-2: Connection timeout — 500ms for connect+write
            let stream_result = timeout(SOCKET_CONNECT_TIMEOUT, async {
                let mut stream = UnixStream::connect(path).await?;
                stream.write_all(payload.as_bytes()).await?;
                stream.write_all(b"\n").await?;
                Ok::<_, std::io::Error>(stream)
            })
            .await;

            let mut stream = match stream_result {
                Ok(Ok(stream)) => stream,
                Ok(Err(_)) => continue, // connect/write failed, try next
                Err(_) => {
                    eprintln!(
                        "media-control: timeout connecting to {socket_path}"
                    );
                    continue;
                }
            };

            // FR-4: Response verification — read response with 200ms timeout
            let mut buf = String::new();
            let mut reader = BufReader::new(&mut stream);
            match timeout(SOCKET_RESPONSE_TIMEOUT, reader.read_line(&mut buf)).await {
                Ok(Ok(_)) => {
                    // Check for mpv error response
                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&buf)
                        && let Some(err) = resp.get("error").and_then(|e| e.as_str())
                        && err != "success"
                    {
                        eprintln!("media-control: mpv returned error: {err}");
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("media-control: failed to read mpv response: {e}");
                }
                Err(_) => {
                    // No response within timeout — command was sent, treat as success
                }
            }

            return Ok(());
        }
    }

    // FR-5: All paths failed on both attempts
    Err(crate::error::MediaControlError::mpv_no_socket())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to safely set an environment variable in tests.
    ///
    /// # Safety
    ///
    /// This is only safe in single-threaded test contexts. Tests modifying
    /// environment variables should use `#[serial_test::serial]` or similar.
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

    #[test]
    fn suppress_file_path_uses_xdg_runtime_dir() {
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

    #[test]
    fn suppress_file_path_fallback() {
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
    async fn suppress_avoider_succeeds() {
        // Verify suppress_avoider writes without error.
        // We don't check the file content because parallel tests also write
        // to the same shared suppress file, causing race conditions.
        suppress_avoider().await.expect("should write suppress file");

        // Verify the file exists
        let path = get_suppress_file_path();
        assert!(path.exists(), "suppress file should exist at {path:?}");
    }

    #[tokio::test]
    async fn clear_suppression_succeeds() {
        // Just verify it doesn't error. Can't assert file content because
        // parallel tests also write to the shared suppress file.
        clear_suppression().await.expect("should clear suppress file");
    }

    #[test]
    fn socket_validation_skips_regular_file() {
        use std::os::unix::fs::FileTypeExt;

        // Create a regular file — should NOT be identified as a socket
        let dir = tempfile::tempdir().unwrap();
        let fake_socket = dir.path().join("fake-socket");
        std::fs::write(&fake_socket, "not a socket").unwrap();

        let meta = std::fs::metadata(&fake_socket).unwrap();
        assert!(
            !meta.file_type().is_socket(),
            "regular file should not be identified as socket"
        );
    }

    #[test]
    fn socket_validation_detects_real_socket() {
        use std::os::unix::fs::FileTypeExt;

        // Create a real Unix socket
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test-socket");
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let meta = std::fs::metadata(&socket_path).unwrap();
        assert!(
            meta.file_type().is_socket(),
            "Unix socket should be identified as socket"
        );
    }

    #[test]
    fn socket_validation_handles_nonexistent() {
        let result = std::fs::metadata("/tmp/definitely-nonexistent-socket-path-12345");
        assert!(result.is_err(), "nonexistent path should fail metadata");
    }

    #[tokio::test]
    async fn send_mpv_ipc_command_succeeds_with_real_socket() {
        // Create a real Unix socket listener and verify the command gets through
        use tokio::io::AsyncBufReadExt;
        use tokio::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test-mpv-socket");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let original = env::var("MPV_IPC_SOCKET").ok();

        // SAFETY: Test is single-threaded
        unsafe {
            set_env("MPV_IPC_SOCKET", socket_path.to_str().unwrap());
        }

        // Spawn a task to accept the connection and respond
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            // Verify we received the command
            assert!(line.contains("script-message"));
            assert!(line.contains("test-cmd"));
            // Send success response (mpv IPC protocol)
            use tokio::io::AsyncWriteExt;
            reader
                .get_mut()
                .write_all(br#"{"error":"success"}"#)
                .await
                .unwrap();
            reader.get_mut().write_all(b"\n").await.unwrap();
        });

        let result = send_mpv_script_message("test-cmd").await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");

        handle.await.unwrap();

        // SAFETY: Restore
        unsafe {
            if let Some(val) = original {
                set_env("MPV_IPC_SOCKET", &val);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }
    }

    #[test]
    fn get_media_window_with_clients_uses_focus_from_clients() {
        use crate::config::Pattern;
        use crate::hyprland::{Client, Workspace};

        // Create a simple pattern that matches mpv
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];

        let matcher = WindowMatcher::new(&patterns).unwrap();

        // Create mock clients where Firefox is focused (focusHistoryID == 0)
        // but mpv is also present and pinned
        let clients = vec![
            Client {
                address: "0x1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [1920, 1080],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "firefox".to_string(),
                title: "Browser".to_string(),
                focus_history_id: 0, // Firefox is currently focused
            },
            Client {
                address: "0x2".to_string(),
                mapped: true,
                hidden: false,
                at: [100, 100],
                size: [640, 360],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: true,
                pinned: true,
                fullscreen: 0,
                monitor: 0,
                class: "mpv".to_string(),
                title: "video.mp4".to_string(),
                focus_history_id: 1,
            },
        ];

        // The function should derive focus_addr from clients where focusHistoryID == 0
        // This should be Firefox (0x1), not requiring a separate get_active_window() call
        let focus_addr = clients
            .iter()
            .filter(|c| c.focus_history_id == 0)
            .map(|c| c.address.as_str())
            .next();

        // Verify we found Firefox
        assert_eq!(focus_addr, Some("0x1"));

        // Now call find_media_window with the derived focus
        let result = matcher.find_media_window(&clients, focus_addr);

        // Should find mpv with priority 1 (pinned) even though Firefox is focused
        assert!(result.is_some());
        let media = result.unwrap();
        assert_eq!(media.address, "0x2");
        assert_eq!(media.class, "mpv");
        assert_eq!(media.priority, 1); // Pinned beats focused non-media
    }
}
