//! Window-management commands and their internal helpers.
//!
//! Hosts every command that touches a Hyprland window — fullscreen toggle,
//! directional move, pin/float, minify, focus, close, and the avoider —
//! plus the dispatch-string builders, suppress-file machinery, minify-state
//! plumbing, and effective-position resolver they share.

pub mod avoid;
pub mod close;
pub mod focus;
pub mod fullscreen;
pub(crate) mod geometry;
pub mod minify;
pub mod move_window;
pub mod pin;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::fs;

use super::shared::{CommandContext, runtime_dir};
use crate::error::Result;
use crate::hyprland::Client;
use crate::window::MediaWindow;

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

/// Debug-only assertion: `addr` must be empty or pass the canonical
/// `^0x[0-9A-Fa-f]{1,32}$` shape enforced by
/// [`crate::hyprland::is_valid_address`].
///
/// Production callers receive addresses through the `Client::address`
/// deserialiser which already replaces malformed values with `""`; the
/// assert exists to catch regressions in tests where a stray literal could
/// otherwise smuggle `;dispatch exec ...` into an interpolated IPC string.
///
/// We do NOT short-circuit the dispatch when the address is empty —
/// Hyprland treats `address:` with no value as a no-op, which is the same
/// behaviour the deserialiser-side validation guarantees for hostile input.
#[inline]
fn assert_valid_addr(addr: &str) {
    debug_assert!(
        addr.is_empty() || crate::hyprland::is_valid_address(addr),
        "window address must be empty or match ^0x[0-9A-Fa-f]{{1,32}}$: {addr}"
    );
}

/// `focuswindow address:<addr>` action.
#[inline]
pub(crate) fn focus_window_action(addr: &str) -> String {
    assert_valid_addr(addr);
    format!("focuswindow address:{addr}")
}

/// `pin address:<addr>` action.
#[inline]
pub(crate) fn pin_action(addr: &str) -> String {
    assert_valid_addr(addr);
    format!("pin address:{addr}")
}

/// `togglefloating address:<addr>` action.
#[inline]
pub(crate) fn toggle_floating_action(addr: &str) -> String {
    assert_valid_addr(addr);
    format!("togglefloating address:{addr}")
}

/// `closewindow address:<addr>` action.
#[inline]
pub(crate) fn close_window_action(addr: &str) -> String {
    assert_valid_addr(addr);
    format!("closewindow address:{addr}")
}

/// `movewindowpixel exact <x> <y>,address:<addr>` action.
#[inline]
pub(crate) fn move_pixel_action(addr: &str, x: i32, y: i32) -> String {
    assert_valid_addr(addr);
    format!("movewindowpixel exact {x} {y},address:{addr}")
}

/// `resizewindowpixel exact <w> <h>,address:<addr>` action.
#[inline]
pub(crate) fn resize_pixel_action(addr: &str, w: i32, h: i32) -> String {
    assert_valid_addr(addr);
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
        if let Err(e) = ctx.hyprland.batch(&["keyword cursor:no_warps false"]).await {
            tracing::warn!(
                "cursor:no_warps cleanup batch failed during restore_focus fallback: {e}; \
                 cursor:no_warps may be stuck on `true` until another command resets it"
            );
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

/// Re-arm avoider suppression and restore focus to `addr`, swallowing any
/// `restore_focus` failure with a `warn!`.
///
/// Captures the pattern repeated by the avoider's two focus-restoring code
/// paths (mouseover-toggle and mouseover-geometry): `suppress_avoider`
/// must fire immediately before the `focuswindow` dispatch so the event
/// Hyprland echoes back is short-circuited by `should_suppress`. The
/// failure swallow is intentional — the caller is on a best-effort
/// reconciliation path; a missing focus restore is a UX glitch, not a hard
/// error worth aborting the avoid tick over.
pub(crate) async fn restore_focus_suppressed(ctx: &CommandContext, addr: &str) {
    suppress_avoider().await;
    if let Err(e) = restore_focus(ctx, addr).await {
        tracing::warn!("media-control: failed to restore focus: {e}");
    }
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
    get_minify_state_path().map(|p| p.exists()).unwrap_or(false)
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
/// Defends against pathological config (NaN, negative, or out-of-range
/// `minified_scale`) by clamping the scale factor to the same `(0.0, 1.0]`
/// bound the config validator already enforces, then clamping the scaled
/// pixel value before converting back to `i32`. Without the upper bound,
/// an `f32 → i32` saturating-cast on `NaN` or out-of-range values would
/// yield `0` / `i32::MAX` and propagate into geometry math; without
/// matching the config bound, a config-validator drift could let the
/// minified branch silently *upscale* the window.
fn scaled_dims(w: i32, h: i32, raw_scale: f32) -> (i32, i32) {
    debug_assert!(
        raw_scale > 0.0 && raw_scale <= 1.0,
        "minified_scale must be in (0.0, 1.0]; got {raw_scale}"
    );
    let scale = if raw_scale.is_finite() {
        raw_scale.clamp(0.0, 1.0)
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
///
/// Stats the minified marker file once and forwards to
/// [`reposition_to_default_with_minified`]. Callers that need to control the
/// minified flag explicitly (e.g. `commands::minify`, which moves the window
/// to its **post-toggle** geometry before flipping the on-disk flag) should
/// call the `_with_minified` variant directly.
pub(crate) async fn reposition_to_default(ctx: &CommandContext, addr: &str) -> Result<()> {
    // Compute the minified flag once: each `resolve_effective_position` /
    // `effective_dimensions` call would otherwise stat the same minify
    // marker file. With four reads in this function alone, the redundant
    // syscalls are non-trivial — the `_with_minified` variants thread the
    // bool through so we pay for one stat instead of four.
    reposition_to_default_with_minified(ctx, addr, is_minified()).await
}

/// Variant of [`reposition_to_default`] that takes an explicit `minified`
/// flag instead of stat'ing the marker file.
///
/// Required by `commands::minify` which needs to reposition to the
/// **post-toggle** geometry while the on-disk flag still reflects the
/// pre-toggle state — only flipping the flag once the dispatch succeeds.
/// Passing the bool through avoids both the redundant stat and the
/// pre/post-toggle ambiguity.
pub(crate) async fn reposition_to_default_with_minified(
    ctx: &CommandContext,
    addr: &str,
    minified: bool,
) -> Result<()> {
    let positioning = &ctx.config.positioning;
    // Fall back through `resolve_effective_position_with_minified` so the
    // fallback name is also adjusted for minified mode — using the raw
    // config value here would bypass the minified offset and place the
    // window incorrectly.
    let target_x = resolve_effective_position_with_minified(ctx, &positioning.default_x, minified)
        .unwrap_or_else(|| {
            resolve_position_or_with_minified(
                ctx,
                "x_right",
                ctx.config.positions.x_right,
                minified,
            )
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

#[cfg(test)]
mod tests {
    use super::super::shared::async_env_test_mutex;
    use super::*;
    use crate::config::Config;
    use crate::hyprland::HyprlandClient;
    use std::env;

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
        let _g = async_env_test_mutex().lock().await;
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
        let _g = async_env_test_mutex().lock().await;
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
        let _g = async_env_test_mutex().lock().await;
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
        let _g = async_env_test_mutex().lock().await;
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
        let _g = async_env_test_mutex().lock().await;
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
    fn get_media_window_with_clients_uses_focus_from_clients() {
        use crate::config::Pattern;
        use crate::hyprland::{Client, Workspace};
        use crate::window::{Priority, WindowMatcher};

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
        assert_eq!(media.priority, Priority::Pinned); // Pinned beats focused non-media
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
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded test
        unsafe {
            set_env("XDG_RUNTIME_DIR", "tmp/runtime");
            let result = runtime_dir();
            assert!(
                matches!(
                    result,
                    Err(crate::error::MediaControlError::InvalidArgument(_))
                ),
                "relative path must be rejected, got {result:?}"
            );
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
            let result = runtime_dir();
            assert!(
                matches!(
                    result,
                    Err(crate::error::MediaControlError::InvalidArgument(_))
                ),
                "parent-dir traversal must be rejected, got {result:?}"
            );
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
            let result = runtime_dir();
            assert!(
                matches!(
                    result,
                    Err(crate::error::MediaControlError::InvalidArgument(_))
                ),
                "nonexistent path must be rejected, got {result:?}"
            );
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }
    }

    /// Security: ensure `runtime_dir()` rejects an unset env var entirely
    /// rather than silently falling back to `/tmp` (which is world-writable
    /// on most systems and exposes derived paths to symlink attacks).
    #[tokio::test]
    async fn runtime_dir_rejects_missing_env_var() {
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded
        unsafe {
            remove_env("XDG_RUNTIME_DIR");
            let result = runtime_dir();
            assert!(
                matches!(
                    result,
                    Err(crate::error::MediaControlError::InvalidArgument(_))
                ),
                "unset XDG_RUNTIME_DIR must be rejected, got {result:?}"
            );
            if let Some(val) = original {
                set_env("XDG_RUNTIME_DIR", &val);
            }
        }
    }

    /// `runtime_socket_path` (Hyprland helper) must reject env vars whose
    /// instance signature contains separators or `..`.
    #[tokio::test]
    async fn runtime_socket_path_rejects_traversal_in_signature() {
        use crate::hyprland::runtime_socket_path;

        let _g = async_env_test_mutex().lock().await;
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded test, restored at end
        unsafe {
            // Use a real existing dir so runtime_dir part passes validation
            set_env("XDG_RUNTIME_DIR", "/tmp");

            for bad in &["../escape", "a/b", ".hidden", "..", ""] {
                set_env("HYPRLAND_INSTANCE_SIGNATURE", bad);
                assert!(
                    runtime_socket_path(".socket.sock").await.is_err(),
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

        let _g = async_env_test_mutex().lock().await;
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded
        unsafe {
            set_env("XDG_RUNTIME_DIR", "relative/path");
            set_env("HYPRLAND_INSTANCE_SIGNATURE", "valid_sig");
            assert!(runtime_socket_path(".socket.sock").await.is_err());

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

        let _g = async_env_test_mutex().lock().await;
        let orig_runtime = env::var("XDG_RUNTIME_DIR").ok();
        let orig_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();

        // SAFETY: single-threaded test, restored at end
        unsafe {
            set_env("XDG_RUNTIME_DIR", "/tmp");
            set_env("HYPRLAND_INSTANCE_SIGNATURE", "valid_sig");

            // Names that must be rejected.
            for bad in &["", "..", ".", "../escape", "a/b", "a\\b", "x\0y"] {
                assert!(
                    runtime_socket_path(bad).await.is_err(),
                    "name {bad:?} must be rejected"
                );
            }

            // Sanity: the real socket names callers actually use must pass
            // validation. Resolution itself may fall back to the env hint
            // (returning Ok) since /tmp/hypr/valid_sig may not exist;
            // both Ok and a non-name-validation Err are acceptable here —
            // we only assert the name is not the rejection cause.
            for good in &[".socket.sock", ".socket2.sock"] {
                let res = runtime_socket_path(good).await;
                // If it errors, it must be due to env/resolution, not name.
                if let Err(e) = &res {
                    let msg = format!("{e}");
                    assert!(
                        !msg.contains("HYPRLAND_INSTANCE_SIGNATURE")
                            || !msg.contains("invalid environment variable"),
                        "name {good:?} should not be rejected as unsafe; got {e}"
                    );
                }
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
        let _g = async_env_test_mutex().lock().await;

        // Provide a valid XDG_RUNTIME_DIR — the new `runtime_dir`
        // contract refuses to invent one for us. A per-test tempdir
        // sidesteps the cross-test races on the shared system path.
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let runtime = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test, restored at end
        unsafe {
            set_env("XDG_RUNTIME_DIR", runtime.path().to_str().unwrap());
        }

        let mock = MockHyprland::start().await;
        let ctx = mock.default_context();

        // Clear any prior suppression from sibling tests.
        clear_suppression().await;

        reposition_to_default(&ctx, "0xdead").await.unwrap();

        // Both move + resize must have been dispatched.
        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter()
                .any(|c| c.contains("resizewindowpixel") && c.contains("0xdead")),
            "expected resize dispatch: {cmds:?}"
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("movewindowpixel") && c.contains("0xdead")),
            "expected move dispatch: {cmds:?}"
        );

        // Suppress file should hold a recent (positive) timestamp. With the
        // per-test tempdir there are no parallel writers, but we still poll
        // a few iterations in case fs flush is delayed.
        let path = get_suppress_file_path().expect("XDG_RUNTIME_DIR set above");
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

        // SAFETY: restore env before the assert (which can panic)
        unsafe {
            if let Some(v) = original {
                set_env("XDG_RUNTIME_DIR", &v);
            } else {
                remove_env("XDG_RUNTIME_DIR");
            }
        }

        assert!(
            got_nonzero,
            "reposition_to_default must write a non-zero suppress timestamp"
        );
    }
}
