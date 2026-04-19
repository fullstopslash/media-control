//! mpv seek by absolute percentage.

use super::send_mpv_ipc_command;
use crate::error::Result;

/// Build the seek IPC payload. Percent is clamped to 0..=100 (defense in depth;
/// the CLI parser already enforces this range).
fn build_payload(percent: u8) -> String {
    let pct = percent.min(100);
    serde_json::json!({"command": ["seek", pct, "absolute-percent"]}).to_string()
}

/// Seek to an absolute percentage position in mpv.
///
/// `percent` should be 0–100; values above 100 are clamped.
///
/// # Errors
///
/// Returns [`crate::error::MediaControlError::MpvIpc`] with kind `NoSocket`
/// if no mpv IPC socket is available.
pub async fn seek(percent: u8) -> Result<()> {
    send_mpv_ipc_command(&build_payload(percent)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_payload_format() {
        let parsed: serde_json::Value = serde_json::from_str(&build_payload(50)).unwrap();
        let cmd = parsed["command"].as_array().unwrap();
        assert_eq!(cmd[0], "seek");
        assert_eq!(cmd[1], 50);
        assert_eq!(cmd[2], "absolute-percent");
    }

    #[test]
    fn seek_clamps_over_100() {
        let parsed: serde_json::Value = serde_json::from_str(&build_payload(255)).unwrap();
        assert_eq!(parsed["command"][1], 100);
    }

    #[test]
    fn seek_zero() {
        let parsed: serde_json::Value = serde_json::from_str(&build_payload(0)).unwrap();
        assert_eq!(parsed["command"][1], 0);
    }

    #[test]
    fn seek_boundary_100() {
        let parsed: serde_json::Value = serde_json::from_str(&build_payload(100)).unwrap();
        assert_eq!(parsed["command"][1], 100);
    }
}
