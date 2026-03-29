//! Play subcommand — resolve a Jellyfin item and start playback.
//!
//! Replaces shim-play.sh with native Rust. Supports three targets:
//! - `next-up`: First NextUp item across all shows
//! - `recent-pinchflat`: Most recent unwatched video from Pinchflat library
//! - `<item-id>`: Direct Jellyfin item ID

use super::{send_mpv_script_message, send_mpv_script_message_with_args, CommandContext};
use crate::jellyfin::JellyfinClient;

/// What to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayTarget {
    /// First NextUp item across all shows.
    NextUp,
    /// Most recent unwatched Pinchflat video.
    RecentPinchflat,
    /// A specific Jellyfin item ID.
    ItemId(String),
}

impl PlayTarget {
    /// Parse a target string from the CLI.
    pub fn parse(s: &str) -> Self {
        match s {
            "next-up" => Self::NextUp,
            "recent-pinchflat" => Self::RecentPinchflat,
            id => Self::ItemId(id.to_string()),
        }
    }
}

/// Resolve a playback target and start playback via IPC.
///
/// All targets now resolve to an item ID and send it directly to the shim
/// via `play-item` IPC command. No Jellyfin session routing needed.
pub async fn play(ctx: &CommandContext, target_str: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let target = PlayTarget::parse(target_str);

    // NextUp: delegate entirely to the shim
    if matches!(target, PlayTarget::NextUp) {
        send_mpv_script_message("play-next-up").await?;
        return Ok(());
    }

    // Resolve item ID
    let item_id = match &target {
        PlayTarget::RecentPinchflat => {
            let jf = JellyfinClient::from_default_credentials().await?;
            let lib_id = ctx
                .config
                .play
                .pinchflat_library_id
                .as_ref()
                .ok_or("No pinchflat_library_id in config.toml [play] section")?;
            let items = jf
                .get_unwatched_items(lib_id, "DateCreated", "Descending", None, 1)
                .await?;
            items
                .into_iter()
                .next()
                .map(|item| item.id)
                .ok_or("No unwatched Pinchflat videos found")?
        }
        PlayTarget::ItemId(id) => id.clone(),
        PlayTarget::NextUp => unreachable!(),
    };

    // Send IPC play-source hint (non-fatal)
    if let Err(e) = send_mpv_script_message_with_args("set-play-source", &["strategy"]).await {
        eprintln!("media-control: IPC hint failed (non-fatal): {e}");
    }

    // Send item ID directly to shim via IPC — shim resolves URL and plays
    send_mpv_script_message_with_args("play-item", &[&item_id]).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_target_parse_next_up() {
        assert_eq!(PlayTarget::parse("next-up"), PlayTarget::NextUp);
    }

    #[test]
    fn play_target_parse_recent_pinchflat() {
        assert_eq!(
            PlayTarget::parse("recent-pinchflat"),
            PlayTarget::RecentPinchflat
        );
    }

    #[test]
    fn play_target_parse_item_id() {
        assert_eq!(
            PlayTarget::parse("a5c0a87b1d058d1b7e70f5406ee274e2"),
            PlayTarget::ItemId("a5c0a87b1d058d1b7e70f5406ee274e2".to_string())
        );
    }

    #[test]
    fn play_target_parse_unknown_defaults_to_item_id() {
        assert_eq!(
            PlayTarget::parse("something-else"),
            PlayTarget::ItemId("something-else".to_string())
        );
    }
}
