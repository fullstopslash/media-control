//! Playback status query.
//!
//! Queries mpv IPC for current playback state and outputs human-readable
//! or JSON format. Designed for status bar integration (waybar/polybar).

use super::query_mpv_property;

/// Format seconds as MM:SS.
fn format_time(seconds: f64) -> String {
    let total = seconds as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

/// Query mpv for playback status and print it.
///
/// Returns `Ok(true)` if playing, `Ok(false)` if not playing.
/// The caller should set the exit code based on the return value.
pub async fn status(json_output: bool) -> Result<bool, Box<dyn std::error::Error>> {
    // Try to query media-title first — if this fails, nothing is playing
    let title = match query_mpv_property("media-title").await {
        Ok(v) => v.as_str().unwrap_or("Unknown").to_string(),
        Err(_) => {
            if json_output {
                println!(r#"{{"playing":false}}"#);
            }
            return Ok(false);
        }
    };

    // Query remaining properties (best-effort, default on failure)
    let position = query_mpv_property("playback-time")
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let duration = query_mpv_property("duration")
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let paused = query_mpv_property("pause")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if json_output {
        let output = serde_json::json!({
            "playing": true,
            "title": title,
            "position": position,
            "duration": duration,
            "paused": paused,
        });
        println!("{output}");
    } else {
        println!("Playing: {title}");
        println!(
            "Position: {} / {}",
            format_time(position),
            format_time(duration)
        );
        println!("Paused: {}", if paused { "yes" } else { "no" });
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "0:00");
    }

    #[test]
    fn format_time_seconds_only() {
        assert_eq!(format_time(45.7), "0:45");
    }

    #[test]
    fn format_time_minutes_and_seconds() {
        assert_eq!(format_time(754.2), "12:34");
    }

    #[test]
    fn format_time_over_an_hour() {
        assert_eq!(format_time(3661.0), "61:01");
    }
}
