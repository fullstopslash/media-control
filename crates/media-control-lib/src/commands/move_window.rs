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
//! let ctx = CommandContext::new()?;
//! move_window(&ctx, Direction::Left).await?;  // Move to left edge
//! move_window(&ctx, Direction::Down).await?;  // Move to bottom edge
//! # Ok(())
//! # }
//! ```

use super::{get_media_window, suppress_avoider, CommandContext};
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
    /// assert_eq!(Direction::from_str("h"), Some(Direction::Left));
    /// assert_eq!(Direction::from_str("l"), Some(Direction::Right));
    ///
    /// // Intuitive names (case-insensitive)
    /// assert_eq!(Direction::from_str("left"), Some(Direction::Left));
    /// assert_eq!(Direction::from_str("RIGHT"), Some(Direction::Right));
    /// assert_eq!(Direction::from_str("Up"), Some(Direction::Up));
    /// ```
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        // Try intuitive names first (case-insensitive)
        match s.to_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            // Fall back to vim-style single character
            _ => s.chars().next().and_then(Self::from_char),
        }
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
/// let ctx = CommandContext::new()?;
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

    // Get positions from config
    let positions = &ctx.config.positions;

    // Calculate new position based on direction
    let (new_x, new_y) = match direction {
        Direction::Left => (positions.x_left, window.y),
        Direction::Right => (positions.x_right, window.y),
        Direction::Up => (window.x, positions.y_top),
        Direction::Down => (window.x, positions.y_bottom),
    };

    // Execute batch command to move and resize
    let move_cmd = format!(
        "dispatch movewindowpixel exact {} {},address:{}",
        new_x, new_y, window.address
    );
    let resize_cmd = format!(
        "dispatch resizewindowpixel exact {} {},address:{}",
        positions.width, positions.height, window.address
    );

    ctx.hyprland
        .batch(&[&move_cmd, &resize_cmd])
        .await?;

    // Suppress avoider to prevent immediate repositioning
    let _ = suppress_avoider().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(Direction::from_str("h"), Some(Direction::Left));
        assert_eq!(Direction::from_str("j"), Some(Direction::Down));
        assert_eq!(Direction::from_str("k"), Some(Direction::Up));
        assert_eq!(Direction::from_str("l"), Some(Direction::Right));
    }

    #[test]
    fn direction_from_str_intuitive_names() {
        assert_eq!(Direction::from_str("left"), Some(Direction::Left));
        assert_eq!(Direction::from_str("right"), Some(Direction::Right));
        assert_eq!(Direction::from_str("up"), Some(Direction::Up));
        assert_eq!(Direction::from_str("down"), Some(Direction::Down));
    }

    #[test]
    fn direction_from_str_case_insensitive() {
        assert_eq!(Direction::from_str("LEFT"), Some(Direction::Left));
        assert_eq!(Direction::from_str("Right"), Some(Direction::Right));
        assert_eq!(Direction::from_str("UP"), Some(Direction::Up));
        assert_eq!(Direction::from_str("DoWn"), Some(Direction::Down));
    }

    #[test]
    fn direction_from_str_vim_fallback() {
        // Single vim-style characters still work
        assert_eq!(Direction::from_str("hjkl"), Some(Direction::Left)); // 'h' is first
    }

    #[test]
    fn direction_from_str_empty() {
        assert_eq!(Direction::from_str(""), None);
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
}
