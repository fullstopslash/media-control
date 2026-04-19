//! mpv chapter navigation.
//!
//! Provides chapter navigation commands (next/prev) for mpv playback
//! using the shared mpv IPC infrastructure.

use super::send_mpv_ipc_command;
use crate::error::Result;

/// Direction for chapter navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterDirection {
    /// Navigate to the next chapter.
    Next,
    /// Navigate to the previous chapter.
    Prev,
}

impl ChapterDirection {
    /// Get the chapter offset value for mpv IPC.
    const fn offset(self) -> i8 {
        match self {
            Self::Next => 1,
            Self::Prev => -1,
        }
    }

    /// Parse a chapter direction from a string.
    ///
    /// Accepts `next`/`Next` for [`Self::Next`] and
    /// `prev`/`Prev`/`previous`/`Previous` for [`Self::Prev`].
    /// Mirrors [`super::move_window::Direction::parse`] so CLI dispatch in
    /// both subcommands stays consistent.
    ///
    /// # Returns
    ///
    /// - `Some(ChapterDirection)` for valid input
    /// - `None` otherwise
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "next" | "Next" => Some(Self::Next),
            "prev" | "Prev" | "previous" | "Previous" => Some(Self::Prev),
            _ => None,
        }
    }
}

/// Build the chapter-navigation IPC payload.
///
/// Centralises payload construction so both the IPC entry point and tests
/// use the same shape, and so future additions (e.g. a step size) live in
/// one place. Uses `serde_json::json!` for safe JSON construction —
/// mirrors `seek::build_payload` and avoids any chance of malformed JSON
/// from string interpolation drift.
fn build_payload(direction: ChapterDirection) -> String {
    serde_json::json!({"command": ["add", "chapter", direction.offset()]}).to_string()
}

/// Navigate to the next or previous chapter in mpv.
///
/// # Errors
///
/// Returns [`crate::error::MediaControlError::MpvIpc`] with kind `NoSocket`
/// if no mpv IPC socket is available.
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::chapter::{chapter, ChapterDirection};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// chapter(ChapterDirection::Next).await?;
/// # Ok(())
/// # }
/// ```
pub async fn chapter(direction: ChapterDirection) -> Result<()> {
    send_mpv_ipc_command(&build_payload(direction)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_direction_offset() {
        assert_eq!(ChapterDirection::Next.offset(), 1);
        assert_eq!(ChapterDirection::Prev.offset(), -1);
    }

    #[test]
    fn parse_accepts_documented_inputs() {
        assert_eq!(
            ChapterDirection::parse("next"),
            Some(ChapterDirection::Next)
        );
        assert_eq!(
            ChapterDirection::parse("Next"),
            Some(ChapterDirection::Next)
        );
        assert_eq!(
            ChapterDirection::parse("prev"),
            Some(ChapterDirection::Prev)
        );
        assert_eq!(
            ChapterDirection::parse("Prev"),
            Some(ChapterDirection::Prev)
        );
        assert_eq!(
            ChapterDirection::parse("previous"),
            Some(ChapterDirection::Prev)
        );
        assert_eq!(
            ChapterDirection::parse("Previous"),
            Some(ChapterDirection::Prev)
        );
    }

    #[test]
    fn parse_rejects_unknown_inputs() {
        assert_eq!(ChapterDirection::parse(""), None);
        assert_eq!(ChapterDirection::parse("NEXT"), None);
        assert_eq!(ChapterDirection::parse("forward"), None);
        assert_eq!(ChapterDirection::parse("n"), None);
    }

    #[test]
    fn mpv_command_format() {
        // Parse rather than string-compare so the test doesn't depend on
        // serde_json's whitespace/key-ordering choices.
        let next: serde_json::Value =
            serde_json::from_str(&build_payload(ChapterDirection::Next)).unwrap();
        let cmd = next["command"].as_array().unwrap();
        assert_eq!(cmd[0], "add");
        assert_eq!(cmd[1], "chapter");
        assert_eq!(cmd[2], 1);

        let prev: serde_json::Value =
            serde_json::from_str(&build_payload(ChapterDirection::Prev)).unwrap();
        assert_eq!(prev["command"][2], -1);
    }
}
