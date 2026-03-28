//! Tag the currently-playing item as "keep" to prevent auto-deletion.
//!
//! Broadcasts `script-message keep` to ALL known mpv sockets. Each mpv
//! instance has its own context handler (shim for Jellyfin, lua for Stash)
//! that acts only when relevant content is playing.

use super::{require_mpv_window, send_to_mpv_socket, CommandContext};
use crate::error::Result;

/// All mpv sockets that might have keepable content.
const KEEP_SOCKETS: &[&str] = &["/tmp/mpvctl-jshim", "/tmp/mpvctl-stash", "/tmp/mpvctl0"];

/// Tag the current item as "keep" across all mpv instances.
pub async fn keep(ctx: &CommandContext) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }

    let payload = r#"{"command":["script-message","keep"]}"#;
    let mut sent = false;

    for socket_path in KEEP_SOCKETS {
        if send_to_mpv_socket(socket_path, payload).await {
            sent = true;
        }
    }

    if !sent {
        eprintln!("media-control: keep: no mpv socket responded");
    }

    Ok(())
}
