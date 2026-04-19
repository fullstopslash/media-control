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
        // Use the existing `From<HyprlandError>` bridge so the typed source
        // chain (env-var name, IO error, etc.) is preserved end-to-end
        // instead of being flattened into a stringified `NotFound`.
        let hyprland = HyprlandClient::new()?;
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
        .find(|c| c.is_focused())
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

/// Get the runtime directory from `$XDG_RUNTIME_DIR`.
///
/// Sanitizes the env value to defend against path-traversal injection:
/// the path must be absolute, contain no `..` components, and exist as a
/// directory.
///
/// # Errors
///
/// Returns [`MediaControlError::InvalidArgument`] when `XDG_RUNTIME_DIR`
/// is unset, empty, relative, contains `..`, or does not point to an
/// existing directory. Falling back to `/tmp` would be world-writable and
/// would expose every derived path (suppress file, minify state) to
/// symlink attacks; we refuse to do so.
pub fn runtime_dir() -> Result<PathBuf> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static MISSING_WARNED: AtomicBool = AtomicBool::new(false);

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
        return Ok(dir);
    }
    if !MISSING_WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "XDG_RUNTIME_DIR is required (must be absolute, free of `..`, and an existing directory); refusing to fall back to /tmp"
        );
    }
    Err(crate::error::MediaControlError::invalid_argument(
        "XDG_RUNTIME_DIR is required: must be set to an absolute, existing directory path with no `..` components",
    ))
}

/// Get the path to the avoider suppress file.
///
/// The suppress file is located at `$XDG_RUNTIME_DIR/media-avoider-suppress`.
/// When this file exists and contains a recent timestamp, the avoider daemon
/// will skip repositioning to prevent feedback loops.
///
/// # Errors
///
/// Propagates the error from [`runtime_dir`] when `XDG_RUNTIME_DIR` is
/// unset or invalid.
pub fn get_suppress_file_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("media-avoider-suppress"))
}

/// Write a value to the suppress file atomically.
///
/// Writes to `<path>.tmp` in the same directory, then atomically renames
/// into place. This guarantees a concurrent reader either sees the previous
/// contents or the full new contents — never an empty mid-write file that
/// would parse as zero and disable suppression. Logs on failure.
async fn write_suppress_file(content: &str) {
    let path = match get_suppress_file_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("cannot write suppress file: {e}");
            return;
        }
    };
    // `<path>.tmp` lives in the same directory (and therefore the same
    // filesystem) as `path`, so `rename` is a single atomic syscall on
    // POSIX. Using a fixed sibling name means a crashed run leaves at most
    // one stale `.tmp` artifact rather than an unbounded set.
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    if let Err(e) = fs::write(&tmp, content).await {
        tracing::debug!("failed to write suppress file (tmp): {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path).await {
        tracing::debug!("failed to rename suppress file into place: {e}");
        // Best-effort cleanup so we don't leave the tmp artifact behind.
        let _ = fs::remove_file(&tmp).await;
    }
}

/// Write a timestamp to the suppress file to prevent avoider repositioning.
///
/// The avoider daemon checks this file before repositioning. If the timestamp
/// is recent (within the configured timeout), it skips the reposition operation.
/// This prevents feedback loops when commands intentionally move windows.
pub async fn suppress_avoider() {
    write_suppress_file(&now_unix_millis().to_string()).await;
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

// ---------------------------------------------------------------------------
// Bare-action helpers
//
// Each helper returns the bare Hyprland action body — e.g. `pin address:0xabc`,
// `movewindowpixel exact 100 200,address:0xabc` — *without* the leading
// `dispatch ` token. Pair with [`HyprlandClient::dispatch`] (single) or
// [`HyprlandClient::dispatch_batch`] (multiple), both of which prepend the
// `dispatch ` token themselves. This keeps the literal `dispatch ` in exactly
// one place per code path and lets the same helper feed both single and batch
// dispatch sites.
// ---------------------------------------------------------------------------

/// `focuswindow address:<addr>` action.
#[inline]
pub(crate) fn focus_window_action(addr: &str) -> String {
    format!("focuswindow address:{addr}")
}

/// `pin address:<addr>` action.
#[inline]
pub(crate) fn pin_action(addr: &str) -> String {
    format!("pin address:{addr}")
}

/// `togglefloating address:<addr>` action.
#[inline]
pub(crate) fn toggle_floating_action(addr: &str) -> String {
    format!("togglefloating address:{addr}")
}

/// `closewindow address:<addr>` action.
#[inline]
pub(crate) fn close_window_action(addr: &str) -> String {
    format!("closewindow address:{addr}")
}

/// `movewindowpixel exact <x> <y>,address:<addr>` action.
#[inline]
pub(crate) fn move_pixel_action(addr: &str, x: i32, y: i32) -> String {
    format!("movewindowpixel exact {x} {y},address:{addr}")
}

/// `resizewindowpixel exact <w> <h>,address:<addr>` action.
#[inline]
pub(crate) fn resize_pixel_action(addr: &str, w: i32, h: i32) -> String {
    format!("resizewindowpixel exact {w} {h},address:{addr}")
}

/// Convert a slice of `String`s into the `&[&str]` shape `HyprlandClient::batch` wants.
///
/// Centralises the noisy `.iter().map(String::as_str).collect()` pattern so
/// callers building dynamic batch lists read uniformly.
#[inline]
pub(crate) fn as_str_refs(cmds: &[String]) -> Vec<&str> {
    cmds.iter().map(String::as_str).collect()
}

/// Current wall-clock time in milliseconds since UNIX epoch.
///
/// Returns `0` when the system clock is before `UNIX_EPOCH` (impossible on a
/// healthy system; preserves the prior `unwrap_or_default()` semantics).
/// Saturates at [`u64::MAX`] in the year-584-million case so callers can
/// drop their `try_from(...).unwrap_or(u64::MAX)` boilerplate.
#[inline]
pub(crate) fn now_unix_millis() -> u64 {
    let raw = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(raw).unwrap_or(u64::MAX)
}

/// Restore focus to a window without warping the cursor.
///
/// Tries modern `cursor:no_warps` syntax first, falls back to legacy
/// `general:no_cursor_warps` for older Hyprland versions.
///
/// If the modern batch fails partway, the trailing `cursor:no_warps false`
/// reset may never have fired, leaving warps disabled until something else
/// clears it. Before falling back, we emit the reset unconditionally so
/// the modern keyword can never be left stuck on `true`. The reset is
/// cheap and idempotent, so issuing it on the (rare) success-then-failure
/// edge is harmless.
pub async fn restore_focus(ctx: &CommandContext, addr: &str) -> Result<()> {
    // Build the focuswindow batch entry once, in `dispatch`-prefixed form so
    // it can ride alongside the `keyword` lines in `batch()` (which doesn't
    // auto-prefix).
    let focus = format!("dispatch {}", focus_window_action(addr));

    let result = ctx
        .hyprland
        .batch(&[
            "keyword cursor:no_warps true",
            &focus,
            "keyword cursor:no_warps false",
        ])
        .await;

    if result.is_err() {
        // Best-effort cleanup: ensure the modern keyword is reset to
        // `false` even if the batch above failed mid-flight (succeeded at
        // setting `true` but never reached the trailing `false`). Errors
        // here are intentionally swallowed — the legacy retry below is
        // what the caller is actually waiting on. `batch` with a single
        // element is the right primitive: `keyword …` is NOT a dispatch
        // action, so the `dispatch_batch` / `dispatch` helpers (which
        // auto-prefix `dispatch `) would mangle it.
        if let Err(e) = ctx
            .hyprland
            .batch(&["keyword cursor:no_warps false"])
            .await
        {
            tracing::debug!("modern cursor:no_warps reset on fallback failed: {e}");
        }

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
///
/// # Errors
///
/// Propagates the error from [`runtime_dir`] when `XDG_RUNTIME_DIR` is
/// unset or invalid.
pub fn get_minify_state_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("media-control-minified"))
}

/// Check if minified mode is active.
///
/// Returns `false` when `XDG_RUNTIME_DIR` is unavailable — the safe
/// default, since minified is an opt-in mode and we never want a missing
/// env var to silently flip the window into the scaled branch.
pub fn is_minified() -> bool {
    get_minify_state_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Toggle minified mode on/off. Returns the new state.
pub async fn toggle_minified() -> Result<bool> {
    let path = get_minify_state_path()?;
    if path.exists() {
        fs::remove_file(&path).await?;
        Ok(false)
    } else {
        fs::write(&path, "1").await?;
        Ok(true)
    }
}

/// Compute scaled (width, height) for the minified branch.
///
/// Defends against pathological config (NaN, negative, or huge `minified_scale`)
/// by clamping the scaled value into a sane range before converting back to
/// `i32`. Without this, an `f32 → i32` saturating-cast on `NaN` or out-of-range
/// values would yield `0` / `i32::MAX` and propagate into geometry math.
fn scaled_dims(w: i32, h: i32, raw_scale: f32) -> (i32, i32) {
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

/// Get the effective window dimensions, accounting for minified mode.
///
/// Filesystem-stat shortcut wrapper around
/// [`effective_dimensions_with_minified`]. Prefer the `_with_minified`
/// variant when the caller already has the bool — repeated stats add up
/// in hot paths like the avoider.
pub fn effective_dimensions(ctx: &CommandContext) -> (i32, i32) {
    effective_dimensions_with_minified(ctx, is_minified())
}

/// Variant of [`effective_dimensions`] that takes a precomputed `minified`
/// flag, avoiding a redundant filesystem stat in callers that already
/// computed it (or that need to call this alongside
/// [`resolve_effective_position_with_minified`]).
#[inline]
pub(crate) fn effective_dimensions_with_minified(
    ctx: &CommandContext,
    minified: bool,
) -> (i32, i32) {
    let w = ctx.config.positions.width;
    let h = ctx.config.positions.height;
    if !minified {
        return (w, h);
    }
    scaled_dims(w, h, ctx.config.positioning.minified_scale)
}

/// Resolve a position name adjusted for minified mode, falling back to `default` when unset.
///
/// Convenience wrapper that captures the `resolve(...).unwrap_or(default)` pattern
/// repeated across `avoid.rs`, `move_window.rs`, and `mod.rs::reposition_to_default`.
#[inline]
pub(crate) fn resolve_position_or(ctx: &CommandContext, name: &str, default: i32) -> i32 {
    resolve_effective_position(ctx, name).unwrap_or(default)
}

/// Variant of [`resolve_position_or`] that takes a precomputed `minified`
/// flag — see [`effective_dimensions_with_minified`].
#[inline]
pub(crate) fn resolve_position_or_with_minified(
    ctx: &CommandContext,
    name: &str,
    default: i32,
    minified: bool,
) -> i32 {
    resolve_effective_position_with_minified(ctx, name, minified).unwrap_or(default)
}

/// Resolve a position name adjusted for minified mode.
///
/// When minified, "x_right" and "y_bottom" shift outward because the
/// smaller window needs a larger x/y to maintain the same gap from the
/// screen edge. "x_left" and "y_top" stay the same.
pub fn resolve_effective_position(ctx: &CommandContext, name: &str) -> Option<i32> {
    resolve_effective_position_with_minified(ctx, name, is_minified())
}

/// Variant of [`resolve_effective_position`] that takes a precomputed
/// `minified` flag — see [`effective_dimensions_with_minified`].
#[inline]
pub(crate) fn resolve_effective_position_with_minified(
    ctx: &CommandContext,
    name: &str,
    minified: bool,
) -> Option<i32> {
    let raw = ctx.config.resolve_position(name)?;
    if !minified {
        return Some(raw);
    }
    let p = &ctx.config.positions;
    let (ew, eh) = scaled_dims(p.width, p.height, ctx.config.positioning.minified_scale);
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
    // Compute the minified flag once: each `resolve_effective_position` /
    // `effective_dimensions` call would otherwise stat the same minify
    // marker file. With four reads in this function alone, the redundant
    // syscalls are non-trivial — the `_with_minified` variants thread the
    // bool through so we pay for one stat instead of four.
    let minified = is_minified();
    // Fall back through `resolve_effective_position_with_minified` so the
    // fallback name is also adjusted for minified mode — using the raw
    // config value here would bypass the minified offset and place the
    // window incorrectly.
    let target_x = resolve_effective_position_with_minified(ctx, &positioning.default_x, minified)
        .unwrap_or_else(|| {
            resolve_position_or_with_minified(ctx, "x_right", ctx.config.positions.x_right, minified)
        });
    let target_y = resolve_effective_position_with_minified(ctx, &positioning.default_y, minified)
        .unwrap_or_else(|| {
            resolve_position_or_with_minified(
                ctx,
                "y_bottom",
                ctx.config.positions.y_bottom,
                minified,
            )
        });
    let (ew, eh) = effective_dimensions_with_minified(ctx, minified);

    // Suppress immediately before dispatch. Idempotent w.r.t. an earlier
    // caller-side suppress — both writes set a fresh timestamp.
    suppress_avoider().await;

    ctx.hyprland
        .dispatch_batch(&[
            &resize_pixel_action(addr, ew, eh),
            &move_pixel_action(addr, target_x, target_y),
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

/// Validate an mpv IPC socket path candidate sourced from env input.
///
/// Mirrors the constraints applied to `XDG_RUNTIME_DIR` in [`runtime_dir`]:
/// the path must be absolute and contain no `..` components. Existence is
/// NOT checked here (the socket may legitimately not yet exist when the
/// caller probes the candidate list); the lstat-based [`is_unix_socket`]
/// downstream catches that case.
///
/// # Errors
///
/// Returns [`MediaControlError::InvalidArgument`] when the supplied
/// `MPV_IPC_SOCKET` is empty, relative, or contains `..` components.
fn validate_mpv_socket_path(raw: &str) -> Result<PathBuf> {
    if raw.is_empty() {
        return Err(crate::error::MediaControlError::invalid_argument(
            "MPV_IPC_SOCKET must not be empty",
        ));
    }
    let p = PathBuf::from(raw);
    if !p.is_absolute() {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "MPV_IPC_SOCKET must be an absolute path (got {raw:?})"
        )));
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "MPV_IPC_SOCKET must not contain `..` components (got {raw:?})"
        )));
    }
    Ok(p)
}

/// Get the ordered list of mpv IPC socket paths to try.
///
/// `MPV_IPC_SOCKET` (when set) is validated through
/// [`validate_mpv_socket_path`] — the same constraints `runtime_dir`
/// applies to `XDG_RUNTIME_DIR`. An invalid value is logged once and
/// dropped from the candidate list rather than poisoning the whole call;
/// the hardcoded fallbacks still get a chance.
fn mpv_socket_paths() -> Vec<String> {
    let mut paths = Vec::with_capacity(3);
    if let Ok(env_path) = env::var("MPV_IPC_SOCKET") {
        match validate_mpv_socket_path(&env_path) {
            Ok(p) => {
                // `from(PathBuf).to_string_lossy().into_owned()` would
                // smuggle non-UTF8 bytes through with `?` placeholders; the
                // validator already rejected unusable shapes, so a direct
                // String round-trip is safe here.
                if let Some(s) = p.to_str() {
                    paths.push(s.to_string());
                } else {
                    tracing::warn!(
                        "MPV_IPC_SOCKET contains non-UTF8 bytes; ignoring and using fallbacks"
                    );
                }
            }
            Err(e) => {
                tracing::warn!("ignoring invalid MPV_IPC_SOCKET: {e}");
            }
        }
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

    // Distinguish the three failure modes in the log so a wedged-mpv
    // condition (timeout) is observable from the same log stream as a
    // closed-socket condition (EOF) or a malformed line that bubbled all
    // the way out (parse). The public return shape is preserved
    // (`Ok(None)`) so callers don't have to learn three new error
    // variants — the diagnostic lives in the trace, not the type.
    match read_result {
        Ok(Ok(val)) => Ok(Some(val)),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            tracing::debug!(
                "mpv {socket_path}: EOF before response (mpv closed the connection)"
            );
            Ok(None)
        }
        Ok(Err(e)) => {
            tracing::debug!("mpv {socket_path}: read error: {e}");
            Ok(None)
        }
        Err(_) => {
            tracing::debug!(
                "mpv {socket_path}: response read timed out after {}ms (socket alive but mpv hung)",
                SOCKET_RESPONSE_TIMEOUT.as_millis()
            );
            Ok(None)
        }
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

/// Validate that a CLI-supplied IPC token does not exceed `max_len` bytes.
///
/// Returns [`crate::error::MediaControlError::InvalidArgument`] (not
/// `MpvIpc/ConnectionFailed`) when the cap is exceeded — the connection was
/// never attempted, the input was rejected. Centralised so every IPC-bound
/// CLI argument enforces the cap identically and emits the same error shape
/// for callers/tests.
#[inline]
pub(crate) fn validate_ipc_token_len(label: &str, value: &str, max_len: usize) -> Result<()> {
    if value.len() > max_len {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "{label} too long: {} bytes (max {max_len})",
            value.len()
        )));
    }
    Ok(())
}

/// Try each candidate mpv socket path in turn, with optional retry passes.
///
/// Returns the first `Some(T)` produced by any (path, attempt) pair; if
/// all combinations fail, emits a single `tracing::debug!` listing every
/// attempted path so the caller's "no socket" error has actionable context
/// in the logs.
///
/// # Iteration order
///
/// `for path { for attempt { … } }` — exhaust retries on each path before
/// moving to the next. The other order (`for attempt { for path { … } }`)
/// burns every retry on a wedged path 1 before path 2 is ever tried,
/// turning a stale-but-present socket into a long latency spike for a
/// caller that only needs the live fallback. With this ordering, a stale
/// path 1 still gives the live path 2 the very first attempt on its first
/// pass — at the cost of `RETRY_DELAY` between attempts on the same
/// path, which is the desired latency budget for a single bad endpoint.
///
/// # Timing
///
/// Per-path timeouts are applied inside `op`, not here, so total wall time
/// scales as `paths.len() * (retries + 1) * <op timeout> + paths.len() *
/// retries * RETRY_DELAY`. With the default 50ms connect + 50ms response
/// budget, 3 paths, and `retries=1`, the worst case is
/// `3 * 2 * 100ms + 3 * 1 * 25ms = 675ms`.
async fn try_mpv_paths<T, F, Fut>(retries: u8, mut op: F) -> Option<T>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let paths = mpv_socket_paths();
    for path in &paths {
        for attempt in 0..=retries {
            if attempt > 0 {
                tokio::time::sleep(RETRY_DELAY).await;
            }
            // Clone the owned String to sidestep the
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
        u32::from(retries) + 1,
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

    #[tokio::test]
    async fn suppress_file_path_uses_xdg_runtime_dir() {
        let _g = super::async_env_test_mutex().lock().await;
        // Save original value
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // Use a real existing directory so the validator inside
        // `runtime_dir` accepts it. Pre-fix this used `/run/user/1000`
        // which doesn't exist in CI sandboxes.
        let dir = tempfile::tempdir().unwrap();
        let dir_str = dir.path().to_str().unwrap();

        // SAFETY: Test is single-threaded and restores the original value
        unsafe {
            set_env("XDG_RUNTIME_DIR", dir_str);
            let path = get_suppress_file_path().expect("XDG_RUNTIME_DIR is valid");
            assert_eq!(path, dir.path().join("media-avoider-suppress"));

            // Restore original value
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    #[tokio::test]
    async fn suppress_file_path_errors_when_runtime_dir_missing() {
        let _g = super::async_env_test_mutex().lock().await;
        // Save original value
        let original = env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: Test is single-threaded and restores the original value
        unsafe {
            // The env var is required — there is no `/tmp` fallback any more.
            remove_env("XDG_RUNTIME_DIR");
            let result = get_suppress_file_path();
            assert!(
                matches!(
                    result,
                    Err(crate::error::MediaControlError::InvalidArgument(_))
                ),
                "expected InvalidArgument when XDG_RUNTIME_DIR is unset, got {result:?}"
            );

            // Restore original value
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            }
        }
    }

    #[tokio::test]
    async fn suppress_avoider_writes_file() {
        let _g = super::async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();

        // SAFETY: single-threaded test
        unsafe {
            set_env("XDG_RUNTIME_DIR", dir.path().to_str().unwrap());
        }

        suppress_avoider().await;
        let path = get_suppress_file_path().unwrap();
        assert!(path.exists(), "suppress file should exist at {path:?}");

        // SAFETY: restore
        unsafe {
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    #[tokio::test]
    async fn clear_suppression_writes_file() {
        let _g = super::async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();

        // SAFETY: single-threaded test
        unsafe {
            set_env("XDG_RUNTIME_DIR", dir.path().to_str().unwrap());
        }

        clear_suppression().await;
        let path = get_suppress_file_path().unwrap();
        assert!(path.exists(), "suppress file should exist at {path:?}");

        // SAFETY: restore
        unsafe {
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    /// Atomicity: a concurrent reader must never observe an empty (mid-write)
    /// suppress file. The fixed `write_suppress_file` writes to `<path>.tmp`
    /// then renames; the rename is a single syscall, so any read either
    /// returns the prior content or the full new content.
    #[tokio::test]
    async fn write_suppress_file_is_atomic() {
        let _g = super::async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();

        // SAFETY: single-threaded test
        unsafe {
            set_env("XDG_RUNTIME_DIR", dir.path().to_str().unwrap());
        }

        // Pre-seed a known value so an empty read would be obviously wrong.
        let path = get_suppress_file_path().unwrap();
        tokio::fs::write(&path, "12345").await.unwrap();

        // Race a reader against a writer. Each iteration overwrites and reads
        // back; the read must always parse to a positive integer (never empty).
        let writer = async {
            for i in 0..50 {
                write_suppress_file(&format!("{}", 1_000_000 + i)).await;
            }
        };
        let reader = async {
            for _ in 0..200 {
                if let Ok(s) = tokio::fs::read_to_string(&path).await {
                    let trimmed = s.trim();
                    assert!(
                        !trimmed.is_empty(),
                        "atomic write must never expose empty file"
                    );
                    let _: u64 = trimmed.parse().unwrap_or_else(|_| {
                        panic!("read non-numeric content during write: {trimmed:?}")
                    });
                }
                tokio::task::yield_now().await;
            }
        };
        tokio::join!(writer, reader);

        // SAFETY: restore
        unsafe {
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
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
            .filter(|c| c.is_focused())
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
    #[tokio::test]
    async fn runtime_dir_rejects_relative_path() {
        let _g = super::async_env_test_mutex().lock().await;
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
        let _g = super::async_env_test_mutex().lock().await;
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
        let _g = super::async_env_test_mutex().lock().await;
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
