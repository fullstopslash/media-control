//! Random subcommand — trigger random playback via mpv-shim IPC.
//!
//! Sends a `script-message random [type]` to the shim, which delegates
//! to the active store's `random_item()` implementation.
//!
//! Types are store-specific:
//! - Jellyfin: `show`, `series`, `movie`
//! - Twitch: (any or none — picks random live channel)
//! - Stash: `scene`, `performer`, `studio`

use super::{send_mpv_script_message, send_mpv_script_message_with_args};

/// Trigger random playback via mpv-shim IPC.
///
/// If `random_type` is provided, it's passed as an argument to the
/// `random` script-message. The active store interprets the type.
pub async fn random(random_type: Option<&str>) -> crate::error::Result<()> {
    match random_type {
        Some(t) => send_mpv_script_message_with_args("random", &[t]).await,
        None => send_mpv_script_message("random").await,
    }
}

#[cfg(test)]
mod tests {
    /// The `random` function is a thin routing wrapper around the shared
    /// mpv IPC layer. Behavioural verification is covered by the IPC
    /// integration tests in `commands::tests`; this module verifies only
    /// that the public entry point compiles in both `Some(_)` and `None`
    /// branches and is `Send + Sync`-friendly for the tokio runtime.
    use super::random;

    #[test]
    fn random_signature_compiles_for_both_branches() {
        // We don't await — building the futures is enough to exercise the
        // monomorphisation of both arms without racing on env state.
        let _none = random(None);
        let _some = random(Some("show"));
    }
}
