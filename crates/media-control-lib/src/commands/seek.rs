//! mpv seek by absolute percentage.

use super::send_mpv_ipc_command;
use crate::error::Result;

/// Seek to an absolute percentage position in mpv.
///
/// `percent` should be 0–100. Socket failure is sufficient guard.
pub async fn seek(percent: u8) -> Result<()> {
    let pct = percent.min(100);
    let payload = format!(r#"{{"command":["seek",{},"absolute-percent"]}}"#, pct);
    send_mpv_ipc_command(&payload).await
}
