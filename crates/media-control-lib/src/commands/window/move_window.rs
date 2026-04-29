//! Vim-style directional movement to preset positions.
//!
//! Moves the media window to one of four corner positions based on
//! vim-style direction keys (h/j/k/l).
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::commands::{CommandContext, move_window::{Direction, move_window}};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ctx = CommandContext::new().await?;
//! move_window(&ctx, Direction::Left).await?;  // Move to left edge
//! move_window(&ctx, Direction::Down).await?;  // Move to bottom edge
//! # Ok(())
//! # }
//! ```

use super::{
    CommandContext, get_media_window, move_pixel_action, resize_pixel_action, suppress_avoider,
};
use crate::error::Result;

/// Movement direction using vim-style keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Move to left edge (h key).
    Left,
    /// Move to bottom edge (j key).
    Down,
    /// Move to top edge (k key).
    Up,
    /// Move to right edge (l key).
    Right,
}

impl Direction {
    /// Parse a direction from a vim-style character.
    ///
    /// # Returns
    ///
    /// - `Some(Direction)` for valid keys (h, j, k, l)
    /// - `None` for any other character
    ///
    /// # Example
    ///
    /// ```
    /// use media_control_lib::commands::move_window::Direction;
    ///
    /// assert_eq!(Direction::from_char('h'), Some(Direction::Left));
    /// assert_eq!(Direction::from_char('j'), Some(Direction::Down));
    /// assert_eq!(Direction::from_char('k'), Some(Direction::Up));
    /// assert_eq!(Direction::from_char('l'), Some(Direction::Right));
    /// assert_eq!(Direction::from_char('x'), None);
    /// ```
    #[must_use]
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'h' => Some(Self::Left),
            'j' => Some(Self::Down),
            'k' => Some(Self::Up),
            'l' => Some(Self::Right),
            _ => None,
        }
    }

    /// Parse a direction from a string.
    ///
    /// Accepts both vim-style keys (h, j, k, l) and intuitive names
    /// (left, down, up, right). Case-insensitive for named directions.
    ///
    /// # Returns
    ///
    /// - `Some(Direction)` for valid directions
    /// - `None` for invalid input
    ///
    /// # Example
    ///
    /// ```
    /// use media_control_lib::commands::move_window::Direction;
    ///
    /// // Vim-style keys
    /// assert_eq!(Direction::parse("h"), Some(Direction::Left));
    /// assert_eq!(Direction::parse("l"), Some(Direction::Right));
    ///
    /// // Intuitive names (case-insensitive)
    /// assert_eq!(Direction::parse("left"), Some(Direction::Left));
    /// assert_eq!(Direction::parse("RIGHT"), Some(Direction::Right));
    /// assert_eq!(Direction::parse("Up"), Some(Direction::Up));
    /// ```
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        // Try intuitive names first (case-insensitive, no allocation)
        if s.eq_ignore_ascii_case("left") {
            return Some(Self::Left);
        }
        if s.eq_ignore_ascii_case("right") {
            return Some(Self::Right);
        }
        if s.eq_ignore_ascii_case("up") {
            return Some(Self::Up);
        }
        if s.eq_ignore_ascii_case("down") {
            return Some(Self::Down);
        }
        // Fall back to vim-style single character
        s.chars().next().and_then(Self::from_char)
    }
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Left => "left",
            Self::Down => "down",
            Self::Up => "up",
            Self::Right => "right",
        };
        write!(f, "{s}")
    }
}

/// Move the media window in the specified direction.
///
/// Calculates the new position based on direction:
/// - `Left` (h): x = positions.x_left, y = current y
/// - `Right` (l): x = positions.x_right, y = current y
/// - `Up` (k): x = current x, y = positions.y_top
/// - `Down` (j): x = current x, y = positions.y_bottom
///
/// The window is also resized to the configured width/height.
///
/// # Returns
///
/// - `Ok(())` on success or if no media window is found (silent no-op)
/// - `Err(...)` if Hyprland IPC fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, move_window::{Direction, move_window}};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new().await?;
///
/// // Move window to each corner
/// move_window(&ctx, Direction::Up).await?;    // Top edge
/// move_window(&ctx, Direction::Right).await?; // Right edge (top-right corner)
/// # Ok(())
/// # }
/// ```
pub async fn move_window(ctx: &CommandContext, direction: Direction) -> Result<()> {
    // Get media window, return Ok(()) if not found (matches bash behavior)
    let Some(window) = get_media_window(ctx).await? else {
        return Ok(());
    };

    // Fullscreen guard — mirrors `pin.rs` and `minify.rs`. Dispatching
    // `movewindowpixel` against a fullscreen window is a wasted round-trip
    // (Hyprland ignores geometry on fullscreen) and would emit a spurious
    // `movewindow` event that consumes a debounce cycle in the avoider.
    if window.fullscreen > 0 {
        return Ok(());
    }

    // Resolve only the position needed for this direction (avoids unnecessary stat calls).
    // The `resolve_position_or` helper centralises the `.unwrap_or(default)` pattern
    // shared with `avoid.rs`; here `0` is the canonical fallback for an unknown name.
    let resolve = |name: &str| super::resolve_position_or(ctx, name, 0);
    let (ew, eh) = super::effective_dimensions(ctx);

    let (new_x, new_y) = match direction {
        Direction::Left => (resolve("x_left"), window.y),
        Direction::Right => (resolve("x_right"), window.y),
        Direction::Up => (window.x, resolve("y_top")),
        Direction::Down => (window.x, resolve("y_bottom")),
    };

    // Execute batch command to move and resize
    let move_cmd = move_pixel_action(&window.address, new_x, new_y);
    let resize_cmd = resize_pixel_action(&window.address, ew, eh);

    // Suppress BEFORE dispatch — the movewindow event arrives within the
    // daemon's debounce window, so we have to beat it to the suppress file.
    suppress_avoider().await;

    ctx.hyprland
        .dispatch_batch(&[&move_cmd, &resize_cmd])
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::with_isolated_runtime_dir;

    #[test]
    fn direction_from_char_valid() {
        assert_eq!(Direction::from_char('h'), Some(Direction::Left));
        assert_eq!(Direction::from_char('j'), Some(Direction::Down));
        assert_eq!(Direction::from_char('k'), Some(Direction::Up));
        assert_eq!(Direction::from_char('l'), Some(Direction::Right));
    }

    #[test]
    fn direction_from_char_invalid() {
        assert_eq!(Direction::from_char('x'), None);
        assert_eq!(Direction::from_char('H'), None); // Case sensitive
        assert_eq!(Direction::from_char('a'), None);
        assert_eq!(Direction::from_char(' '), None);
    }

    #[test]
    fn direction_from_str_valid() {
        assert_eq!(Direction::parse("h"), Some(Direction::Left));
        assert_eq!(Direction::parse("j"), Some(Direction::Down));
        assert_eq!(Direction::parse("k"), Some(Direction::Up));
        assert_eq!(Direction::parse("l"), Some(Direction::Right));
    }

    #[test]
    fn direction_from_str_intuitive_names() {
        assert_eq!(Direction::parse("left"), Some(Direction::Left));
        assert_eq!(Direction::parse("right"), Some(Direction::Right));
        assert_eq!(Direction::parse("up"), Some(Direction::Up));
        assert_eq!(Direction::parse("down"), Some(Direction::Down));
    }

    #[test]
    fn direction_from_str_case_insensitive() {
        assert_eq!(Direction::parse("LEFT"), Some(Direction::Left));
        assert_eq!(Direction::parse("Right"), Some(Direction::Right));
        assert_eq!(Direction::parse("UP"), Some(Direction::Up));
        assert_eq!(Direction::parse("DoWn"), Some(Direction::Down));
    }

    #[test]
    fn direction_from_str_vim_fallback() {
        // Single vim-style characters still work
        assert_eq!(Direction::parse("hjkl"), Some(Direction::Left)); // 'h' is first
    }

    #[test]
    fn direction_from_str_empty() {
        assert_eq!(Direction::parse(""), None);
    }

    #[test]
    fn direction_display() {
        assert_eq!(Direction::Left.to_string(), "left");
        assert_eq!(Direction::Down.to_string(), "down");
        assert_eq!(Direction::Up.to_string(), "up");
        assert_eq!(Direction::Right.to_string(), "right");
    }

    #[test]
    fn direction_debug() {
        assert_eq!(format!("{:?}", Direction::Left), "Left");
        assert_eq!(format!("{:?}", Direction::Down), "Down");
        assert_eq!(format!("{:?}", Direction::Up), "Up");
        assert_eq!(format!("{:?}", Direction::Right), "Right");
    }

    #[test]
    fn direction_clone_copy() {
        let dir = Direction::Left;
        #[allow(clippy::clone_on_copy)]
        let cloned = dir.clone();
        let copied = dir;

        assert_eq!(dir, cloned);
        assert_eq!(dir, copied);
    }

    #[test]
    fn direction_equality() {
        assert_eq!(Direction::Left, Direction::Left);
        assert_ne!(Direction::Left, Direction::Right);
        assert_ne!(Direction::Up, Direction::Down);
    }

    // --- E2E tests ---

    use crate::test_helpers::*;

    fn mpv_at(x: i32, y: i32) -> String {
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
                [x, y],
                [640, 360],
            ),
        ];
        make_clients_json(&clients)
    }

    #[tokio::test]
    async fn move_left_dispatches_correct_position() {
        with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            mock.set_response("j/clients", &mpv_at(1272, 712)).await;
            let ctx = mock.default_context();

            move_window(&ctx, Direction::Left).await.unwrap();

            let cmds = mock.captured_commands().await;
            let batch = cmds.iter().find(|c| c.contains("movewindowpixel")).unwrap();
            // x_left=48, keep current y=712
            assert!(
                batch.contains("exact 48 712"),
                "expected x_left=48, y=712: {batch}"
            );
            assert!(batch.contains("resizewindowpixel"), "should also resize");
        })
        .await;
    }

    #[tokio::test]
    async fn move_right_dispatches_correct_position() {
        with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            mock.set_response("j/clients", &mpv_at(48, 712)).await;
            let ctx = mock.default_context();

            move_window(&ctx, Direction::Right).await.unwrap();

            let cmds = mock.captured_commands().await;
            let batch = cmds.iter().find(|c| c.contains("movewindowpixel")).unwrap();
            // x_right=1272, keep current y=712
            assert!(
                batch.contains("exact 1272 712"),
                "expected x_right=1272, y=712: {batch}"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn move_up_dispatches_correct_position() {
        with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            mock.set_response("j/clients", &mpv_at(1272, 712)).await;
            let ctx = mock.default_context();

            move_window(&ctx, Direction::Up).await.unwrap();

            let cmds = mock.captured_commands().await;
            let batch = cmds.iter().find(|c| c.contains("movewindowpixel")).unwrap();
            // keep current x=1272, y_top=48
            assert!(
                batch.contains("exact 1272 48"),
                "expected x=1272, y_top=48: {batch}"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn move_down_dispatches_correct_position() {
        with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            mock.set_response("j/clients", &mpv_at(1272, 48)).await;
            let ctx = mock.default_context();

            move_window(&ctx, Direction::Down).await.unwrap();

            let cmds = mock.captured_commands().await;
            let batch = cmds.iter().find(|c| c.contains("movewindowpixel")).unwrap();
            // keep current x=1272, y_bottom=712
            assert!(
                batch.contains("exact 1272 712"),
                "expected x=1272, y_bottom=712: {batch}"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn move_to_already_correct_position_still_dispatches() {
        // Current behavior: move always dispatches even when the window is
        // already at the target position. This is intentional — the resize
        // bundled in the batch may still be needed (e.g. after minify toggle).
        // Lock that in so we don't silently regress to a "skip if at target"
        // optimisation that would leave geometry stale.
        with_isolated_runtime_dir(|_| async {
            let mock = MockHyprland::start().await;
            // mpv already at x_left=48 with current y=712
            mock.set_response("j/clients", &mpv_at(48, 712)).await;
            let ctx = mock.default_context();

            move_window(&ctx, Direction::Left).await.unwrap();

            let cmds = mock.captured_commands().await;
            let batch = cmds.iter().find(|c| c.contains("movewindowpixel"));
            assert!(
                batch.is_some(),
                "move should dispatch even at correct position: {cmds:?}"
            );
            assert!(
                batch.unwrap().contains("exact 48 712"),
                "still targets x_left=48, y=712"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn move_no_media_window_is_noop() {
        with_isolated_runtime_dir(|_| async {
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

            move_window(&ctx, Direction::Left).await.unwrap();

            let cmds = mock.captured_commands().await;
            assert!(
                !cmds.iter().any(|c| c.contains("movewindowpixel")),
                "should not move: {cmds:?}"
            );
        })
        .await;
    }
}
