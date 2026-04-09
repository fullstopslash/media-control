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
}

/// Navigate to the next or previous chapter in mpv.
///
/// # Errors
///
/// Returns an error if no mpv IPC socket is available.
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
    let payload = format!(r#"{{"command":["add","chapter",{}]}}"#, direction.offset());
    send_mpv_ipc_command(&payload).await
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
    fn mpv_command_format() {
        let next_cmd = format!(
            r#"{{"command":["add","chapter",{}]}}"#,
            ChapterDirection::Next.offset()
        );
        assert_eq!(next_cmd, r#"{"command":["add","chapter",1]}"#);

        let prev_cmd = format!(
            r#"{{"command":["add","chapter",{}]}}"#,
            ChapterDirection::Prev.offset()
        );
        assert_eq!(prev_cmd, r#"{"command":["add","chapter",-1]}"#);
    }
}
