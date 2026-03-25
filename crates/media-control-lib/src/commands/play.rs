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

/// Resolve a playback target, send IPC hint, and start playback.
pub async fn play(ctx: &CommandContext, target_str: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let target = PlayTarget::parse(target_str);
    let jf = JellyfinClient::from_default_credentials().await?;

    // NextUp: delegate entirely to the shim's merged queue (includes movies + series)
    if matches!(target, PlayTarget::NextUp) {
        send_mpv_script_message("play-next-up").await?;
        return Ok(());
    }

    // Step 1: Resolve item ID
    let item_id = match &target {
        PlayTarget::RecentPinchflat => {
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

    // Step 2: Send IPC play-source hint (non-fatal)
    let source = "strategy";
    if let Err(e) = send_mpv_script_message_with_args("set-play-source", &[source]).await {
        eprintln!("media-control: IPC hint failed (non-fatal): {e}");
    }

    // Step 3: Get resume position
    let resume_ticks = match jf.get_item_resume_ticks(&item_id).await {
        Ok(ticks) => ticks,
        Err(e) => {
            eprintln!("media-control: failed to get resume position (starting from beginning): {e}");
            0
        }
    };

    // Step 4: Find session and play
    let session = jf
        .find_mpv_session()
        .await?
        .ok_or("Shim not connected")?;
    jf.play_item_with_resume(&session.id, &item_id, resume_ticks)
        .await?;

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
