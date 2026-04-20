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
pub mod context;
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

pub use context::{clear_suppression, get_suppress_file_path, runtime_dir, suppress_avoider};
#[cfg(test)]
pub(crate) use context::async_env_test_mutex;

use std::env;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
///
/// The no-warps keyword is always cleared — even when both IPC paths fail —
/// to avoid leaving the cursor permanently stuck in no-warp mode.
pub async fn restore_focus(ctx: &CommandContext, addr: &str) -> Result<()> {
    let focus = focus_window_cmd(addr);

    // Inner async block holds the actual logic so the cleanup `false` keyword
    // can be sent unconditionally after it, regardless of outcome.
    let result = async {
        // Try modern syntax first.
        let modern = ctx
            .hyprland
            .batch(&[
                "keyword cursor:no_warps true",
                &focus,
                "keyword cursor:no_warps false",
            ])
            .await;

        if modern.is_ok() {
            return Ok(());
        }

        // Fall back to legacy syntax for older Hyprland versions.
        ctx.hyprland
            .batch(&[
                "keyword general:no_cursor_warps true",
                &focus,
                "keyword general:no_cursor_warps false",
            ])
            .await
    }
    .await;

    // Safety net: if both paths failed we may have sent a `true` keyword
    // without a matching `false` (e.g. if Hyprland processed the first command
    // in a batch before the socket error). Clear both variants unconditionally
    // so the cursor is never permanently stuck in no-warp mode.
    if result.is_err() {
        let _ = ctx
            .hyprland
            .batch(&[
                "keyword cursor:no_warps false",
                "keyword general:no_cursor_warps false",
            ])
            .await;
    }

    Ok(result?)
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
///
/// Suppresses the avoider BEFORE dispatching — the move/resize events fire
/// within the daemon's debounce window and would otherwise race the suppress
/// file. Callers may still suppress earlier to cover additional dispatches
/// they issue in the same operation; this internal suppression is a safety
/// net so a caller can never forget the contract.
pub(crate) async fn reposition_to_default(ctx: &CommandContext, addr: &str) -> Result<()> {
    let positioning = &ctx.config.positioning;
    let target_x = resolve_effective_position(ctx, &positioning.default_x)
        .unwrap_or(ctx.config.positions.x_right);
    let target_y = resolve_effective_position(ctx, &positioning.default_y)
        .unwrap_or(ctx.config.positions.y_bottom);
    let (ew, eh) = effective_dimensions(ctx);

    // Suppress immediately before dispatch. Idempotent w.r.t. an earlier
    // caller-side suppress — both writes set a fresh timestamp.
    suppress_avoider().await;

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
/// `None` after all paths and retries are exhausted; emits a single
/// `tracing::debug!` listing every attempted path so the caller's
/// "no socket" error has actionable context in the logs.
///
/// # Timing
///
/// Per-path timeouts are applied inside `op`, not here, so total wall time
/// scales as `(retries + 1) * paths.len() * <op timeout>`. With the default
/// 50ms connect + 50ms response budget and 3 paths, a single call (`retries=0`)
/// caps at ~300ms; a retried call (`retries=1`) caps at ~625ms incl.
/// [`RETRY_DELAY`].
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
    tracing::debug!(
        "mpv IPC failed across {} path(s) after {} attempt(s): {:?}",
        paths.len(),
        retries as u32 + 1,
        paths
    );
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
/// the response. Single attempt across all candidate paths (no retry pass)
/// — designed for fast status queries.
///
/// # Timing
///
/// Per-path timeout is `SOCKET_CONNECT_TIMEOUT + SOCKET_RESPONSE_TIMEOUT`
/// (currently 100 ms). With up to 3 candidate sockets the worst-case wall
/// time is ~300 ms; in the common single-socket case it's bounded by the
/// per-path timeout.
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

/// Send a payload to a *specific* mpv socket (fire-and-forget, no response read).
///
/// Bypasses [`mpv_socket_paths`]'s discovery list — the caller already knows
/// which socket they want (e.g. the shim socket when closing a shim window).
/// Returns `true` on successful write, `false` on any failure.
pub(crate) async fn send_to_mpv_socket(socket_path: &str, payload: &str) -> bool {
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

        // Hold the async env-mutex across all `.await`s in this test so a
        // parallel test cannot rewrite MPV_IPC_SOCKET between our set_env
        // call and the internal `mpv_socket_paths()` env::var read inside
        // `send_mpv_script_message`. The guard is `Send` so this is safe.
        let _g = super::async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test-mpv-socket");
        let listener = UnixListener::bind(&socket_path).unwrap();

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
    #[tokio::test]
    async fn runtime_socket_path_rejects_traversal_in_signature() {
        use crate::hyprland::runtime_socket_path;

        let _g = super::async_env_test_mutex().lock().await;
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
    #[tokio::test]
    async fn runtime_socket_path_rejects_relative_runtime_dir() {
        use crate::hyprland::runtime_socket_path;

        let _g = super::async_env_test_mutex().lock().await;
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

    /// `runtime_socket_path` must reject `name` arguments that are empty,
    /// contain path separators, contain `..`, or reduce to `.` / `..`.
    /// Without this, callers could (intentionally or via injection) build
    /// paths that escape the `hypr/<sig>/` confinement.
    #[tokio::test]
    async fn runtime_socket_path_rejects_unsafe_name_argument() {
        use crate::hyprland::runtime_socket_path;

        let _g = super::async_env_test_mutex().lock().await;
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded test, restored at end
        unsafe {
            set_env("XDG_RUNTIME_DIR", "/tmp");
            set_env("HYPRLAND_INSTANCE_SIGNATURE", "valid_sig");

            // Names that must be rejected.
            for bad in &["", "..", ".", "../escape", "a/b", "a\\b", "x\0y"] {
                assert!(
                    runtime_socket_path(bad).is_err(),
                    "name {bad:?} must be rejected"
                );
            }

            // Sanity: the real socket names callers actually use must pass.
            for good in &[".socket.sock", ".socket2.sock"] {
                assert!(
                    runtime_socket_path(good).is_ok(),
                    "name {good:?} must be accepted"
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

    /// `reposition_to_default` must suppress the avoider BEFORE its dispatch
    /// so the move/resize events fire while the suppress timestamp is fresh.
    /// This locks in the self-enforcing contract — callers that forget to
    /// suppress won't trigger an avoid bounce.
    #[tokio::test]
    async fn reposition_to_default_self_suppresses_before_dispatch() {
        use crate::test_helpers::MockHyprland;

        // Hold the async env-mutex for the whole body — this test reads
        // and asserts on the shared on-disk suppress file, which other
        // parallel tests also write. Without this lock, a sibling's
        // `clear_suppression` (which writes "0") races with our read of
        // the timestamp and the assertion flaps.
        let _g = super::async_env_test_mutex().lock().await;

        let mock = MockHyprland::start().await;
        let ctx = mock.default_context();

        // Clear any prior suppression from sibling tests.
        clear_suppression().await;

        reposition_to_default(&ctx, "0xtest").await.unwrap();

        // Both move + resize must have been dispatched.
        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter().any(|c| c.contains("resizewindowpixel") && c.contains("0xtest")),
            "expected resize dispatch: {cmds:?}"
        );
        assert!(
            cmds.iter().any(|c| c.contains("movewindowpixel") && c.contains("0xtest")),
            "expected move dispatch: {cmds:?}"
        );

        // Suppress file should hold a recent (positive) timestamp. The shared
        // on-disk path may be racing with parallel tests — tolerate transient
        // mid-write empty reads by polling briefly.
        let path = get_suppress_file_path();
        let mut got_nonzero = false;
        for _ in 0..10 {
            let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            if let Ok(ts) = content.trim().parse::<u64>()
                && ts > 0
            {
                got_nonzero = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert!(
            got_nonzero,
            "reposition_to_default must write a non-zero suppress timestamp"
        );
    }

    /// Verify try_mpv_paths returns None when no socket responds — sanity
    /// check on the shared retry helper.
    ///
    /// Holds the async env-mutex across the await so a parallel
    /// `MPV_IPC_SOCKET`-mutating test (e.g.
    /// `send_mpv_ipc_command_succeeds_with_real_socket`) cannot rewrite the
    /// var mid-flight and confuse our assertion.
    #[tokio::test]
    async fn try_mpv_paths_returns_none_when_no_socket_works() {
        let _g = super::async_env_test_mutex().lock().await;
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

    /// Retry-exhaustion path: when every candidate mpv socket fails,
    /// `send_mpv_ipc_command` must surface the typed
    /// [`crate::error::MediaControlError::MpvIpc`] variant (NoSocket
    /// kind) — not a generic IO error or success. Guards the failure
    /// contract callers like `chapter` and `seek` rely on.
    ///
    /// Skips when the host has a live socket at one of the hardcoded
    /// fallback paths (`/tmp/mpv-shim` or `/tmp/mpvctl-jshim`) — the test
    /// can't simulate a no-socket world if a real mpv-shim is running on
    /// the dev box. Asserting against a pre-existing socket would either
    /// succeed spuriously or send a no-op `script-message` to the user's
    /// real mpv instance.
    #[tokio::test]
    async fn send_mpv_ipc_command_returns_no_socket_when_all_paths_fail() {
        use crate::error::{MediaControlError, MpvIpcErrorKind};

        let _g = super::async_env_test_mutex().lock().await;

        // Bail if any of the hardcoded fallback sockets are live on this host.
        if is_unix_socket(Path::new(MPV_IPC_SOCKET_DEFAULT))
            || is_unix_socket(Path::new(MPV_IPC_SOCKET_FALLBACK))
        {
            eprintln!(
                "skipping: host has a live mpv socket at one of {MPV_IPC_SOCKET_DEFAULT:?}/{MPV_IPC_SOCKET_FALLBACK:?}"
            );
            return;
        }

        let original = env::var("MPV_IPC_SOCKET").ok();

        // Point env at a path that does not exist. With the host-socket
        // skip above, all three candidates should fail and we should see
        // NoSocket.
        unsafe {
            set_env(
                "MPV_IPC_SOCKET",
                "/tmp/mpc-audit-nonexistent-socket-91827465",
            );
        }

        let result = send_mpv_ipc_command(r#"{"command":["no-op"]}"#).await;

        // SAFETY: restore env before any assert that might panic.
        unsafe {
            if let Some(v) = original {
                set_env("MPV_IPC_SOCKET", &v);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }

        match result {
            Err(MediaControlError::MpvIpc { kind, .. }) => {
                assert_eq!(
                    kind,
                    MpvIpcErrorKind::NoSocket,
                    "expected NoSocket; got {kind:?}"
                );
            }
            Ok(()) => panic!("send_mpv_ipc_command should fail when no socket exists"),
            Err(e) => panic!("expected MpvIpc/NoSocket; got {e:?}"),
        }
    }
}
