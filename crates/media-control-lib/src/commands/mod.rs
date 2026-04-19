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
pub mod keep;
pub mod mark_watched;
pub mod minify;
pub mod move_window;
pub mod pin;
pub mod play;
pub mod random;
pub mod seek;
pub mod status;

use std::env;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

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
        let window_matcher = WindowMatcher::new(&config.patterns);
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
        // `ConfigError` bridges via `#[from]` — preserves the typed source
        // chain (path, regex, range failures) instead of `Box<dyn Error>`.
        let config = Config::load()?;
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
        let hyprland =
            HyprlandClient::new().map_err(|e| crate::error::MediaControlError::HyprlandIpc {
                kind: crate::error::HyprlandIpcErrorKind::SocketNotFound,
                source: Some(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    e.to_string(),
                )),
            })?;

        let window_matcher = WindowMatcher::new(&config.patterns);

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
pub(crate) fn find_focused_address(clients: &[Client]) -> Option<&str> {
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
pub fn get_media_window_with_clients(
    ctx: &CommandContext,
    clients: &[Client],
) -> Option<MediaWindow> {
    let focus_addr = find_focused_address(clients);
    ctx.window_matcher.find_media_window(clients, focus_addr)
}

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
/// Any test that mutates these must hold this lock for the whole body
/// or it will race with parallel tests touching the same globals.
#[cfg(test)]
pub(crate) fn suppress_file_test_mutex() -> &'static std::sync::Mutex<()> {
    use std::sync::OnceLock;
    static M: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    M.get_or_init(|| std::sync::Mutex::new(()))
}

/// Build a `dispatch focuswindow address:<addr>` command string.
#[inline]
pub(crate) fn focus_window_cmd(addr: &str) -> String {
    format!("dispatch focuswindow address:{addr}")
}

/// Build a `dispatch pin address:<addr>` command string.
#[inline]
pub(crate) fn pin_cmd(addr: &str) -> String {
    format!("dispatch pin address:{addr}")
}

/// Build a `dispatch togglefloating address:<addr>` command string.
#[inline]
pub(crate) fn toggle_floating_cmd(addr: &str) -> String {
    format!("dispatch togglefloating address:{addr}")
}

/// Restore focus to a window without warping the cursor.
///
/// Tries modern `cursor:no_warps` syntax first, falls back to legacy
/// `general:no_cursor_warps` for older Hyprland versions.
pub async fn restore_focus(ctx: &CommandContext, addr: &str) -> Result<()> {
    let focus = focus_window_cmd(addr);

    let result = ctx
        .hyprland
        .batch(&[
            "keyword cursor:no_warps true",
            &focus,
            "keyword cursor:no_warps false",
        ])
        .await;

    if result.is_err() {
        ctx.hyprland
            .batch(&[
                "keyword general:no_cursor_warps true",
                &focus,
                "keyword general:no_cursor_warps false",
            ])
            .await?;
    }

    Ok(())
}

/// Get the path to the minified state file.
///
/// Presence of this file means the media window is in minified mode.
/// Located in `$XDG_RUNTIME_DIR` (tmpfs) so it resets on reboot.
pub fn get_minify_state_path() -> PathBuf {
    runtime_dir().join("media-control-minified")
}

/// Check if minified mode is active.
pub fn is_minified() -> bool {
    get_minify_state_path().exists()
}

/// Toggle minified mode on/off. Returns the new state.
pub async fn toggle_minified() -> Result<bool> {
    let path = get_minify_state_path();
    if path.exists() {
        fs::remove_file(&path).await?;
        Ok(false)
    } else {
        fs::write(&path, "1").await?;
        Ok(true)
    }
}

/// Get the effective window dimensions, accounting for minified mode.
///
/// Defends against pathological config (NaN, negative, or huge `minified_scale`)
/// by clamping the scaled value into a sane range before converting back to
/// `i32`. Without this, an `f32 → i32` saturating-cast on `NaN` or out-of-range
/// values would yield `0` / `i32::MAX` and propagate into geometry math.
pub fn effective_dimensions(ctx: &CommandContext) -> (i32, i32) {
    let w = ctx.config.positions.width;
    let h = ctx.config.positions.height;
    if !is_minified() {
        return (w, h);
    }
    let raw_scale = ctx.config.positioning.minified_scale;
    let scale = if raw_scale.is_finite() {
        raw_scale.clamp(0.0, 10.0)
    } else {
        1.0
    };
    // `clamp` rules out NaN/inf so the cast below is well-defined;
    // truncation toward zero is fine for pixel dimensions.
    #[allow(clippy::cast_possible_truncation)]
    let scaled = |dim: i32| ((dim as f32) * scale).clamp(0.0, i32::MAX as f32) as i32;
    (scaled(w), scaled(h))
}

/// Resolve a position name adjusted for minified mode.
///
/// When minified, "x_right" and "y_bottom" shift outward because the
/// smaller window needs a larger x/y to maintain the same gap from the
/// screen edge. "x_left" and "y_top" stay the same.
pub fn resolve_effective_position(ctx: &CommandContext, name: &str) -> Option<i32> {
    let raw = ctx.config.resolve_position(name)?;
    if !is_minified() {
        return Some(raw);
    }
    let p = &ctx.config.positions;
    let (ew, eh) = effective_dimensions(ctx);
    match name {
        "x_right" => Some(raw + (p.width - ew)),
        "y_bottom" => Some(raw + (p.height - eh)),
        _ => Some(raw),
    }
}

/// Resize and move a window to its default configured position.
///
/// Resolves the default x/y from config (adjusted for minified mode),
/// then batches a resize + move. Used by fullscreen exit, pin, and minify.
pub(crate) async fn reposition_to_default(ctx: &CommandContext, addr: &str) -> Result<()> {
    let positioning = &ctx.config.positioning;
    let target_x = resolve_effective_position(ctx, &positioning.default_x)
        .unwrap_or(ctx.config.positions.x_right);
    let target_y = resolve_effective_position(ctx, &positioning.default_y)
        .unwrap_or(ctx.config.positions.y_bottom);
    let (ew, eh) = effective_dimensions(ctx);
    ctx.hyprland
        .batch(&[
            &format!("dispatch resizewindowpixel exact {ew} {eh},address:{addr}"),
            &format!("dispatch movewindowpixel exact {target_x} {target_y},address:{addr}"),
        ])
        .await?;
    Ok(())
}

/// Default mpv IPC socket path (mpv-shim).
pub(crate) const MPV_IPC_SOCKET_DEFAULT: &str = "/tmp/mpv-shim";

/// Fallback mpv IPC socket path (legacy).
const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl-jshim";

/// Timeout for connecting to and writing to a socket (local Unix socket — fast).
const SOCKET_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// Timeout for reading a response from mpv.
const SOCKET_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// Delay between retry attempts.
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

/// Check that a path exists and is a Unix socket.
///
/// Uses `symlink_metadata` (lstat) — does NOT follow symlinks. This defends
/// against an attacker placing a symlink at a predictable `/tmp` path that
/// points to a file or socket they control.
fn is_unix_socket(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false)
}

/// Shim query socket path for cached lookups.
const SHIM_QUERY_SOCKET: &str = "/tmp/mpv-shim-query.sock";

/// Connect to a Unix socket and write `payload\n`, returning the open stream.
///
/// Bounded by `SOCKET_CONNECT_TIMEOUT`. Caller is responsible for
/// validating that `path` is a real Unix socket via [`is_unix_socket`]
/// before calling — defends against symlink-to-regular-file in /tmp.
async fn connect_and_write(
    path: &Path,
    payload: &str,
    append_newline: bool,
) -> std::io::Result<UnixStream> {
    timeout(SOCKET_CONNECT_TIMEOUT, async {
        let mut stream = UnixStream::connect(path).await?;
        stream.write_all(payload.as_bytes()).await?;
        if append_newline {
            stream.write_all(b"\n").await?;
        }
        Ok::<_, std::io::Error>(stream)
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "socket connect timeout"))?
}

/// Query the shim's query socket with a JSON request.
///
/// Returns the raw response string, or None if the socket is unavailable.
/// Single attempt, no retry — designed for fast cached lookups.
pub async fn query_shim(request: &serde_json::Value) -> Option<String> {
    let path = Path::new(SHIM_QUERY_SOCKET);
    if !is_unix_socket(path) {
        return None;
    }

    let payload = request.to_string();
    let mut stream = connect_and_write(path, &payload, true).await.ok()?;
    if stream.shutdown().await.is_err() {
        return None;
    }

    let mut buf = String::new();
    stream.read_to_string(&mut buf).await.ok()?;
    if buf.is_empty() { None } else { Some(buf) }
}

/// Get the ordered list of mpv IPC socket paths to try.
fn mpv_socket_paths() -> Vec<String> {
    let mut paths = Vec::with_capacity(3);
    if let Ok(env_path) = env::var("MPV_IPC_SOCKET") {
        paths.push(env_path);
    }
    paths.push(MPV_IPC_SOCKET_DEFAULT.to_string());
    paths.push(MPV_IPC_SOCKET_FALLBACK.to_string());
    paths
}

/// Low-level: connect to a single validated socket, send payload, optionally read response.
///
/// Returns the parsed JSON response if `read_response` is true, or `None` if fire-and-forget.
/// Skips non-socket paths. Uses SOCKET_CONNECT_TIMEOUT for connect+write,
/// SOCKET_RESPONSE_TIMEOUT for reading.
async fn mpv_ipc_exchange(
    socket_path: &str,
    payload: &str,
    read_response: bool,
) -> std::result::Result<Option<serde_json::Value>, ()> {
    let path = Path::new(socket_path);

    if !is_unix_socket(path) {
        // Use lstat-based exists so a dangling symlink is reported, but a
        // missing path stays quiet (typical case during startup).
        if std::fs::symlink_metadata(path).is_ok() {
            tracing::warn!("skipping {socket_path}: not a socket");
        }
        return Err(());
    }

    // Connect + write with timeout
    let mut stream = match connect_and_write(path, payload, true).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            tracing::warn!("timeout connecting to {socket_path}");
            return Err(());
        }
        Err(e) => {
            tracing::debug!("connect/write to {socket_path} failed: {e}");
            return Err(());
        }
    };

    if !read_response {
        return Ok(None);
    }

    // Read response with timeout, skipping mpv event lines.
    // Reuse a single buffer to avoid per-line heap allocation when mpv
    // floods events between our request and its response.
    let mut reader = BufReader::new(&mut stream);
    let read_result = timeout(SOCKET_RESPONSE_TIMEOUT, async {
        let mut buf = String::with_capacity(256);
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await?;
            if n == 0 {
                // EOF — mpv closed the connection
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "mpv closed connection",
                ));
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&buf) {
                // Skip unsolicited event messages
                if val.get("event").is_some() && val.get("error").is_none() {
                    continue;
                }
                return Ok::<_, std::io::Error>(val);
            }
            // Unparseable line — skip
        }
    })
    .await;

    match read_result {
        Ok(Ok(val)) => Ok(Some(val)),
        _ => Ok(None), // read failed or timed out — command was sent
    }
}

/// Send a script-message to mpv via IPC socket (fire-and-forget).
///
/// Writes the command and closes — does NOT read the response. During rapid
/// fire, mpv floods the socket with event lines (file-loaded, property-change,
/// etc.) and reading through them to find the response adds 10-50ms+ per call.
/// Script-messages are delivered asynchronously by mpv regardless of response.
pub async fn send_mpv_script_message(message: &str) -> Result<()> {
    send_mpv_script_message_with_args(message, &[]).await
}

/// Send a multi-argument script-message to mpv (fire-and-forget).
pub async fn send_mpv_script_message_with_args(message: &str, args: &[&str]) -> Result<()> {
    let mut parts: Vec<&str> = vec!["script-message", message];
    parts.extend_from_slice(args);
    let payload = serde_json::json!({"command": parts}).to_string();
    send_mpv_ipc_command(&payload).await
}

/// Try each candidate mpv socket path in turn, with optional retry passes.
///
/// On the first path that yields `Some(T)`, returns it. Otherwise returns
/// `None` after all paths and retries are exhausted.
async fn try_mpv_paths<T, F, Fut>(retries: u8, mut op: F) -> Option<T>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let paths = mpv_socket_paths();
    for attempt in 0..=retries {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAY).await;
        }
        for path in &paths {
            // Clone the owned PathBuf into a String to sidestep the
            // closure-borrow-vs-future-lifetime tangle: `op` returns a future
            // that may outlive the `&path` borrow, so we hand it an owned copy.
            if let Some(v) = op(path.clone()).await {
                return Some(v);
            }
        }
    }
    None
}

/// Send a raw JSON command to mpv via IPC socket (fire-and-forget).
///
/// Tries multiple socket paths with retry. Does not read the response —
/// avoids blocking on mpv's event flood during rapid-fire commands.
pub async fn send_mpv_ipc_command(payload: &str) -> Result<()> {
    let ok = try_mpv_paths(1, |path| async move {
        mpv_ipc_exchange(&path, payload, false).await.ok().map(|_| ())
    })
    .await;

    ok.ok_or_else(crate::error::MediaControlError::mpv_no_socket)
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
    let payload = serde_json::json!({"command": ["get_property", property]}).to_string();

    // Single attempt across paths (no retry) — caller wants a fast lookup.
    let result = try_mpv_paths(0, |path| {
        let payload = &payload;
        async move {
            match mpv_ipc_exchange(&path, payload, true).await {
                Ok(Some(resp)) => Some(resp),
                _ => None,
            }
        }
    })
    .await;

    let resp = result.ok_or_else(crate::error::MediaControlError::mpv_no_socket)?;

    let err_str = resp.get("error").and_then(|e| e.as_str());
    if err_str == Some("success") {
        Ok(resp.get("data").cloned().unwrap_or(serde_json::Value::Null))
    } else {
        Err(crate::error::MediaControlError::mpv_connection_failed(
            format!("mpv error for {property:?}: {}", err_str.unwrap_or("unknown")),
        ))
    }
}

/// Send a payload to a specific mpv socket (fire-and-forget, no response read).
///
/// Used by `keep` for broadcast semantics.
pub async fn send_to_mpv_socket(socket_path: &str, payload: &str) -> bool {
    mpv_ipc_exchange(socket_path, payload, false).await.is_ok()
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
        let _g = super::suppress_file_test_mutex().lock().unwrap();
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
        let _g = super::suppress_file_test_mutex().lock().unwrap();
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
        let _g = super::suppress_file_test_mutex().lock().unwrap();
        suppress_avoider().await;
        let path = get_suppress_file_path();
        assert!(path.exists(), "suppress file should exist at {path:?}");
    }

    #[tokio::test]
    async fn clear_suppression_writes_file() {
        let _g = super::suppress_file_test_mutex().lock().unwrap();
        clear_suppression().await;
        let path = get_suppress_file_path();
        assert!(path.exists(), "suppress file should exist at {path:?}");
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

        // Spawn a task to accept the connection and verify the command arrived
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            // Verify we received the command
            assert!(line.contains("script-message"));
            assert!(line.contains("test-cmd"));
            // No response needed — fire-and-forget
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

        let matcher = WindowMatcher::new(&patterns);

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
                pid: 0,
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
                pid: 0,
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

    #[test]
    fn effective_dimensions_normal() {
        let config = Config::default();
        let ctx = CommandContext::for_test(
            HyprlandClient::with_socket_path("/tmp/nonexistent-test-socket".into()),
            config.clone(),
        )
        .unwrap();

        // When not minified, returns raw config dimensions
        let (w, h) = effective_dimensions(&ctx);
        assert_eq!(w, config.positions.width);
        assert_eq!(h, config.positions.height);
    }

    #[test]
    fn resolve_effective_position_normal() {
        let config = Config::default();
        let ctx = CommandContext::for_test(
            HyprlandClient::with_socket_path("/tmp/nonexistent-test-socket".into()),
            config.clone(),
        )
        .unwrap();

        assert_eq!(
            resolve_effective_position(&ctx, "x_left"),
            Some(config.positions.x_left)
        );
        assert_eq!(
            resolve_effective_position(&ctx, "x_right"),
            Some(config.positions.x_right)
        );
        assert_eq!(resolve_effective_position(&ctx, "unknown"), None);
    }

    /// Security: ensure `runtime_dir()` rejects relative XDG_RUNTIME_DIR
    /// (would otherwise resolve to CWD-relative paths).
    #[test]
    fn runtime_dir_rejects_relative_path() {
        let _g = super::suppress_file_test_mutex().lock().unwrap();
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
    #[test]
    fn runtime_dir_rejects_parent_dir_traversal() {
        let _g = super::suppress_file_test_mutex().lock().unwrap();
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
    #[test]
    fn runtime_dir_rejects_nonexistent() {
        let _g = super::suppress_file_test_mutex().lock().unwrap();
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

    /// Security: socket validation must NOT follow symlinks. A symlink
    /// pointing to a real socket should be rejected, since an attacker who
    /// controls /tmp could plant a symlink targeting a socket they own.
    #[test]
    fn socket_validation_rejects_symlink_to_socket() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real_sock = dir.path().join("real.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&real_sock).unwrap();

        let symlink_path = dir.path().join("via-symlink.sock");
        symlink(&real_sock, &symlink_path).unwrap();

        // is_unix_socket uses lstat; a symlink (even pointing at a real
        // socket) must NOT pass.
        assert!(!is_unix_socket(&symlink_path),
            "symlink to socket must be rejected by lstat-based check");
        // The real socket (no symlink in the path) should pass.
        assert!(is_unix_socket(&real_sock));
    }

    /// Helper duplicates clean up after themselves; verify symlink to a
    /// regular file is also rejected (would otherwise be silently followed
    /// by std::fs::metadata).
    #[test]
    fn socket_validation_rejects_symlink_to_regular_file() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let regular = dir.path().join("regular.txt");
        std::fs::write(&regular, "data").unwrap();
        let link = dir.path().join("link.sock");
        symlink(&regular, &link).unwrap();

        assert!(!is_unix_socket(&link));
    }

    /// `runtime_socket_path` (Hyprland helper) must reject env vars whose
    /// instance signature contains separators or `..`.
    #[test]
    fn runtime_socket_path_rejects_traversal_in_signature() {
        use crate::hyprland::runtime_socket_path;

        let _g = super::suppress_file_test_mutex().lock().unwrap();
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded test, restored at end
        unsafe {
            // Use a real existing dir so runtime_dir part passes validation
            set_env("XDG_RUNTIME_DIR", "/tmp");

            for bad in &["../escape", "a/b", ".hidden", "..", ""] {
                set_env("HYPRLAND_INSTANCE_SIGNATURE", bad);
                assert!(
                    runtime_socket_path(".socket.sock").is_err(),
                    "signature {bad:?} must be rejected"
                );
            }

            // Restore
            if let Some(v) = orig_runtime {
                set_env("XDG_RUNTIME_DIR", &v);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
            if let Some(v) = orig_sig {
                set_env("HYPRLAND_INSTANCE_SIGNATURE", &v);
            } else {
                remove_env("HYPRLAND_INSTANCE_SIGNATURE");
            }
        }
    }

    /// `runtime_socket_path` must reject relative XDG_RUNTIME_DIR.
    #[test]
    fn runtime_socket_path_rejects_relative_runtime_dir() {
        use crate::hyprland::runtime_socket_path;

        let _g = super::suppress_file_test_mutex().lock().unwrap();
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded
        unsafe {
            set_env("XDG_RUNTIME_DIR", "relative/path");
            set_env("HYPRLAND_INSTANCE_SIGNATURE", "valid_sig");
            assert!(runtime_socket_path(".socket.sock").is_err());

            // Restore
            if let Some(v) = orig_runtime {
                set_env("XDG_RUNTIME_DIR", &v);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
            if let Some(v) = orig_sig {
                set_env("HYPRLAND_INSTANCE_SIGNATURE", &v);
            } else {
                remove_env("HYPRLAND_INSTANCE_SIGNATURE");
            }
        }
    }

    /// Verify try_mpv_paths returns None when no socket responds — sanity
    /// check on the shared retry helper.
    #[tokio::test]
    async fn try_mpv_paths_returns_none_when_no_socket_works() {
        let original = env::var("MPV_IPC_SOCKET").ok();
        // SAFETY: single-threaded test
        unsafe {
            set_env("MPV_IPC_SOCKET", "/tmp/definitely-nonexistent-mpv-socket-xyz");
        }
        let result: Option<()> = try_mpv_paths(0, |_path| async { None }).await;
        assert!(result.is_none());

        // SAFETY: restore
        unsafe {
            if let Some(v) = original {
                set_env("MPV_IPC_SOCKET", &v);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }
    }
}
