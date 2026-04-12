//! mpv seek by absolute percentage.

use super::send_mpv_ipc_command;
use crate::error::Result;

/// Seek to an absolute percentage position in mpv.
///
/// `percent` should be 0–100; values above 100 are clamped.
pub async fn seek(percent: u8) -> Result<()> {
    let pct = percent.min(100);
    let payload = format!(r#"{{"command":["seek",{},"absolute-percent"]}}"#, pct);
    send_mpv_ipc_command(&payload).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn seek_payload_format() {
        let pct: u8 = 50;
        let payload = format!(r#"{{"command":["seek",{},"absolute-percent"]}}"#, pct.min(100));
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let cmd = parsed["command"].as_array().unwrap();
        assert_eq!(cmd[0], "seek");
        assert_eq!(cmd[1], 50);
        assert_eq!(cmd[2], "absolute-percent");
    }

    #[test]
    fn seek_clamps_over_100() {
        let pct: u8 = 255;
        let clamped = pct.min(100);
        assert_eq!(clamped, 100);
    }

    #[test]
    fn seek_zero() {
        let pct: u8 = 0;
        let payload = format!(r#"{{"command":["seek",{},"absolute-percent"]}}"#, pct.min(100));
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(parsed["command"][1], 0);
    }
}
