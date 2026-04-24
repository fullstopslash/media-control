//! Playback status query.
//!
//! Queries mpv IPC for current playback state and outputs human-readable
//! or JSON format. Designed for status bar integration (waybar/polybar).

use super::query_mpv_property;

/// Coerce an `f64` from mpv into a finite, non-negative `f64`.
///
/// mpv occasionally surfaces NaN (uninitialised property), `Infinity`
/// (live streams), and small negatives during seek. Normalise here so
/// downstream arithmetic and JSON serialisation stay well-defined —
/// `serde_json` rejects NaN/Infinity, which would otherwise turn a
/// recoverable status query into a hard error.
fn sanitize(v: f64) -> f64 {
    if v.is_finite() && v >= 0.0 { v } else { 0.0 }
}

/// Format seconds as `M:SS`.
///
/// Inputs are sanitised via [`sanitize`] before truncation so NaN, infinity
/// and negative values all collapse to `0:00` instead of producing
/// nonsense like `307445734561825860:15` (which a saturating `f64 as u64`
/// cast would yield for `f64::INFINITY`).
fn format_time(seconds: f64) -> String {
    // After `sanitize`, value is in `[0, f64::MAX]`. Cap at `u64::MAX`
    // before the cast so the saturating-cast behaviour is explicit rather
    // than implicit.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = sanitize(seconds).min(u64::MAX as f64) as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}

/// Query mpv for playback status and print it.
///
/// Returns `Ok(true)` if playing, `Ok(false)` if not playing.
/// The caller should set the exit code based on the return value.
///
/// Output goes to stdout: a JSON object when `json_output` is true (status-bar
/// integration), otherwise three human-readable lines. When nothing is
/// playing and `json_output` is false, no output is produced — the exit code
/// is the only signal.
///
/// # Errors
///
/// This function does not return mpv-IPC errors directly: a failed
/// `media-title` query is interpreted as "nothing is playing" and yields
/// `Ok(false)`. The `Result` is retained for forward-compatibility (e.g. if
/// a future serialisation step is added).
pub async fn status(json_output: bool) -> crate::error::Result<bool> {
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

    // Query remaining properties concurrently (best-effort, default on failure)
    let (pos_result, dur_result, pause_result) = tokio::join!(
        query_mpv_property("playback-time"),
        query_mpv_property("duration"),
        query_mpv_property("pause"),
    );
    // `sanitize` guards against NaN/Infinity that would otherwise make
    // `serde_json::to_string` fail on the JSON branch below.
    let position = sanitize(pos_result.ok().and_then(|v| v.as_f64()).unwrap_or(0.0));
    let duration = sanitize(dur_result.ok().and_then(|v| v.as_f64()).unwrap_or(0.0));
    let paused = pause_result.ok().and_then(|v| v.as_bool()).unwrap_or(false);

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

    #[test]
    fn format_time_negative() {
        // Negative values (can happen during mpv seeks) should not panic
        assert_eq!(format_time(-5.0), "0:00");
    }

    #[test]
    fn format_time_large_value() {
        // Very large values should not panic
        let result = format_time(1e15);
        assert!(result.contains(':'));
    }

    #[test]
    fn format_time_infinity() {
        // f64::INFINITY would saturate-cast to u64::MAX without sanitisation,
        // yielding a 19-digit minutes count. Sanitise -> "0:00".
        assert_eq!(format_time(f64::INFINITY), "0:00");
        assert_eq!(format_time(f64::NEG_INFINITY), "0:00");
    }

    #[test]
    fn sanitize_rejects_non_finite_and_negative() {
        assert_eq!(sanitize(f64::NAN), 0.0);
        assert_eq!(sanitize(f64::INFINITY), 0.0);
        assert_eq!(sanitize(f64::NEG_INFINITY), 0.0);
        assert_eq!(sanitize(-1.0), 0.0);
        assert_eq!(sanitize(0.0), 0.0);
        assert_eq!(sanitize(42.5), 42.5);
    }

    #[test]
    fn format_time_exactly_60() {
        assert_eq!(format_time(60.0), "1:00");
    }

    #[test]
    fn format_time_nan() {
        // NaN should not panic (saturates to 0 via max(0.0))
        let result = format_time(f64::NAN);
        assert_eq!(result, "0:00");
    }
}
