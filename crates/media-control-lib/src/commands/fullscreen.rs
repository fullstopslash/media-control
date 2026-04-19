//! Toggle fullscreen with focus restoration and pin state preservation.
//!
//! This command toggles fullscreen mode on the media window while preserving
//! the pin state and restoring focus to the previously focused window.

use regex::Regex;

use super::{
    CommandContext, as_str_refs, clear_suppression, focus_window_action,
    get_media_window_with_clients, pin_action, restore_focus, suppress_avoider,
    toggle_floating_action,
};
use crate::error::Result;
use crate::hyprland::Client;
use crate::window::MediaWindow;

/// Maximum retry attempts when exiting fullscreen.
const MAX_FULLSCREEN_EXIT_ATTEMPTS: u8 = 3;

/// Bare `fullscreen 0` action — toggles fullscreen on the currently focused
/// window (the dispatcher does not accept an `address:` selector). Centralised
/// so the literal lives in exactly one place across the enter/exit/retry paths.
const FULLSCREEN_TOGGLE: &str = "fullscreen 0";

/// Check if a window title matches a Picture-in-Picture pattern.
///
/// Uses case-insensitive regex `picture.*picture` to match variants like
/// "Picture-in-Picture", "picture-in-picture", "Picture in Picture", etc.
pub fn is_pip_title(title: &str) -> bool {
    static PIP_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PIP_REGEX
        .get_or_init(|| Regex::new(r"(?i)picture.*picture").expect("valid regex"))
        .is_match(title)
}

/// Toggle fullscreen for the media window.
///
/// Behavior:
/// - If not fullscreen and `always_pin` is set but window is unpinned: pin it instead
/// - If fullscreen: exit fullscreen, restore pin state, restore previous focus
/// - If not fullscreen: enter fullscreen (temporarily unpin if pinned)
///
/// # Errors
///
/// Returns an error if Hyprland IPC fails.
pub async fn fullscreen(ctx: &CommandContext) -> Result<()> {
    let clients = ctx.hyprland.get_clients().await?;
    let Some(media) = get_media_window_with_clients(ctx, &clients) else {
        return Ok(());
    };

    let is_fullscreen = media.fullscreen > 0;

    // Auto-pin if configured and unpinned (only when not fullscreen)
    if !is_fullscreen && media.always_pin && !media.pinned {
        return auto_pin_window(ctx, &media).await;
    }

    if is_fullscreen {
        exit_fullscreen(ctx, &media, &clients).await
    } else {
        enter_fullscreen_mode(ctx, &media).await
    }
}

/// Auto-pin a window that has always_pin set.
///
/// Makes the window floating first if needed, then pins it. When both ops
/// are required, batches them so the window doesn't briefly appear tiled
/// before being pinned.
async fn auto_pin_window(ctx: &CommandContext, media: &MediaWindow) -> Result<()> {
    let addr = &media.address;

    // Suppress BEFORE the dispatch — pinwindow / movewindow events would
    // otherwise race the daemon and trigger an avoid pass on the just-pinned
    // window before the user even sees it settle.
    suppress_avoider().await;

    if media.floating {
        ctx.hyprland.dispatch(&pin_action(addr)).await?;
    } else {
        ctx.hyprland
            .dispatch_batch(&[&toggle_floating_action(addr), &pin_action(addr)])
            .await?;
    }
    Ok(())
}

/// Enter fullscreen mode.
///
/// Focuses the media window, temporarily unpins if pinned, then goes fullscreen.
/// Uses batch commands to make operations atomic and avoid race conditions.
async fn enter_fullscreen_mode(ctx: &CommandContext, media: &MediaWindow) -> Result<()> {
    // Build batch commands to execute atomically
    // Note: fullscreen dispatcher doesn't accept address selector, it operates on focused window
    // So we must focus first, then fullscreen, all in one batch to avoid race conditions
    let mut cmds: Vec<String> = Vec::with_capacity(3);

    // 1. Focus the media window
    cmds.push(focus_window_action(&media.address));

    // 2. Temporarily unpin if pinned (fullscreen windows cannot be pinned)
    if media.pinned {
        cmds.push(pin_action(&media.address));
    }

    // 3. Toggle fullscreen (operates on the now-focused window)
    cmds.push(FULLSCREEN_TOGGLE.to_string());

    // Suppress BEFORE the batch — the activewindow + fullscreen events
    // arrive within the daemon's debounce window, so we have to beat them.
    suppress_avoider().await;

    // Execute all commands atomically
    ctx.hyprland.dispatch_batch(&as_str_refs(&cmds)).await?;

    Ok(())
}

/// Delay to give Hyprland time to process a fullscreen state change before we
/// check whether it took effect. Hyprland queues compositor events, and
/// querying `j/clients` immediately after [`FULLSCREEN_TOGGLE`] often returns
/// stale state — especially for PiP windows whose parent process (e.g. Firefox)
/// triggers additional geometry events.
const FULLSCREEN_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// Read the current fullscreen state of `addr`.
///
/// Returns `Ok(state)` on success and `Err(())` if the IPC call failed —
/// callers treat the latter as "exited" so the retry loop bails instead of
/// hammering the socket during a compositor transition.
///
/// **Missing-window semantics**: when the address is not present in the
/// client snapshot, this returns `Ok(0)` — i.e. "not fullscreen". This is
/// intentional: if the media window disappeared mid-exit (closed by the user
/// or torn down by the app) there is nothing left to unfullscreen, so the
/// retry loop should bail. `restore_after_fullscreen_exit` already guards on
/// `media_window.is_some()` and skips the dependent restore steps in that case.
async fn read_fullscreen_state(ctx: &CommandContext, addr: &str) -> std::result::Result<u8, ()> {
    match ctx.hyprland.get_clients().await {
        Ok(clients) => Ok(clients
            .iter()
            .find(|c| c.address == *addr)
            .map(|c| c.fullscreen)
            .unwrap_or(0)),
        Err(e) => {
            tracing::warn!(
                "get_clients failed during fullscreen exit check (treating as exited): {e}"
            );
            Err(())
        }
    }
}

/// Drive Hyprland out of fullscreen for `addr` with bounded retries.
///
/// Issues an initial `focus + fullscreen 0` batch (already done by caller),
/// then polls `fullscreen` state and re-toggles up to `MAX_FULLSCREEN_EXIT_ATTEMPTS`
/// times. The final attempt fires three toggles in one batch to force-flush
/// stuck state in the compositor (odd toggle count lands on "off").
async fn drive_exit_fullscreen(ctx: &CommandContext, addr: &str) -> Result<()> {
    let focus = focus_window_action(addr);
    let mut attempt = 0;
    while attempt < MAX_FULLSCREEN_EXIT_ATTEMPTS {
        let Ok(current_fs) = read_fullscreen_state(ctx, addr).await else {
            break;
        };
        if current_fs == 0 {
            break;
        }

        attempt += 1;
        // Refresh suppression before retry — the next batch will emit events
        // the daemon would otherwise pick up.
        suppress_avoider().await;

        // Final attempt: triple-toggle to unstick wedged compositor state.
        // Each `fullscreen 0` is a toggle; 3 toggles from "on" lands on "off"
        // with two intermediate flips that often clear stuck state.
        let is_final = attempt == MAX_FULLSCREEN_EXIT_ATTEMPTS;
        let retry_result = if is_final {
            ctx.hyprland
                .dispatch_batch(&[
                    &focus,
                    FULLSCREEN_TOGGLE,
                    FULLSCREEN_TOGGLE,
                    FULLSCREEN_TOGGLE,
                ])
                .await
        } else {
            ctx.hyprland
                .dispatch_batch(&[&focus, FULLSCREEN_TOGGLE])
                .await
        };
        if let Err(e) = retry_result {
            tracing::warn!("fullscreen exit retry {attempt} failed (non-fatal): {e}");
            break;
        }

        if is_final {
            // After the triple-toggle final attempt, verify it actually
            // produced state 0. A persistent nonzero state is a signal of
            // Hyprland misbehavior worth surfacing in logs — restore still
            // runs (the window might be partially exited and a re-pin /
            // reposition is still useful), but we want the warning visible.
            // We also surface IPC failures here: silently swallowing them
            // would hide a compositor that's actively dropping requests
            // mid-recovery, which is exactly when we want diagnostics.
            tokio::time::sleep(FULLSCREEN_SETTLE_DELAY).await;
            match read_fullscreen_state(ctx, addr).await {
                Ok(s) if s != 0 => tracing::warn!(
                    "fullscreen exit triple-toggle did not produce state 0; state={s}"
                ),
                Err(()) => tracing::warn!(
                    "could not verify fullscreen state after triple-toggle (IPC failure)"
                ),
                _ => {}
            }
        } else {
            tokio::time::sleep(FULLSCREEN_SETTLE_DELAY).await;
        }
    }
    Ok(())
}

/// Exit fullscreen with retry logic, pin restoration, and focus restoration.
async fn exit_fullscreen(
    ctx: &CommandContext,
    media: &MediaWindow,
    clients: &[Client],
) -> Result<()> {
    let addr = &media.address;
    let should_restore_pin = media.always_pin || media.pinned || is_pip_title(&media.title);
    let previous_focus = ctx.window_matcher.find_previous_focus(clients, addr, None);
    // Suppress avoider BEFORE starting - prevents repositioning during state changes
    suppress_avoider().await;

    // Focus the media window and toggle fullscreen atomically
    // Note: fullscreen dispatcher doesn't accept address selector, operates on focused window
    let focus = focus_window_action(addr);
    ctx.hyprland
        .dispatch_batch(&[&focus, FULLSCREEN_TOGGLE])
        .await?;

    // Give Hyprland time to process the fullscreen state change before polling.
    // Without this delay, the first get_clients() often sees stale fullscreen=2
    // (especially for Firefox PiP), triggering unnecessary retries and hammering
    // the socket during a compositor transition.
    tokio::time::sleep(FULLSCREEN_SETTLE_DELAY).await;

    drive_exit_fullscreen(ctx, addr).await?;

    restore_after_fullscreen_exit(ctx, media, should_restore_pin, previous_focus.as_deref()).await
}

/// Best-effort restore steps after the fullscreen-exit toggle has settled:
/// fresh client snapshot → re-pin if needed → reposition → restore focus →
/// trigger an explicit avoid with fresh state.
///
/// Each step is non-fatal: a failure only logs and skips the dependent work.
/// We can't rely on the avoider daemon to fix positioning because the move
/// and focus events we emit fall inside its debounce window.
///
/// `original_media` is the pre-exit `MediaWindow` snapshot. We use it to
/// detect address recycling — Hyprland window addresses are heap pointers,
/// so if the original window died mid-exit and a freshly-spawned window
/// happened to land at the same address, restoring pin/position to it would
/// corrupt an unrelated window. The class check below catches that.
async fn restore_after_fullscreen_exit(
    ctx: &CommandContext,
    original_media: &MediaWindow,
    should_restore_pin: bool,
    previous_focus: Option<&str>,
) -> Result<()> {
    let addr = &original_media.address;

    // Refresh suppression before pin/focus restoration
    suppress_avoider().await;

    // Get fresh state after exiting fullscreen. If this call fails, skip
    // the secondary steps (pin restore, reposition) — they're best-effort.
    let fresh_clients = match ctx.hyprland.get_clients().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("get_clients failed after fullscreen exit, skipping restore steps: {e}");
            clear_suppression().await;
            return Ok(());
        }
    };

    let fresh_window = fresh_clients.iter().find(|c| c.address == *addr);

    // Address-recycling guard. Compositor addresses are heap pointers; in
    // the rare case the media window died mid-exit and a new window was
    // allocated at the same address, the class will differ. Skip the
    // dependent restore steps (pin/reposition/focus) — repositioning an
    // unrelated window would be far worse than leaving the user to re-pin.
    if let Some(fresh) = fresh_window
        && fresh.class != original_media.class
    {
        tracing::warn!(
            "post-fullscreen window address {} now belongs to class '{}' (expected '{}'); skipping reposition",
            addr,
            fresh.class,
            original_media.class
        );
        clear_suppression().await;
        return Ok(());
    }

    let current_pinned = fresh_window.map(|c| c.pinned).unwrap_or(false);

    // Restore pin if needed. Non-fatal: if Hyprland can't pin right now (e.g.
    // the window is still mid-transition), the user can re-pin manually and
    // the window is at least un-fullscreened.
    if should_restore_pin
        && !current_pinned
        && let Err(e) = ctx.hyprland.dispatch(&pin_action(addr)).await
    {
        tracing::warn!("pin restore after fullscreen exit failed (non-fatal): {e}");
    }

    // Position the media window to default position and resize. Non-fatal.
    if fresh_window.is_some()
        && let Err(e) = super::reposition_to_default(ctx, addr).await
    {
        tracing::warn!("reposition after fullscreen exit failed (non-fatal): {e}");
    }

    // Restore focus to previous window if valid. Non-fatal.
    if let Some(prev_addr) = previous_focus
        && let Some(target_addr) = find_valid_focus_target(&fresh_clients, addr, prev_addr)
        && let Err(e) = restore_focus(ctx, &target_addr).await
    {
        tracing::warn!("focus restore after fullscreen exit failed (non-fatal): {e}");
    }

    // Clear suppression and explicitly trigger avoid with fresh state.
    // We can't rely on the daemon because:
    // 1. The movewindow event from our repositioning may have updated the daemon's debounce timer
    // 2. The activewindow event from focus restore may arrive within the debounce window (15ms)
    // 3. The daemon would skip avoid due to debounce, leaving the window in wrong position
    //
    // By explicitly calling avoid here, we ensure proper positioning with fresh client data.
    clear_suppression().await;
    if let Err(e) = super::avoid::avoid(ctx).await {
        tracing::debug!("avoid after fullscreen exit failed (non-fatal): {e}");
    }

    Ok(())
}

/// Find a valid window to restore focus to.
///
/// Prefers the specified previous focus, falls back to most recently focused
/// mapped+visible window on the same workspace as the media window. Returns
/// the address as an owned String.
///
/// Filtering rules for the fallback path mirror
/// [`crate::window::WindowMatcher::find_previous_focus`]:
/// - Same workspace as the media window (avoids cross-workspace focus jumps).
/// - Excludes the media window itself.
/// - Excludes hidden / unmapped windows.
/// - Excludes never-focused windows (`focus_history_id < 0`).
fn find_valid_focus_target(
    clients: &[Client],
    media_addr: &str,
    prev_addr: &str,
) -> Option<String> {
    // Check if previous focus window is still valid (and not the media window).
    // The previous-focus address is trusted because it came from
    // `WindowMatcher::find_previous_focus` which already applied the workspace
    // and history filters at capture time.
    if prev_addr != media_addr
        && clients
            .iter()
            .any(|c| c.address == prev_addr && c.is_visible())
    {
        return Some(prev_addr.to_string());
    }

    // Fallback: find most recently focused mapped+visible window, excluding
    // never-focused windows. When the media window is still in `clients`,
    // restrict to its workspace so we don't yank focus across workspaces and
    // trigger an unwanted workspace switch. When the media window has already
    // been removed from the snapshot (e.g. it was closed mid-operation) we
    // can't compute a workspace, so we fall back to any workspace — restoring
    // focus to *something* is better than leaving the user with nothing focused.
    let media_workspace = clients
        .iter()
        .find(|c| c.address == media_addr)
        .map(|c| c.workspace.id);

    clients
        .iter()
        .filter(|c| {
            c.address != media_addr
                && c.is_visible()
                && c.has_focus_history()
                && media_workspace.is_none_or(|ws| c.workspace.id == ws)
        })
        .min_by_key(|c| c.focus_history_id)
        .map(|c| c.address.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    /// Re-export the shared no-suppress config from `test_helpers` under the
    /// local name existing tests expect. Centralised so all test modules
    /// share one fixture.
    use crate::test_helpers::test_config_no_suppress as test_config;

    #[test]
    fn pip_title_detection() {
        assert!(is_pip_title("Picture-in-Picture"));
        assert!(is_pip_title("picture-in-picture"));
        assert!(is_pip_title("Picture in Picture"));
        assert!(is_pip_title("PICTURE-IN-PICTURE"));
        assert!(!is_pip_title("Not a PiP window"));
        assert!(!is_pip_title("Picture"));
        assert!(!is_pip_title(""));
    }

    #[test]
    fn find_valid_focus_target_prefers_previous() {
        use crate::hyprland::Workspace;

        let clients = vec![
            Client {
                address: "0x1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
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
                focus_history_id: 1,
                pid: 0,
            },
            Client {
                address: "0x2".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "kitty".to_string(),
                title: "Terminal".to_string(),
                focus_history_id: 0,
                pid: 0,
            },
            Client {
                address: "0x3".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
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
                focus_history_id: 2,
                pid: 0,
            },
        ];

        // Should prefer the specified previous focus
        let result = find_valid_focus_target(&clients, "0x3", "0x1");
        assert_eq!(result.as_deref(), Some("0x1"));

        // If previous focus is media itself, should skip it
        let result = find_valid_focus_target(&clients, "0x3", "0x3");
        assert_eq!(result.as_deref(), Some("0x2")); // Falls back to most recent
    }

    #[test]
    fn find_valid_focus_target_falls_back_when_invalid() {
        use crate::hyprland::Workspace;

        let clients = vec![
            Client {
                address: "0x1".to_string(),
                mapped: true,
                hidden: true, // Hidden - invalid
                at: [0, 0],
                size: [100, 100],
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
                focus_history_id: 1,
                pid: 0,
            },
            Client {
                address: "0x2".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 1,
                    name: "1".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "kitty".to_string(),
                title: "Terminal".to_string(),
                focus_history_id: 0,
                pid: 0,
            },
        ];

        // Previous focus 0x1 is hidden, should fall back to 0x2
        let result = find_valid_focus_target(&clients, "0x99", "0x1");
        assert_eq!(result.as_deref(), Some("0x2"));
    }

    #[test]
    fn find_valid_focus_target_returns_none_when_no_candidates() {
        use crate::hyprland::Workspace;

        let clients = vec![Client {
            address: "0x1".to_string(),
            mapped: true,
            hidden: false,
            at: [0, 0],
            size: [100, 100],
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
            focus_history_id: 0,
            pid: 0,
        }];

        // Only the media window exists
        let result = find_valid_focus_target(&clients, "0x1", "0x999");
        assert!(result.is_none());
    }

    /// Regression: when the media window is on workspace 1 and the only other
    /// candidate sits on workspace 2, the fallback path must NOT cross
    /// workspaces — doing so would yank the user's focus to a workspace they
    /// did not ask for. Returning None lets the caller leave focus where it
    /// landed naturally after fullscreen exit.
    #[test]
    fn find_valid_focus_target_does_not_cross_workspaces() {
        use crate::hyprland::Workspace;

        let clients = vec![
            // media on ws1
            Client {
                address: "0xd1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
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
            // candidate on ws2 — must NOT be picked
            Client {
                address: "0xb2".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: Workspace {
                    id: 2,
                    name: "2".to_string(),
                },
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "firefox".to_string(),
                title: "Browser".to_string(),
                focus_history_id: 0,
                pid: 0,
            },
        ];

        let result = find_valid_focus_target(&clients, "0xd1", "0xe2");
        assert!(
            result.is_none(),
            "must not cross workspaces in fallback; got {result:?}"
        );
    }

    /// Regression: never-focused windows (`focus_history_id < 0`) must not be
    /// picked as the focus-restore fallback. They were never focused in this
    /// session so handing them focus is jarring.
    #[test]
    fn find_valid_focus_target_skips_never_focused() {
        use crate::hyprland::Workspace;

        let ws1 = Workspace {
            id: 1,
            name: "1".to_string(),
        };
        let clients = vec![
            Client {
                address: "0xd1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: ws1.clone(),
                floating: true,
                pinned: true,
                fullscreen: 0,
                monitor: 0,
                class: "mpv".to_string(),
                title: "video.mp4".to_string(),
                focus_history_id: 1,
                pid: 0,
            },
            Client {
                address: "0xe1".to_string(),
                mapped: true,
                hidden: false,
                at: [0, 0],
                size: [100, 100],
                workspace: ws1,
                floating: false,
                pinned: false,
                fullscreen: 0,
                monitor: 0,
                class: "term".to_string(),
                title: "term".to_string(),
                focus_history_id: -1, // never focused
                pid: 0,
            },
        ];

        // Previous focus is the media window itself (so we hit the fallback),
        // and the only other candidate has focus_history_id < 0 → must skip.
        let result = find_valid_focus_target(&clients, "0xd1", "0xd1");
        assert!(
            result.is_none(),
            "never-focused windows must be excluded; got {result:?}"
        );
    }

    // --- E2E tests using mock Hyprland ---

    #[tokio::test]
    async fn fullscreen_enter_unpinned() {
        let mock = MockHyprland::start().await;

        // mpv is floating, not pinned, not fullscreen
        let clients = vec![
            make_test_client_full(
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
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                false,
                true,
                0,
                1,
                0,
                1,
                [1272, 712],
                [640, 360],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should batch: focus + fullscreen (no unpin since not pinned)
        let batch = cmds.iter().find(|c| c.contains("fullscreen"));
        assert!(batch.is_some(), "expected fullscreen dispatch: {cmds:?}");
        let batch = batch.unwrap();
        assert!(
            batch.contains("focuswindow"),
            "should focus before fullscreen"
        );
        // Should NOT contain pin toggle
        assert!(
            !batch.contains("dispatch pin"),
            "should not unpin when not pinned: {batch}"
        );
    }

    #[tokio::test]
    async fn fullscreen_enter_pinned_unpins_first() {
        let mock = MockHyprland::start().await;

        // mpv is pinned + floating
        let clients = vec![
            make_test_client_full(
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
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                true,
                true,
                0,
                1,
                0,
                1,
                [1272, 712],
                [640, 360],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let batch = cmds.iter().find(|c| c.contains("fullscreen"));
        assert!(batch.is_some(), "expected fullscreen: {cmds:?}");
        let batch = batch.unwrap();
        // Should contain pin toggle (unpin before fullscreen)
        assert!(
            batch.contains("dispatch pin"),
            "should unpin before fullscreen: {batch}"
        );
    }

    #[tokio::test]
    async fn fullscreen_exit_restores_pin() {
        let mock = MockHyprland::start().await;

        // mpv is fullscreen, was pinned (pinned=true even though fullscreen)
        // After exit_fullscreen, the mock returns it as non-fullscreen, non-pinned
        let clients_fullscreen = vec![
            make_test_client_full(
                "0xb1",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                true,
                true,
                2,
                1,
                0,
                0,
                [0, 0],
                [1920, 1080], // fullscreen=2, pinned=true
            ),
        ];
        let clients_exited = vec![
            make_test_client_full(
                "0xb1",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                false,
                true,
                0,
                1,
                0,
                0,
                [1272, 712],
                [640, 360], // fullscreen=0, pinned=false
            ),
        ];

        // First call returns fullscreen, subsequent calls return exited
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fullscreen),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should dispatch pin to restore it
        let has_pin = cmds
            .iter()
            .any(|c| c.contains("dispatch pin address:0xd1") && !c.contains("fullscreen"));
        assert!(has_pin, "should restore pin after exit: {cmds:?}");
    }

    #[tokio::test]
    async fn fullscreen_no_media_window_is_noop() {
        let mock = MockHyprland::start().await;

        // No media windows
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

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Only j/clients fetch, no dispatches
        assert_eq!(cmds.len(), 1, "should only fetch clients: {cmds:?}");
        assert_eq!(cmds[0], "j/clients");
    }

    #[tokio::test]
    async fn fullscreen_auto_pin_when_always_pin_set() {
        let mock = MockHyprland::start().await;

        // PiP window: always_pin=true in default config for "Picture-in-Picture" title
        // Window is floating but NOT pinned
        let clients = vec![
            make_test_client_full(
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
            ),
            make_test_client_full(
                "0xa1",
                "firefox",
                "Picture-in-Picture",
                false,
                true,
                0,
                1,
                0,
                1,
                [1272, 712],
                [320, 180],
            ),
        ];
        mock.set_response("j/clients", &make_clients_json(&clients))
            .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should pin instead of fullscreening (auto-pin behavior)
        let has_pin = cmds.iter().any(|c| c.contains("dispatch pin"));
        assert!(has_pin, "should auto-pin PiP window: {cmds:?}");
        let has_fullscreen = cmds.iter().any(|c| c.contains("fullscreen"));
        assert!(
            !has_fullscreen,
            "should NOT fullscreen when auto-pinning: {cmds:?}"
        );
    }

    #[tokio::test]
    async fn fullscreen_exit_restores_focus_to_previous() {
        let mock = MockHyprland::start().await;

        // mpv fullscreen, firefox was previous focus
        let clients_fullscreen = vec![
            make_test_client_full(
                "0xb1",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                false,
                true,
                2,
                1,
                0,
                0,
                [0, 0],
                [1920, 1080],
            ),
        ];
        let clients_exited = vec![
            make_test_client_full(
                "0xb1",
                "firefox",
                "Browser",
                false,
                false,
                0,
                1,
                0,
                1,
                [0, 0],
                [1920, 1080],
            ),
            make_test_client_full(
                "0xd1",
                "mpv",
                "video.mp4",
                false,
                true,
                0,
                1,
                0,
                0,
                [1272, 712],
                [640, 360],
            ),
        ];

        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fullscreen),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Should restore focus to firefox
        let has_focus_restore = cmds
            .iter()
            .any(|c| c.contains("focuswindow address:0xb1") && c.contains("no_warps"));
        assert!(
            has_focus_restore,
            "should restore focus to firefox: {cmds:?}"
        );
    }

    /// Regression: toggling fullscreen on then off should be symmetric — each
    /// invocation drives Hyprland through `dispatch fullscreen 0` exactly once
    /// per call (the second call is the exit path which already covers retry).
    #[tokio::test]
    async fn fullscreen_toggle_is_idempotent_on_unpinned() {
        // Two back-to-back invocations: enter, then exit. Verify that each
        // invocation independently runs without panicking and that the second
        // invocation observes the new fullscreen=2 state (i.e. takes the exit
        // path with retry/restore).
        let mock = MockHyprland::start().await;

        // Sequence: enter (sees fs=0), then exit pass sees fs=2 then fs=0.
        let pre = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            false,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        let entered = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            false,
            true,
            2,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        let exited = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            false,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&pre),     // first call: enter sees fs=0
                make_clients_json(&entered), // second call: exit sees fs=2
                make_clients_json(&exited),  // exit retry verifies fs=0
                make_clients_json(&exited),  // post-exit fresh fetch
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap(); // enter
        fullscreen(&ctx).await.unwrap(); // exit

        let cmds = mock.captured_commands().await;
        let fs_dispatches = cmds
            .iter()
            .filter(|c| c.contains("dispatch fullscreen 0"))
            .count();
        // Enter: 1 fullscreen dispatch in the batch.
        // Exit:  1 fullscreen dispatch in the initial batch + retries verify state.
        assert!(
            fs_dispatches >= 2,
            "expected at least 2 fullscreen dispatches across enter+exit: {cmds:?}"
        );
    }

    /// Regression: a regular pinned media window — neither PiP (title) nor
    /// always_pin (pattern) — must still be re-pinned after fullscreen exit.
    /// `should_restore_pin` collapses three branches (`always_pin || pinned ||
    /// is_pip_title`); this test locks down the middle branch (raw pinned) so a
    /// future refactor can't silently drop it.
    ///
    /// Uses class=mpv (default config: always_pin=false) and title=video.mp4
    /// (not PiP). Pre-state: pinned=true, fullscreen=2. Post-state: the mock
    /// returns it unpinned (mirrors how Hyprland clears pin on entering fs).
    #[tokio::test]
    async fn fullscreen_exit_regular_pinned_restores_pin() {
        let mock = MockHyprland::start().await;

        // Pre-exit: mpv (non-PiP, non-always_pin) is pinned + fullscreen.
        let clients_fs = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            true, // pinned
            true, // floating
            2,    // fullscreen
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        // Post-exit: still mpv (no address recycle), unpinned, normal size.
        let clients_exited = vec![make_test_client_full(
            "0xd1",
            "mpv",
            "video.mp4",
            false, // pinned cleared by fs exit
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [640, 360],
        )];
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fs),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        // Pin restore is dispatched as a standalone `dispatch pin` (not part
        // of the fullscreen batch). Filter out the fullscreen batch to be
        // unambiguous.
        let has_pin_restore = cmds
            .iter()
            .any(|c| c.contains("dispatch pin address:0xd1") && !c.contains("fullscreen"));
        assert!(
            has_pin_restore,
            "regular pinned mpv must be re-pinned after fs exit: {cmds:?}"
        );
    }

    /// Regression: PiP windows have always_pin=true. After exit, pin must be
    /// restored even when the window WAS pinned before fullscreening.
    #[tokio::test]
    async fn fullscreen_exit_pip_restores_pin() {
        let mock = MockHyprland::start().await;

        let clients_fs = vec![make_test_client_full(
            "0xa1",
            "firefox",
            "Picture-in-Picture",
            true,
            true,
            2,
            1,
            0,
            0,
            [0, 0],
            [1920, 1080],
        )];
        let clients_exited = vec![make_test_client_full(
            "0xa1",
            "firefox",
            "Picture-in-Picture",
            false,
            true,
            0,
            1,
            0,
            0,
            [1272, 712],
            [320, 180],
        )];
        mock.set_response_sequence(
            "j/clients",
            vec![
                make_clients_json(&clients_fs),
                make_clients_json(&clients_exited),
            ],
        )
        .await;

        let ctx = mock.context(test_config());
        fullscreen(&ctx).await.unwrap();

        let cmds = mock.captured_commands().await;
        let has_pin_restore = cmds
            .iter()
            .any(|c| c.contains("dispatch pin address:0xa1") && !c.contains("fullscreen"));
        assert!(has_pin_restore, "PiP exit must restore pin: {cmds:?}");
    }
}
