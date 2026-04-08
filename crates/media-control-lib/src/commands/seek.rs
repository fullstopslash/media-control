//! mpv seek by absolute percentage.

use super::{CommandContext, require_mpv_window, send_mpv_ipc_command};
use crate::error::Result;

/// Seek to an absolute percentage position in mpv.
///
/// `percent` should be 0–100. If no mpv window is found, silently succeeds.
pub async fn seek(ctx: &CommandContext, percent: u8) -> Result<()> {
    if require_mpv_window(ctx).await?.is_none() {
        return Ok(());
    }
    let pct = percent.min(100);
    let payload = format!(r#"{{"command":["seek",{},"absolute-percent"]}}"#, pct);
    send_mpv_ipc_command(&payload).await
}
