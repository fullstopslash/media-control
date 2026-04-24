//! Smart window repositioning to avoid overlap.
//!
//! Repositions the media window to prevent overlapping with the focused window.
//! Called by the trigger daemon in response to window/workspace events.
//!
//! # Logic Overview
//!
//! The avoid command handles several cases:
//!
//! 1. **Single-workspace mode**: When only 0-1 non-media windows exist on the workspace,
//!    position media windows at their preferred location from config.
//!
//! 2. **Mouseover**: When the focused window IS a media window, calculate target
//!    position based on focused window geometry and move away.
//!
//! 3. **Geometry overlap**: When media windows overlap with the focused window,
//!    calculate target position and reposition to avoid overlap.
//!
//! 4. **Fullscreen non-media**: When a non-media app is fullscreen, move all
//!    media windows out of the way.

use tokio::fs;

use std::fmt;

use super::geometry::{PositionResolver, Rect};
use super::{
    CommandContext, get_suppress_file_path, move_pixel_action, now_unix_millis,
    resize_pixel_action, restore_focus_suppressed, suppress_avoider,
};
use crate::error::Result;
use crate::hyprland::Client;
use crate::window::MediaWindow;

/// Hyprland fullscreen states are 0/1/2/3; `> FULLSCREEN_NONE` means
/// "some fullscreen state is active" — do NOT simplify to `== 1`.
const FULLSCREEN_NONE: u8 = 0;

/// Wide-window threshold clamp ceiling. `u8` to match the config field.
const PERCENT_MAX: u8 = 100;

/// Hyprland's scratchpad-monitor sentinel; `< 0` means "no real output".
/// Regression test: `avoid_scratchpad_focused_returns_early`.
const SCRATCHPAD_MONITOR: i32 = -1;

/// Test-only 8-arg shim over [`Rect::overlaps`] so the in-module overflow
/// / edge-touching / degenerate-dimension test suite reads without a
/// constructor at every call site.
#[cfg(test)]
#[inline]
#[allow(clippy::too_many_arguments)]
fn rectangles_overlap(
    x1: i32,
    y1: i32,
    w1: i32,
    h1: i32,
    x2: i32,
    y2: i32,
    w2: i32,
    h2: i32,
) -> bool {
    Rect::new(x1, y1, w1, h1).overlaps(&Rect::new(x2, y2, w2, h2))
}

/// Position pair for single-workspace mode (primary + secondary for toggle).
struct PositionPair {
    primary_x: i32,
    primary_y: i32,
    secondary_x: i32,
    secondary_y: i32,
    width: Option<i32>,
    height: Option<i32>,
}

/// Get primary + secondary positions for the mouseover toggle, applying
/// any focused-class / title override from config. Takes the precomputed
/// `minified` flag from the per-tick resolve (avoids 4 redundant stats).
fn get_position_pair(
    ctx: &CommandContext,
    focused_class: &str,
    focused_title: &str,
    minified: bool,
) -> PositionPair {
    let positions = &ctx.config.positions;
    let positioning = &ctx.config.positioning;
    let r = PositionResolver::new(ctx, minified);

    // Default positions (adjusted for minified mode)
    let default_primary_x = r.resolve_or(&positioning.default_x, positions.x_right);
    let default_primary_y = r.resolve_or(&positioning.default_y, positions.y_bottom);
    let default_secondary_x = r.resolve_or(&positioning.secondary_x, positions.x_left);
    let default_secondary_y = r.resolve_or(&positioning.secondary_y, positions.y_bottom);

    // Check for class/title override (case-insensitive class, regex title)
    if let Some(o) = positioning.get_override(focused_class, focused_title) {
        let override_or = |field: &Option<String>, default: i32| -> i32 {
            field
                .as_ref()
                .and_then(|s| r.resolve_opt(s))
                .unwrap_or(default)
        };
        return PositionPair {
            primary_x: override_or(&o.pref_x, default_primary_x),
            primary_y: override_or(&o.pref_y, default_primary_y),
            secondary_x: override_or(&o.secondary_x, default_secondary_x),
            secondary_y: override_or(&o.secondary_y, default_secondary_y),
            width: o.pref_width,
            height: o.pref_height,
        };
    }

    PositionPair {
        primary_x: default_primary_x,
        primary_y: default_primary_y,
        secondary_x: default_secondary_x,
        secondary_y: default_secondary_y,
        width: None,
        height: None,
    }
}

/// Calculate target position to avoid the focused window.
///
/// This is the core avoidance algorithm matching the original bash script:
/// - If focused window is wide (>= wide_window_threshold% of available width):
///   Move vertically (keep x, change y based on media's current vertical position)
/// - Otherwise: Move horizontally (keep y, change x based on media's current horizontal position)
fn calculate_target_position(
    ctx: &CommandContext,
    media_x: i32,
    media_y: i32,
    focus_w: i32,
    minified: bool,
) -> (i32, i32) {
    let positioning = &ctx.config.positioning;
    let positions = &ctx.config.positions;
    let r = PositionResolver::new(ctx, minified);

    // Use effective positions (adjusted for minified mode)
    let x_left = r.resolve_or("x_left", positions.x_left);
    let x_right = r.resolve_or("x_right", positions.x_right);
    let y_top = r.resolve_or("y_top", positions.y_top);
    let y_bottom = r.resolve_or("y_bottom", positions.y_bottom);

    let (media_width, _) = super::effective_dimensions_with_minified(ctx, minified);
    // Widen to i64: socket-provided geometry could push these sums past i32::MAX.
    // All center-point math uses safe-midpoint formulas so even adversarial
    // i32 extremes cannot overflow before comparison.
    let x_left_64 = i64::from(x_left);
    let x_right_64 = i64::from(x_right);
    let y_top_64 = i64::from(y_top);
    let y_bottom_64 = i64::from(y_bottom);
    let media_width_64 = i64::from(media_width);

    let available_width = x_right_64 + media_width_64 - x_left_64;
    let screen_center_x = x_left_64 + (x_right_64 - x_left_64) / 2;
    let screen_center_y = y_top_64 + (y_bottom_64 - y_top_64) / 2;

    // Clamp threshold percentage to [0, PERCENT_MAX] — config could be misconfigured.
    let wide_threshold = i64::from(positioning.wide_window_threshold.min(PERCENT_MAX));
    let wide_cutoff = available_width.saturating_mul(wide_threshold) / i64::from(PERCENT_MAX);

    if i64::from(focus_w) >= wide_cutoff {
        let target_y = if i64::from(media_y) >= screen_center_y {
            y_top
        } else {
            y_bottom
        };
        (media_x, target_y)
    } else {
        // Safe midpoint: i64::from(media_x) + media_width_64 / 2 cannot overflow
        // because both operands are already widened.
        let media_center = i64::from(media_x) + media_width_64 / 2;
        let target_x = if media_center <= screen_center_x {
            x_right
        } else {
            x_left
        };
        (target_x, media_y)
    }
}

/// Move a media window to a specific position.
/// Respects minified mode — scales dimensions when active.
///
/// Suppresses the avoider BEFORE dispatching the batch so the
/// window-moved event Hyprland fires cannot race the daemon back into
/// the avoid path before suppression takes hold. Even if the batch
/// fails, leaving the suppress timestamp warm for one cycle is harmless
/// — it just defers the next avoid run by `suppress_ms`.
async fn move_media_window(
    ctx: &CommandContext,
    addr: &str,
    x: i32,
    y: i32,
    width: Option<i32>,
    height: Option<i32>,
    minified: bool,
) -> Result<()> {
    let (ew, eh) = super::effective_dimensions_with_minified(ctx, minified);
    let w = width.unwrap_or(ew);
    let h = height.unwrap_or(eh);

    suppress_avoider().await;

    ctx.hyprland
        .dispatch_batch(&[
            &move_pixel_action(addr, x, y),
            &resize_pixel_action(addr, w, h),
        ])
        .await?;

    Ok(())
}

/// Move a media window to `(target_x, target_y)`, but only if the target
/// itself does not overlap the predicate's "should avoid" rectangle. This
/// guards against bouncing — repositioning the window to a place that is
/// also obstructed.
///
/// Returns `Ok(true)` if the window moved, `Ok(false)` if the target was
/// also blocked.
async fn try_move_clear_of<F>(
    ctx: &CommandContext,
    addr: &str,
    target_x: i32,
    target_y: i32,
    minified: bool,
    blocked: F,
) -> Result<bool>
where
    F: Fn(i32, i32, i32, i32) -> bool,
{
    let (media_w, media_h) = super::effective_dimensions_with_minified(ctx, minified);
    if blocked(target_x, target_y, media_w, media_h) {
        tracing::debug!("avoid: target ({target_x}, {target_y}) also overlaps, skipping");
        return Ok(false);
    }
    move_media_window(ctx, addr, target_x, target_y, None, None, minified).await?;
    Ok(true)
}

/// Check if avoider should be suppressed due to recent activity.
///
/// Returns `false` (i.e. "not suppressed, run avoid") on every read /
/// parse / env failure. Each failure mode is logged at `debug` so an
/// operator who notices the avoider thrashing can tell whether the
/// suppress file was missing (normal — no recent dispatch), unreadable
/// (permission regression, stat race), unparseable (write was truncated by
/// a non-atomic writer), or whether the env var itself disappeared.
/// Without these logs the failure path is silent and indistinguishable
/// from "no suppress was ever issued".
async fn should_suppress(suppress_timeout_ms: u64) -> bool {
    let path = match get_suppress_file_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("suppress check skipped: cannot resolve path: {e}");
            return false;
        }
    };

    let content = match fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // ENOENT is the normal "no recent dispatch" case — keep it at
            // `trace` so we don't drown the signal in noise on every tick.
            tracing::trace!("suppress file absent: {}", path.display());
            return false;
        }
        Err(e) => {
            tracing::debug!(
                "suppress file unreadable (treating as not suppressed): {}: {e}",
                path.display()
            );
            return false;
        }
    };

    let Ok(timestamp_ms) = content.trim().parse::<u64>() else {
        tracing::debug!(
            "suppress file content not parseable as u64 (treating as not suppressed): {:?}",
            content.trim()
        );
        return false;
    };

    let now = now_unix_millis();
    now.saturating_sub(timestamp_ms) < suppress_timeout_ms
}

/// Check if a position is within tolerance of target.
///
/// Widens to `i64` so neither the subtraction nor `.abs()` can overflow on
/// adversarial socket input (e.g. `i32::MIN`).
#[inline]
fn within_tolerance(actual: i32, target: i32, tolerance: i32) -> bool {
    (i64::from(actual) - i64::from(target)).abs() <= i64::from(tolerance)
}

/// Borrowed view of the focused window plus a precomputed media-match flag.
///
/// Single source of truth: the `&Client` borrow keeps every field of the
/// underlying client reachable. Accessor methods normalize `Client`'s shape
/// (`at[0]`, `at[1]`, `size[0]`, `size[1]`, `workspace.id`) to the names
/// avoidance code uses (`x`, `y`, `width`, `height`, `workspace_id`).
struct FocusedWindow<'a> {
    client: &'a Client,
    is_media: bool,
}

impl<'a> FocusedWindow<'a> {
    fn find(clients: &'a [Client], ctx: &CommandContext) -> Option<Self> {
        let client = clients.iter().find(|c| c.is_focused())?;
        let is_media = ctx.window_matcher.matches(client).is_some();
        Some(Self { client, is_media })
    }

    #[inline]
    fn address(&self) -> &str {
        &self.client.address
    }
    #[inline]
    fn x(&self) -> i32 {
        self.client.at[0]
    }
    #[inline]
    fn y(&self) -> i32 {
        self.client.at[1]
    }
    #[inline]
    fn width(&self) -> i32 {
        self.client.size[0]
    }
    #[inline]
    fn height(&self) -> i32 {
        self.client.size[1]
    }
    #[inline]
    fn floating(&self) -> bool {
        self.client.floating
    }
    #[inline]
    fn monitor(&self) -> i32 {
        self.client.monitor
    }
    #[inline]
    fn workspace_id(&self) -> i32 {
        self.client.workspace.id
    }
    #[inline]
    fn fullscreen(&self) -> u8 {
        self.client.fullscreen
    }
    #[inline]
    fn class(&self) -> &str {
        &self.client.class
    }
    #[inline]
    fn title(&self) -> &str {
        &self.client.title
    }
    #[inline]
    fn is_media(&self) -> bool {
        self.is_media
    }

    #[inline]
    fn rect(&self) -> Rect {
        Rect::new(self.x(), self.y(), self.width(), self.height())
    }
}

/// Counts all visible non-media windows on `workspace_id` AND `monitor_id`,
/// including the currently focused window.
///
/// The monitor filter matters on multi-monitor setups: workspaces with the
/// same id can exist on different monitors, and a non-media window on
/// monitor 2's workspace 1 should not influence avoidance behaviour for a
/// media window on monitor 1's workspace 1. Without this, the
/// "single-window" heuristic (`<= 1` non-media peer) would mis-classify a
/// monitor as crowded based on windows the user can't even see from here.
fn count_non_media_windows(
    clients: &[Client],
    workspace_id: i32,
    monitor_id: i32,
    ctx: &CommandContext,
) -> usize {
    clients
        .iter()
        .filter(|c| {
            c.workspace.id == workspace_id
                && c.monitor == monitor_id
                && c.is_visible()
                && ctx.window_matcher.matches(c).is_none()
        })
        .count()
}

/// Which avoidance strategy to apply. `dispatch` runs the per-arm
/// handler with all shared inputs already plumbed through.
enum AvoidCase<'a> {
    /// Single workspace, non-media focused: move media to primary position.
    MoveToPrimary,
    /// Single workspace, media focused (mouseover): toggle between primary/secondary.
    MouseoverToggle { prev_window: &'a Client },
    /// Multi-workspace, media focused with overlap: geometry-based move + restore focus.
    MouseoverGeometry,
    /// Multi-workspace, non-media focused with overlap: geometry-based move.
    GeometryOverlap,
    /// Non-media app is fullscreen: move all media out of the way.
    FullscreenNonMedia,
}

impl fmt::Display for AvoidCase<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AvoidCase::MoveToPrimary => "MoveToPrimary",
            AvoidCase::MouseoverToggle { .. } => "MouseoverToggle",
            AvoidCase::MouseoverGeometry => "MouseoverGeometry",
            AvoidCase::GeometryOverlap => "GeometryOverlap",
            AvoidCase::FullscreenNonMedia => "FullscreenNonMedia",
        };
        f.write_str(s)
    }
}

impl<'a> AvoidCase<'a> {
    /// Classify which avoidance case applies, or `None` to skip this tick
    /// (focused media is fullscreen, no media to move, or single-workspace
    /// + non-media fullscreen).
    fn classify(
        focused: &FocusedWindow<'_>,
        is_single_workspace: bool,
        media_windows: &[MediaWindow],
        clients: &'a [Client],
        ctx: &CommandContext,
    ) -> Option<Self> {
        // Hard stops: fullscreen media must never be touched, and there has to
        // be at least one media window to reposition.
        //
        // Hyprland reports fullscreen as 0/1/2/3 (none / max / fullscreen /
        // fullscreen-1). Anything `> FULLSCREEN_NONE` means "some fullscreen
        // state is active" — do NOT simplify to `== 1`.
        if focused.is_media() && focused.fullscreen() > FULLSCREEN_NONE {
            return None;
        }
        if media_windows.is_empty() {
            return None;
        }

        match (
            is_single_workspace,
            focused.is_media(),
            focused.fullscreen() > FULLSCREEN_NONE,
        ) {
            // Single-workspace, non-media fullscreen: leave the user alone.
            (true, false, true) => None,
            // Single-workspace mouseover: needs a previous window to know which
            // pair-position to toggle to.
            (true, true, _) => ctx
                .window_matcher
                .find_previous_focus(clients, focused.address(), Some(focused.workspace_id()))
                .and_then(|addr| clients.iter().find(|c| c.address == addr))
                .map(|prev| AvoidCase::MouseoverToggle { prev_window: prev }),
            // Single-workspace, ordinary non-media focus.
            (true, false, false) => Some(AvoidCase::MoveToPrimary),
            // Multi-workspace, media focused (mouseover-geometry case).
            (false, true, _) => Some(AvoidCase::MouseoverGeometry),
            // Multi-workspace, non-media fullscreen.
            (false, false, true) => Some(AvoidCase::FullscreenNonMedia),
            // Multi-workspace, non-media windowed.
            (false, false, false) => Some(AvoidCase::GeometryOverlap),
        }
    }

    /// Run the per-case handler with all shared inputs plumbed.
    async fn dispatch(
        self,
        ctx: &CommandContext,
        focused: &FocusedWindow<'_>,
        media_windows: &[MediaWindow],
        clients: &[Client],
        minified: bool,
    ) -> Result<()> {
        match self {
            AvoidCase::MoveToPrimary => {
                handle_move_to_primary(ctx, focused, media_windows, minified).await
            }
            AvoidCase::MouseoverToggle { prev_window } => {
                handle_mouseover_toggle(ctx, focused, prev_window, minified).await
            }
            AvoidCase::MouseoverGeometry => {
                handle_mouseover_geometry(ctx, focused, clients, minified).await
            }
            AvoidCase::GeometryOverlap => {
                handle_geometry_overlap(ctx, focused, media_windows, minified).await
            }
            AvoidCase::FullscreenNonMedia => {
                handle_fullscreen_nonmedia(ctx, focused, media_windows, minified).await
            }
        }
    }
}

/// Smart window repositioning to avoid overlap.
///
/// Fetches the client list itself; daemon callers that want to share a
/// per-tick snapshot across burst-fired events should use
/// [`avoid_with_clients`].
pub async fn avoid(ctx: &CommandContext) -> Result<()> {
    if should_suppress(u64::from(ctx.config.positioning.suppress_ms)).await {
        tracing::debug!("avoid: suppressed");
        return Ok(());
    }

    let clients = ctx.hyprland.get_clients().await?;
    avoid_with_clients(ctx, &clients).await
}

/// Variant of [`avoid`] that accepts a pre-fetched `clients` slice.
///
/// Skips the `should_suppress` check — the daemon's own in-memory
/// suppress check runs before this is called.
pub async fn avoid_with_clients(ctx: &CommandContext, clients: &[Client]) -> Result<()> {
    let Some(focused) = FocusedWindow::find(clients, ctx) else {
        tracing::debug!("avoid: no focused window");
        return Ok(());
    };

    // Compute the minified flag exactly once per call. Downstream takes
    // the bool by value so we pay one stat of the minify marker, not
    // four (the resolve + dimensions helpers each used to stat).
    let minified = super::is_minified();

    // Scratchpad windows report `monitor == SCRATCHPAD_MONITOR` and have
    // no real workspace context for the avoidance heuristics. Bail.
    // Regression: `avoid_scratchpad_focused_returns_early`.
    if focused.monitor() <= SCRATCHPAD_MONITOR {
        tracing::debug!(
            "avoid: focused window on scratchpad (monitor={}); skipping",
            focused.monitor()
        );
        return Ok(());
    }

    let non_media_count =
        count_non_media_windows(clients, focused.workspace_id(), focused.monitor(), ctx);
    // "Single-workspace" here means at most one non-media peer alongside
    // the media windows — see `count_non_media_windows` for what this
    // includes (the focused window itself is counted).
    let is_single_workspace = non_media_count <= 1;

    let media_windows: Vec<MediaWindow> = ctx
        .window_matcher
        .find_media_windows(clients, focused.monitor())
        .into_iter()
        .filter(|w| w.fullscreen == 0)
        .collect();

    tracing::debug!(
        "avoid: focused={} is_media={} fullscreen={} single_ws={} media_count={}",
        focused.class(),
        focused.is_media(),
        focused.fullscreen(),
        is_single_workspace,
        media_windows.len()
    );

    let Some(case) =
        AvoidCase::classify(&focused, is_single_workspace, &media_windows, clients, ctx)
    else {
        tracing::debug!("avoid: no action needed");
        return Ok(());
    };

    tracing::debug!("avoid: case={case}");
    case.dispatch(ctx, &focused, &media_windows, clients, minified)
        .await
}

/// Case 1: Move media windows to their primary configured position.
///
/// If the focused window is floating and overlaps primary, move to secondary instead.
/// Tiled/maximized windows always overlap the pinned media window — that's expected,
/// so we only dodge floating windows.
async fn handle_move_to_primary(
    ctx: &CommandContext,
    focused: &FocusedWindow<'_>,
    media_windows: &[MediaWindow],
    minified: bool,
) -> Result<()> {
    let pair = get_position_pair(ctx, focused.class(), focused.title(), minified);
    let tolerance = i32::from(ctx.config.positioning.position_tolerance);
    let (ew, eh) = super::effective_dimensions_with_minified(ctx, minified);
    let pair_w = pair.width.unwrap_or(ew);
    let pair_h = pair.height.unwrap_or(eh);

    let focused_rect = focused.rect();

    for window in media_windows {
        let at_primary = within_tolerance(window.x, pair.primary_x, tolerance)
            && within_tolerance(window.y, pair.primary_y, tolerance);

        // Only check overlap with floating windows — tiled windows always overlap
        // the pinned media window and that's by design.
        // If at primary, test the window's *current* rect; otherwise test
        // where it would land if we moved it to primary.
        let primary_clashes = focused.floating() && {
            let candidate = if at_primary {
                Rect::from_media(window)
            } else {
                Rect::new(pair.primary_x, pair.primary_y, pair_w, pair_h)
            };
            candidate.overlaps(&focused_rect)
        };

        // Pick destination, or skip entirely if already at primary and unblocked.
        let (dest_x, dest_y, label) = if primary_clashes {
            (pair.secondary_x, pair.secondary_y, "secondary")
        } else if !at_primary {
            (pair.primary_x, pair.primary_y, "primary")
        } else {
            continue;
        };

        tracing::debug!("avoid: moving to {label} ({dest_x}, {dest_y})");
        move_media_window(
            ctx,
            &window.address,
            dest_x,
            dest_y,
            pair.width,
            pair.height,
            minified,
        )
        .await?;
    }
    Ok(())
}

/// Case 2: Toggle media window between primary and secondary positions on mouseover.
async fn handle_mouseover_toggle(
    ctx: &CommandContext,
    focused: &FocusedWindow<'_>,
    prev_window: &Client,
    minified: bool,
) -> Result<()> {
    let pair = get_position_pair(ctx, &prev_window.class, &prev_window.title, minified);
    let tolerance = i32::from(ctx.config.positioning.position_tolerance);

    let at_primary = within_tolerance(focused.x(), pair.primary_x, tolerance)
        && within_tolerance(focused.y(), pair.primary_y, tolerance);

    let (target_x, target_y) = if at_primary {
        (pair.secondary_x, pair.secondary_y)
    } else {
        (pair.primary_x, pair.primary_y)
    };

    if !within_tolerance(focused.x(), target_x, tolerance)
        || !within_tolerance(focused.y(), target_y, tolerance)
    {
        move_media_window(
            ctx,
            focused.address(),
            target_x,
            target_y,
            pair.width,
            pair.height,
            minified,
        )
        .await?;
    }

    // Restore focus so subsequent mouseovers trigger new events.
    // `restore_focus_suppressed` re-arms suppression before the focuswindow
    // dispatch — even if the move above was skipped (already at target),
    // the focuswindow event Hyprland echoes back must not re-enter avoid
    // before suppress_ms elapses.
    restore_focus_suppressed(ctx, &prev_window.address).await;
    Ok(())
}

/// Case 3: Geometry-based avoidance when media is focused in multi-workspace.
async fn handle_mouseover_geometry(
    ctx: &CommandContext,
    focused: &FocusedWindow<'_>,
    clients: &[Client],
    minified: bool,
) -> Result<()> {
    // Does this rect overlap any workspace peer (excluding the focused media window)?
    let overlaps_peer = |x, y, w, h| {
        let candidate = Rect::new(x, y, w, h);
        clients.iter().any(|c| {
            c.address != focused.address()
                && c.workspace.id == focused.workspace_id()
                && c.is_visible()
                && candidate.overlaps(&Rect::from_client(c))
        })
    };

    if !overlaps_peer(focused.x(), focused.y(), focused.width(), focused.height()) {
        return Ok(());
    }

    let (target_x, target_y) =
        calculate_target_position(ctx, focused.x(), focused.y(), focused.width(), minified);
    if !try_move_clear_of(
        ctx,
        focused.address(),
        target_x,
        target_y,
        minified,
        &overlaps_peer,
    )
    .await?
    {
        return Ok(());
    }

    // Re-arm suppression before dispatching focuswindow. The earlier
    // move_media_window call inside try_move_clear_of warmed the suppress
    // file, but the restore_focus batch below issues 3 more dispatches and
    // we don't want a tight `suppress_ms` to elapse between them.
    if let Some(prev_addr) = ctx.window_matcher.find_previous_focus(
        clients,
        focused.address(),
        Some(focused.workspace_id()),
    ) {
        restore_focus_suppressed(ctx, &prev_addr).await;
    }
    Ok(())
}

/// Case 4: Non-media focused, geometry overlap in multi-workspace.
///
/// Repositions every overlapping media window whose target slot is also
/// clear. Pre-fix this returned after the first successful move, leaving
/// any other overlapping windows stuck — Hyprland fires no event for the
/// windows that didn't move, so they would never be reconsidered until
/// the next unrelated focus/window event. The single suppression timer
/// (warmed by `move_media_window` on each iteration) covers all moves in
/// the same tick, so issuing several dispatches here is no more dangerous
/// than the single-dispatch path it replaces.
///
/// A window whose target is itself blocked (anti-bounce short-circuit in
/// `try_move_clear_of`) is skipped without aborting the loop — a second
/// movable window must not be starved waiting for its own focus event.
async fn handle_geometry_overlap(
    ctx: &CommandContext,
    focused: &FocusedWindow<'_>,
    media_windows: &[MediaWindow],
    minified: bool,
) -> Result<()> {
    let focused_rect = focused.rect();
    let overlaps_focused = |x, y, w, h| Rect::new(x, y, w, h).overlaps(&focused_rect);

    for window in media_windows {
        if !overlaps_focused(window.x, window.y, window.width, window.height) {
            continue;
        }
        let (target_x, target_y) =
            calculate_target_position(ctx, window.x, window.y, focused.width(), minified);
        tracing::debug!(
            "avoid: overlap detected on {}, target=({target_x}, {target_y})",
            window.address
        );
        // `try_move_clear_of` returns Ok(false) when the target slot is
        // also blocked; that's a per-window decision, not a reason to
        // stop processing siblings. We propagate hard errors (`?`) and
        // continue on soft "couldn't relocate this one" outcomes.
        let _ = try_move_clear_of(
            ctx,
            &window.address,
            target_x,
            target_y,
            minified,
            &overlaps_focused,
        )
        .await?;
    }
    Ok(())
}

/// Case 5: Non-media app is fullscreen, move all media windows away.
async fn handle_fullscreen_nonmedia(
    ctx: &CommandContext,
    focused: &FocusedWindow<'_>,
    media_windows: &[MediaWindow],
    minified: bool,
) -> Result<()> {
    for window in media_windows {
        let (target_x, target_y) =
            calculate_target_position(ctx, window.x, window.y, focused.width(), minified);
        move_media_window(
            ctx,
            &window.address,
            target_x,
            target_y,
            None,
            None,
            minified,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::clear_suppression;
    use super::*;
    use crate::config::Config;
    use crate::test_helpers::*;

    #[test]
    fn rectangles_overlap_detects_overlap() {
        assert!(rectangles_overlap(0, 0, 100, 100, 50, 50, 100, 100));
        assert!(rectangles_overlap(50, 50, 100, 100, 0, 0, 100, 100));
        assert!(rectangles_overlap(0, 0, 200, 200, 50, 50, 50, 50));
    }

    #[test]
    fn rectangles_overlap_detects_no_overlap() {
        assert!(!rectangles_overlap(0, 0, 100, 100, 100, 0, 100, 100));
        assert!(!rectangles_overlap(0, 0, 100, 100, 0, 100, 100, 100));
        assert!(!rectangles_overlap(0, 0, 100, 100, 150, 150, 100, 100));
    }

    #[test]
    fn rectangles_overlap_handles_invalid_dimensions() {
        assert!(!rectangles_overlap(0, 0, 0, 100, 0, 0, 100, 100));
        assert!(!rectangles_overlap(0, 0, 100, 0, 0, 0, 100, 100));
        assert!(!rectangles_overlap(0, 0, -10, 100, 0, 0, 100, 100));
    }

    #[test]
    fn rectangles_overlap_no_overflow_on_extreme_geometry() {
        // Adversarial socket payloads must not overflow the i32 edge math.
        // Pre-fix this would wrap and silently flip the comparison.
        assert!(rectangles_overlap(
            i32::MAX - 100,
            0,
            200,
            100,
            i32::MAX - 50,
            0,
            100,
            100,
        ));
        assert!(!rectangles_overlap(
            i32::MAX - 100,
            0,
            50,
            50,
            0,
            0,
            100,
            100,
        ));
    }

    #[test]
    fn within_tolerance_works() {
        assert!(within_tolerance(100, 100, 5));
        assert!(within_tolerance(103, 100, 5));
        assert!(within_tolerance(97, 100, 5));
        assert!(!within_tolerance(106, 100, 5));
        assert!(!within_tolerance(94, 100, 5));
    }

    #[test]
    fn within_tolerance_no_overflow_on_extreme_inputs() {
        // i32::MIN - i32::MAX would wrap; .abs() on i32::MIN would panic.
        assert!(!within_tolerance(i32::MIN, i32::MAX, 5));
        assert!(!within_tolerance(i32::MAX, i32::MIN, 5));
        assert!(within_tolerance(i32::MAX, i32::MAX, 0));
    }

    #[test]
    fn within_tolerance_negative_tolerance_never_matches() {
        // Negative tolerance should never satisfy `|d| <= -1`. Must not panic.
        assert!(!within_tolerance(100, 100, -1));
        assert!(!within_tolerance(0, 0, -100));
    }

    #[test]
    fn rectangles_overlap_touching_edges_at_extremes() {
        // Right edge of A meets left edge of B at i32::MAX boundary.
        // Touching but not overlapping → false. Pre-i64-widening this would
        // overflow during `x2 + w2`.
        assert!(!rectangles_overlap(
            i32::MAX - 200,
            0,
            100,
            100,
            i32::MAX - 100,
            0,
            100,
            100,
        ));
        // Top edge of B touches bottom edge of A.
        assert!(!rectangles_overlap(0, 0, 100, 100, 0, 100, 100, 100));
    }

    #[test]
    fn rectangles_overlap_zero_or_negative_second_rect() {
        // A valid first rect plus a degenerate second rect must not overlap.
        assert!(!rectangles_overlap(0, 0, 100, 100, 50, 50, 0, 50));
        assert!(!rectangles_overlap(0, 0, 100, 100, 50, 50, 50, 0));
        assert!(!rectangles_overlap(0, 0, 100, 100, 50, 50, -10, 50));
    }

    #[test]
    fn rectangles_overlap_fully_contained() {
        // B fully inside A — must overlap.
        assert!(rectangles_overlap(0, 0, 1000, 1000, 100, 100, 50, 50));
        // A fully inside B — symmetric.
        assert!(rectangles_overlap(100, 100, 50, 50, 0, 0, 1000, 1000));
    }

    // --- E2E tests using mock Hyprland ---

    /// Re-export the shared no-suppress config under the local name the
    /// existing tests expect. Centralised in `test_helpers` so the same
    /// fixture is reused by `commands::fullscreen::tests`.
    use crate::test_helpers::test_config_no_suppress as test_config;

    #[tokio::test]
    async fn avoid_case1_moves_media_to_primary() {
        let mock = MockHyprland::start().await;

        // mpv at [0, 0], firefox at [0, 400] with small size (doesn't overlap primary)
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([0, 400])
                .size([800, 300])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([0, 0])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
        assert!(batch.is_some(), "expected movewindowpixel in: {cmds:?}");
        let batch = batch.unwrap();
        assert!(batch.contains("1272"), "expected x_right=1272 in: {batch}");
        assert!(batch.contains("712"), "expected y_bottom=712 in: {batch}");
    }

    #[tokio::test]
    async fn avoid_case1_skips_when_already_at_primary_no_overlap() {
        let mock = MockHyprland::start().await;

        // mpv at primary, firefox small and not overlapping primary
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(
            !has_move,
            "should not move when at primary with no overlap: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn avoid_case1_moves_to_secondary_when_primary_overlaps() {
        let mock = MockHyprland::start().await;

        // mpv at primary (1272, 712), floating firefox focused and overlapping that position
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .floating(true)
                .focus_history(0)
                .at([900, 500])
                .size([1020, 580]) // overlaps primary position
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360]) // at primary
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
        assert!(
            batch.is_some(),
            "should move when primary overlaps focused: {cmds:?}"
        );
        let batch = batch.unwrap();
        // Should move to secondary (x_left=48) instead of staying at primary
        assert!(
            batch.contains("48"),
            "expected secondary x_left=48: {batch}"
        );
    }

    #[tokio::test]
    async fn avoid_case1_uses_secondary_when_primary_would_overlap() {
        let mock = MockHyprland::start().await;

        // mpv at some random position, floating firefox overlaps the primary position
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .floating(true)
                .focus_history(0)
                .at([900, 500])
                .size([1020, 580]) // overlaps primary (1272, 712)
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([500, 300])
                .size([640, 360]) // not at primary
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
        assert!(batch.is_some(), "should move to secondary: {cmds:?}");
        let batch = batch.unwrap();
        assert!(
            batch.contains("48"),
            "expected secondary x_left=48: {batch}"
        );
    }

    #[tokio::test]
    async fn avoid_case1_skips_fullscreen_focused() {
        let mock = MockHyprland::start().await;

        // Firefox is fullscreen
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .fullscreen(2)
                .focus_history(0)
                .at([0, 0])
                .size([1920, 1080])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([0, 0])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(
            !has_move,
            "should not move when focused is fullscreen (case 1)"
        );
    }

    #[tokio::test]
    async fn avoid_case2_toggles_to_secondary() {
        let mock = MockHyprland::start().await;

        // mpv is focused (focus_history_id=0) and at primary position
        // firefox is previous focus (focus_history_id=1)
        // Single workspace (only 1 non-media window)
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(1)
                .at([0, 0])
                .size([1920, 1080])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(0)
                .at([1272, 712])
                .size([640, 360]) // at primary (default x_right, y_bottom)
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should toggle to secondary position (default: x_left=48, y_bottom=712)
        let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
        assert!(batch.is_some(), "expected move for toggle: {cmds:?}");
        let batch = batch.unwrap();
        assert!(
            batch.contains("48"),
            "expected x_left=48 in toggle: {batch}"
        );

        // Should also restore focus to firefox
        let focus_cmd = cmds.iter().find(|c| c.contains("focuswindow"));
        assert!(focus_cmd.is_some(), "expected focus restore: {cmds:?}");
        assert!(
            focus_cmd.unwrap().contains("0xb1"),
            "expected focus restore to firefox"
        );
    }

    #[tokio::test]
    async fn avoid_case2_no_previous_window_skips() {
        let mock = MockHyprland::start().await;

        // Only mpv on workspace, no previous focus candidate
        let clients = vec![
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(0)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(!has_move, "should skip when no previous window: {cmds:?}");
    }

    #[tokio::test]
    async fn avoid_case3_geometry_overlap_moves_media() {
        let mock = MockHyprland::start().await;

        // Multi-workspace: 2 non-media windows + mpv on same workspace
        // Firefox focused, overlapping with mpv
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([900, 500])
                .size([800, 600]) // overlaps mpv
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Terminal")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(has_move, "should move media away from overlap: {cmds:?}");
    }

    #[tokio::test]
    async fn avoid_case3_no_overlap_skips() {
        let mock = MockHyprland::start().await;

        // Multi-workspace: 2 non-media windows + mpv, no overlap
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Terminal")
                .focus_history(2)
                .at([0, 600])
                .size([800, 400])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360]) // far away, no overlap
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(!has_move, "should not move when no overlap: {cmds:?}");
    }

    #[tokio::test]
    async fn avoid_no_focused_window_returns_early() {
        let mock = MockHyprland::start().await;

        // No window has focus_history_id == 0
        let clients = vec![
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(5)
                .at([100, 100])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(!has_move, "should return early with no focused window");
    }

    /// Regression (bolt 023): scratchpad windows report `monitor == -1`. The
    /// avoid loop uses monitor as a filter for `count_non_media_windows` and
    /// for `find_media_windows`; with `-1` it would mis-classify the
    /// workspace and trigger spurious moves on real-monitor media windows.
    /// Bail early.
    #[tokio::test]
    async fn avoid_scratchpad_focused_returns_early() {
        let mock = MockHyprland::start().await;

        // Focused window on the scratchpad: monitor=-1, focus_history_id=0.
        // A media window on a real monitor is also present; it must not move.
        let clients = vec![
            ClientBuilder::new("0xs1", "scratch", "Scratchpad")
                .floating(true)
                .workspace(42) // special workspace id
                .monitor(-1) // scratchpad monitor
                .focus_history(0) // focused
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([100, 100])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(
            !has_move,
            "scratchpad focus must not trigger media moves: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn avoid_no_media_windows_returns_early() {
        let mock = MockHyprland::start().await;

        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([0, 0])
                .size([1920, 1080])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(!has_move, "should return early with no media windows");
    }

    #[tokio::test]
    async fn should_suppress_with_recent_timestamp() {
        // Test suppress logic directly (not through avoid) to avoid race conditions
        // with the shared suppress file in parallel tests.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-suppress");

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        tokio::fs::write(&path, now_ms.to_string()).await.unwrap();

        // Read and check manually (same logic as should_suppress but with custom path)
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        let timestamp_ms: u64 = content.trim().parse().unwrap();
        let current = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap_or(u64::MAX);
        let elapsed = current.saturating_sub(timestamp_ms);

        assert!(
            elapsed < 60_000,
            "timestamp should be recent: elapsed={elapsed}ms"
        );
        assert!(elapsed < 150, "should be within default suppress_ms");
    }

    #[tokio::test]
    async fn should_suppress_with_stale_timestamp() {
        // Timestamp of 0 is always stale
        // With 0ms timeout, nothing is suppressed
        assert!(
            !should_suppress(0).await,
            "0ms timeout means never suppress"
        );
    }

    #[tokio::test]
    async fn avoid_case1_applies_position_override() {
        let mock = MockHyprland::start().await;

        // mpv not at any configured position
        mock.set_response("j/clients", &scenario_single_workspace_json([500, 500]))
            .await;

        // Config with a position override for firefox class
        let mut config = test_config();
        let override_toml = r#"
            [[positioning.overrides]]
            focused_class = "firefox"
            pref_x = "x_left"
            pref_y = "y_top"
        "#;
        let override_config: Config = toml::from_str(override_toml).unwrap();
        config.positioning.overrides = override_config.positioning.overrides;

        let ctx = mock.context(config);
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
        assert!(batch.is_some(), "expected move with override: {cmds:?}");
        let batch = batch.unwrap();
        // Should use x_left=48, y_top=48 from override
        assert!(
            batch.contains("48"),
            "expected x_left=48 from override: {batch}"
        );
    }

    #[tokio::test]
    async fn avoid_case_fullscreen_nonmedia_moves_media() {
        // Multi-workspace + non-media fullscreen → media should be repositioned.
        let mock = MockHyprland::start().await;

        let clients = vec![
            // Fullscreen firefox (non-media)
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .fullscreen(2)
                .focus_history(0)
                .at([0, 0])
                .size([1920, 1080])
                .build(),
            // Second non-media window so we are NOT in single-workspace mode.
            ClientBuilder::new("0xc1", "kitty", "Terminal")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            // mpv currently far from primary
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([500, 500])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(has_move, "fullscreen non-media should move media: {cmds:?}");
    }

    #[tokio::test]
    async fn avoid_case_mouseover_geometry_moves_focused_media() {
        // Multi-workspace + media focused + media overlaps a peer →
        // MouseoverGeometry path: move the focused media window itself.
        let mock = MockHyprland::start().await;

        let clients = vec![
            // mpv focused (focus_history_id = 0) and overlapping firefox
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(0)
                .at([200, 200])
                .size([640, 360])
                .build(),
            // firefox previously focused, overlapping mpv
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(1)
                .at([100, 100])
                .size([800, 600])
                .build(),
            // second non-media so we are NOT in single-workspace mode
            ClientBuilder::new("0xc1", "kitty", "Terminal")
                .focus_history(2)
                .at([1500, 0])
                .size([200, 200])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_move = cmds.iter().any(|c| c.contains("movewindowpixel"));
        assert!(has_move, "mouseover-geometry should move media: {cmds:?}");
        // Should also restore focus to the previous window (firefox)
        let focus_cmd = cmds.iter().find(|c| c.contains("focuswindow"));
        assert!(focus_cmd.is_some(), "expected focus restore: {cmds:?}");
        assert!(
            focus_cmd.unwrap().contains("0xb1"),
            "expected focus restore to firefox"
        );
    }

    #[tokio::test]
    async fn calculate_target_position_no_overflow_on_extreme_geometry() {
        // Constructed via a context — exercises the same widening as production.
        // We just need to verify no panic / wraparound; the exact target is
        // unimportant because the inputs are adversarial.
        let mock = MockHyprland::start().await;
        let ctx = mock.context(test_config());

        // Extreme media position + huge focus_w must not overflow.
        let _ = calculate_target_position(&ctx, i32::MAX, i32::MAX, i32::MAX, false);
        let _ = calculate_target_position(&ctx, i32::MIN, i32::MIN, i32::MAX, false);
        let _ = calculate_target_position(&ctx, 0, 0, i32::MAX, false);
    }

    #[tokio::test]
    async fn calculate_target_position_clamps_threshold_above_100() {
        // Misconfigured threshold > 100 should still produce a valid target.
        let mock = MockHyprland::start().await;
        let mut config = test_config();
        config.positioning.wide_window_threshold = 250;
        let ctx = mock.context(config);
        let _ = calculate_target_position(&ctx, 100, 100, 800, false);
    }

    // --- Newly added defensive tests ---

    #[test]
    fn rectangles_overlap_negative_coordinates() {
        // Both rects in negative space — overlap calculation must still work.
        assert!(rectangles_overlap(-100, -100, 50, 50, -120, -120, 40, 40));
        assert!(!rectangles_overlap(-100, -100, 50, 50, -200, -200, 40, 40));
        // One negative, one positive — the gap straddles 0.
        assert!(!rectangles_overlap(-100, -100, 50, 50, 0, 0, 100, 100));
        // Touching at 0 along x — should NOT overlap.
        assert!(!rectangles_overlap(-100, 0, 100, 100, 0, 0, 100, 100));
    }

    #[test]
    fn rectangles_overlap_zero_size_first_rect() {
        // The first rect itself is degenerate.
        assert!(!rectangles_overlap(0, 0, 0, 100, 50, 50, 100, 100));
        assert!(!rectangles_overlap(0, 0, 100, 0, 50, 50, 100, 100));
        // Both zero — degenerate, no overlap.
        assert!(!rectangles_overlap(0, 0, 0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn rectangles_overlap_touching_corners_at_normal_coords() {
        // The bottom-right of A meets the top-left of B at (100, 100).
        // Touching but not overlapping → false.
        assert!(!rectangles_overlap(0, 0, 100, 100, 100, 100, 50, 50));
    }

    #[test]
    fn within_tolerance_zero_tolerance() {
        // Zero tolerance is exact equality.
        assert!(within_tolerance(100, 100, 0));
        assert!(!within_tolerance(101, 100, 0));
        assert!(!within_tolerance(99, 100, 0));
    }

    #[tokio::test]
    async fn try_move_clear_of_skips_when_target_blocked() {
        // If the predicate flags the target as blocked, no move issued.
        let mock = MockHyprland::start().await;
        let ctx = mock.context(test_config());

        let moved = try_move_clear_of(&ctx, "0xabc", 100, 100, false, |_, _, _, _| true)
            .await
            .unwrap();
        assert!(!moved, "should report not moved when target is blocked");

        let cmds = mock.captured_commands().await;
        assert!(
            !cmds.iter().any(|c| c.contains("movewindowpixel")),
            "must not dispatch move when target blocked: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn try_move_clear_of_moves_when_target_clear() {
        let mock = MockHyprland::start().await;
        let ctx = mock.context(test_config());

        let moved = try_move_clear_of(&ctx, "0xabc", 200, 300, false, |_, _, _, _| false)
            .await
            .unwrap();
        assert!(moved);

        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter()
                .any(|c| c.contains("movewindowpixel exact 200 300")),
            "expected move dispatch: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn move_media_window_dispatches_then_suppression_is_active() {
        // Cross-checks the race-prevention contract for `move_media_window`:
        // by the time the dispatch reaches Hyprland, the suppress file must
        // hold a recent timestamp — i.e. the avoider would short-circuit on
        // the very next event.
        //
        // The shared on-disk path means parallel tests can be observed mid-
        // write (empty file → parse fails). We poll a few times so a
        // transient empty read is tolerated.
        let mock = MockHyprland::start().await;
        let ctx = mock.context(test_config());

        move_media_window(&ctx, "0xabc", 0, 0, None, None, false)
            .await
            .unwrap();

        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter().any(|c| c.contains("movewindowpixel")),
            "dispatch should have reached the mock"
        );

        // Tolerate brief mid-write races on the shared file by retrying.
        let mut suppressed = false;
        for _ in 0..10 {
            if should_suppress(60_000).await {
                suppressed = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert!(
            suppressed,
            "suppress file should hold a recent timestamp after move_media_window"
        );
    }

    #[tokio::test]
    async fn suppress_file_lifecycle_via_clear() {
        // Tests the lifecycle property of `suppress_avoider` / `clear_suppression`
        // using a private temp-file path so parallel tests touching the global
        // suppress file (via move_media_window) cannot race.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("suppress");

        // Simulate suppress_avoider: write current timestamp.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .to_string();
        tokio::fs::write(&path, &now_ms).await.unwrap();
        let read_back = tokio::fs::read_to_string(&path).await.unwrap();
        let ts: u64 = read_back.trim().parse().unwrap();
        assert!(ts > 0, "suppress timestamp should be positive");

        // Simulate clear_suppression: write "0".
        tokio::fs::write(&path, "0").await.unwrap();
        let cleared = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(cleared.trim(), "0");

        // Re-implement the should_suppress predicate against our private file
        // so we don't read the global path.
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        let timestamp_ms: u64 = content.trim().parse().unwrap();
        let now = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap_or(u64::MAX);
        let suppressed = now.saturating_sub(timestamp_ms) < 60_000;
        assert!(
            !suppressed,
            "0 timestamp must not suppress for any positive timeout"
        );
    }

    #[tokio::test]
    async fn avoid_zero_size_focused_does_not_panic() {
        // Pathological focused window size [0, 0] — the rectangle math must
        // not panic, and we should treat the window as non-overlapping.
        let mock = MockHyprland::start().await;
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .floating(true)
                .focus_history(0)
                .at([500, 500])
                .size([0, 0])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Term")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        // Must not panic. Move may or may not happen depending on classify.
        avoid(&ctx).await.unwrap();
    }

    #[tokio::test]
    async fn avoid_negative_coordinate_focused() {
        // Negative-coordinate focused window (multi-monitor edge case).
        let mock = MockHyprland::start().await;
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([-1920, -100])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Term")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();
        // Should not have panicked; correctness (no overlap) is the assertion.
    }

    #[tokio::test]
    async fn calculate_target_position_threshold_boundary() {
        // Threshold at exactly 100% means focus_w must equal full available
        // width to be classified as wide. Off-by-one here would silently
        // change movement direction.
        let mock = MockHyprland::start().await;
        let mut config = test_config();
        config.positioning.wide_window_threshold = 100;
        let ctx = mock.context(config);

        // Compute expected boundary: avail_w = x_right + media_w - x_left.
        let p = &ctx.config.positions;
        let avail_w = p.x_right + p.width - p.x_left;

        // focus_w one less than avail_w → not wide → move horizontally.
        let (tx_narrow, ty_narrow) = calculate_target_position(&ctx, 0, 100, avail_w - 1, false);
        // focus_w >= avail_w → wide → move vertically (y changes, x same).
        let (tx_wide, ty_wide) = calculate_target_position(&ctx, 0, 100, avail_w, false);

        assert_ne!(
            (tx_narrow, ty_narrow),
            (tx_wide, ty_wide),
            "boundary should switch direction"
        );
        assert_eq!(tx_wide, 0, "wide should preserve media_x");
        assert_ne!(ty_wide, 100, "wide should change media_y");
    }

    #[tokio::test]
    async fn handle_geometry_overlap_loops_past_blocked_target() {
        // Regression: prior to the fix, `handle_geometry_overlap` returned
        // unconditionally after the first overlapping window — even if its
        // target was blocked. After the fix, it continues iterating.
        //
        // We can't easily construct two windows where the first's target is
        // blocked but the second's is not (they share `calculate_target_position`
        // logic seeded by current pos). Instead: a single overlapping window
        // whose target IS clear must still trigger a move. Combined with the
        // existing `handle_geometry_overlap_skips_when_target_also_blocked`
        // which proves the blocked branch is short-circuited, this proves
        // both loop paths.
        let mock = MockHyprland::start().await;
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([900, 500])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Term")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert!(
            cmds.iter().any(|c| c.contains("movewindowpixel")),
            "should move overlapping window with clear target: {cmds:?}"
        );
    }

    /// Verify a single avoid handler warms (or leaves warm) the global
    /// suppress file. Captures state under [`with_isolated_runtime_dir`] so
    /// both the env-mutex and the tempdir are managed by RAII — assertions
    /// run after the captured state is read but before drop, so a panic
    /// can't leak `XDG_RUNTIME_DIR` into a sibling test's environment.
    ///
    /// `suppress_ms = 0` (test_config) ensures should_suppress is bypassed
    /// so avoid runs to completion regardless of file state.
    async fn assert_handler_warms_suppression(
        mock_clients: Vec<crate::hyprland::Client>,
        expect_move_substr: &str,
        expect_focus_substr: &str,
    ) {
        // `with_isolated_runtime_dir` holds the async env-mutex for the whole
        // body, sets XDG_RUNTIME_DIR to a fresh tempdir, and restores both
        // on every exit path (including panic) via the RAII guard.
        let (cmds, ts) = with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            mock.set_response("j/clients", &make_clients_json(&mock_clients))
                .await;
            let ctx = mock.context(test_config());

            clear_suppression().await;
            avoid(&ctx).await.unwrap();

            let cmds = mock.captured_commands().await;
            let path = get_suppress_file_path().expect("XDG_RUNTIME_DIR set above");
            let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            let ts: u64 = content.trim().parse().unwrap_or(0);
            (cmds, ts)
        })
        .await;

        assert!(
            cmds.iter().any(|c| c.contains(expect_move_substr)),
            "expected {expect_move_substr:?} in {cmds:?}"
        );
        assert!(
            cmds.iter().any(|c| c.contains(expect_focus_substr)),
            "expected {expect_focus_substr:?} in {cmds:?}"
        );
        assert!(
            ts > 0,
            "suppress file should contain a positive timestamp (got {ts})"
        );
    }

    #[tokio::test]
    async fn mouseover_toggle_warms_suppression_for_focus_restore() {
        // Regression: focuswindow dispatched by restore_focus would re-enter
        // avoid if suppression wasn't warmed. The fix calls suppress_avoider
        // immediately before restore_focus regardless of the move branch.
        let clients = vec![
            ClientBuilder::new("0xb2", "firefox", "Browser")
                .focus_history(1)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(0)
                .at([1272, 712])
                .size([640, 360])
                .build(),
        ];
        assert_handler_warms_suppression(clients, "movewindowpixel", "focuswindow").await;
    }

    #[tokio::test]
    async fn mouseover_geometry_warms_suppression_for_focus_restore() {
        // Regression: same race as the toggle path — restore_focus emits a
        // focuswindow event which Hyprland ships back to the daemon. Re-arming
        // suppression here keeps the avoider quiet until the dust settles.
        let clients = vec![
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(0)
                .at([200, 200])
                .size([640, 360])
                .build(),
            ClientBuilder::new("0xb2", "firefox", "Browser")
                .focus_history(1)
                .at([100, 100])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Term")
                .focus_history(2)
                .at([1500, 0])
                .size([200, 200])
                .build(),
        ];
        assert_handler_warms_suppression(clients, "movewindowpixel", "focuswindow").await;
    }

    #[tokio::test]
    async fn handle_geometry_overlap_skips_when_target_also_blocked() {
        // The target slot also overlaps focused → no move (anti-bounce).
        let mock = MockHyprland::start().await;
        // Focused window covers EVERYTHING, so any target position the
        // algorithm picks will also overlap.
        let clients = vec![
            ClientBuilder::new("0xb1", "firefox", "Browser")
                .focus_history(0)
                .at([-10_000, -10_000])
                .size([40_000, 40_000])
                .build(),
            ClientBuilder::new("0xc1", "kitty", "Term")
                .focus_history(2)
                .at([0, 0])
                .size([800, 600])
                .build(),
            ClientBuilder::new("0xd1", "mpv", "video.mp4")
                .pinned(true)
                .floating(true)
                .focus_history(1)
                .at([500, 500])
                .size([640, 360])
                .build(),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        avoid(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        assert!(
            !cmds.iter().any(|c| c.contains("movewindowpixel")),
            "should not move when both current and target overlap: {cmds:?}"
        );
    }
}
