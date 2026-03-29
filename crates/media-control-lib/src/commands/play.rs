//! Play subcommand — start playback via mpv-shim IPC.
//!
//! Targets:
//! - `next-up`: play next-up from the currently active store
//! - `<store-name>`: switch to that store and play its next-up (twitch, jellyfin, pinchflat, etc.)
//! - `<item-id>`: play a specific item by hex ID (shim auto-detects store)

use super::{send_mpv_script_message, send_mpv_script_message_with_args, CommandContext};

/// What to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayTarget {
    /// Play next-up from the active store.
    NextUp,
    /// Switch to a named store/context and play its next-up item.
    Store(String),
    /// A specific item ID (hex, 32+ chars).
    ItemId(String),
}

impl PlayTarget {
    /// Parse a target string from the CLI.
    pub fn parse(s: &str) -> Self {
        match s {
            "next-up" => Self::NextUp,
            id if id.len() >= 32 && id.chars().all(|c| c.is_ascii_hexdigit()) => {
                Self::ItemId(id.to_string())
            }
            // Everything else is a store/context name
            name => Self::Store(name.to_string()),
        }
    }
}

/// Start playback via mpv-shim IPC.
///
/// All playback delegation goes through the shim — media-control is just
/// the control plane, not the resolution engine.
pub async fn play(
    _ctx: &CommandContext,
    target_str: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let target = PlayTarget::parse(target_str);

    match target {
        PlayTarget::NextUp => {
            send_mpv_script_message("play-next-up").await?;
        }
        PlayTarget::Store(name) => {
            // Send play-{name} to the shim. Each store/context handles its own
            // play logic (e.g., play-twitch, play-jellyfin, play-pinchflat).
            let cmd = format!("play-{}", name);
            send_mpv_script_message(&cmd).await?;
        }
        PlayTarget::ItemId(id) => {
            send_mpv_script_message_with_args("play-item", &[&id]).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_next_up() {
        assert_eq!(PlayTarget::parse("next-up"), PlayTarget::NextUp);
    }

    #[test]
    fn parse_store_name() {
        assert_eq!(
            PlayTarget::parse("twitch"),
            PlayTarget::Store("twitch".to_string())
        );
        assert_eq!(
            PlayTarget::parse("jellyfin"),
            PlayTarget::Store("jellyfin".to_string())
        );
        assert_eq!(
            PlayTarget::parse("pinchflat"),
            PlayTarget::Store("pinchflat".to_string())
        );
    }

    #[test]
    fn parse_item_id() {
        assert_eq!(
            PlayTarget::parse("a5c0a87b1d058d1b7e70f5406ee274e2"),
            PlayTarget::ItemId("a5c0a87b1d058d1b7e70f5406ee274e2".to_string())
        );
    }
}
