//! Play subcommand — start playback via mpv-shim IPC.
//!
//! Targets:
//! - `next-up`: play next-up from the currently active store
//! - `<store-name>`: switch to that store and play its next-up (twitch, jellyfin, pinchflat, etc.)
//! - `<item-id>`: play a specific item by hex ID (shim auto-detects store)

use super::{send_mpv_script_message, send_mpv_script_message_with_args, validate_ipc_token_len};

/// What to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayTarget {
    /// Play next-up from the active store.
    NextUp,
    /// Switch to a named store/context and play its next-up item.
    Store(String),
    /// A specific item ID (hex, 32+ chars).
    ItemId(String),
}

/// Maximum accepted length for an item-id token.
///
/// Real shim/Jellyfin IDs are 32 hex chars; we allow some headroom for
/// store-prefixed forms but cap to defend the IPC path against pathological
/// CLI input (e.g. a megabyte-long argument).
const ITEM_ID_MAX_LEN: usize = 128;

/// Maximum accepted length for a store/context name.
///
/// Real store names are short tokens like `jellyfin`, `twitch`, `pinchflat`,
/// `stash`. Cap to defend the IPC path against pathological CLI input — the
/// name is interpolated into a `script-message play-<name>` payload sent over
/// the mpv IPC socket.
const STORE_NAME_MAX_LEN: usize = 64;

impl PlayTarget {
    /// Parse a target string from the CLI.
    ///
    /// Recognises three forms:
    /// 1. `"next-up"` — play next-up from the active store
    /// 2. A 32-128 character ASCII-hex string — a specific item ID
    /// 3. Anything else — treated as a store/context name
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "next-up" => Self::NextUp,
            id if (32..=ITEM_ID_MAX_LEN).contains(&id.len())
                && id.chars().all(|c| c.is_ascii_hexdigit()) =>
            {
                Self::ItemId(id.to_string())
            }
            // Everything else is a store/context name
            name => Self::Store(name.to_string()),
        }
    }

    /// Parse from an owned `String`, moving rather than cloning when the
    /// caller already owns the buffer. Equivalent to [`Self::parse`] for a
    /// borrowed `&str` but avoids the second allocation in the `ItemId` /
    /// `Store` branches.
    #[must_use]
    pub fn parse_owned(s: String) -> Self {
        match s.as_str() {
            "next-up" => Self::NextUp,
            id if (32..=ITEM_ID_MAX_LEN).contains(&id.len())
                && id.chars().all(|c| c.is_ascii_hexdigit()) =>
            {
                Self::ItemId(s)
            }
            _ => Self::Store(s),
        }
    }
}

/// Start playback via mpv-shim IPC.
///
/// Accepts an owned `String` so the parsed `ItemId` / `Store` variant can
/// move the buffer instead of re-allocating from a `&str`. CLI callers that
/// already hold a `String` should pass it through directly; tests and code
/// that only have a borrowed `&str` can `.to_string()` at the call site.
///
/// All playback delegation goes through the shim — media-control is just
/// the control plane, not the resolution engine.
///
/// # Errors
///
/// - Returns [`crate::error::MediaControlError::MpvIpc`] with kind `NoSocket`
///   if the mpv IPC socket is unavailable.
/// - Returns [`crate::error::MediaControlError::InvalidArgument`] if the
///   parsed `Store` name is empty or exceeds [`STORE_NAME_MAX_LEN`]
///   (defends the IPC path against unbounded or malformed CLI input).
///   `ItemId` is bounded at parse time by [`ITEM_ID_MAX_LEN`] and so
///   cannot trigger the length branch.
pub async fn play(target: String) -> crate::error::Result<()> {
    match PlayTarget::parse_owned(target) {
        PlayTarget::NextUp => send_mpv_script_message("play-next-up").await,
        PlayTarget::Store(name) => {
            // Empty store name is rejected here (validate_ipc_token_len
            // refuses empty values) so we never ship `script-message play-`
            // to mpv with a trailing dash.
            validate_ipc_token_len("play target", &name, STORE_NAME_MAX_LEN)?;
            send_mpv_script_message(&format!("play-{name}")).await
        }
        PlayTarget::ItemId(id) => {
            send_mpv_script_message_with_args("play-item", &[id.as_str()]).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_next_up() {
        assert_eq!(PlayTarget::parse("next-up"), PlayTarget::NextUp);
    }

    #[test]
    fn parse_store_name() {
        assert_eq!(
            PlayTarget::parse("twitch"),
            PlayTarget::Store("twitch".to_string())
        );
        assert_eq!(
            PlayTarget::parse("jellyfin"),
            PlayTarget::Store("jellyfin".to_string())
        );
        assert_eq!(
            PlayTarget::parse("pinchflat"),
            PlayTarget::Store("pinchflat".to_string())
        );
    }

    #[test]
    fn parse_item_id() {
        assert_eq!(
            PlayTarget::parse("a5c0a87b1d058d1b7e70f5406ee274e2"),
            PlayTarget::ItemId("a5c0a87b1d058d1b7e70f5406ee274e2".to_string())
        );
    }

    #[test]
    fn parse_short_hex_is_store() {
        // 31 chars — too short for an item ID
        assert_eq!(
            PlayTarget::parse("a5c0a87b1d058d1b7e70f5406ee274e"),
            PlayTarget::Store("a5c0a87b1d058d1b7e70f5406ee274e".to_string())
        );
    }

    #[test]
    fn parse_hex_with_non_hex_char_is_store() {
        // Has 'g' — not valid hex
        assert_eq!(
            PlayTarget::parse("a5c0a87b1d058d1b7e70f5406ee274g2"),
            PlayTarget::Store("a5c0a87b1d058d1b7e70f5406ee274g2".to_string())
        );
    }

    #[test]
    fn parse_empty_string_is_store() {
        assert_eq!(PlayTarget::parse(""), PlayTarget::Store(String::new()));
    }

    #[test]
    fn parse_overlong_hex_is_store() {
        // Above ITEM_ID_MAX_LEN, valid hex must NOT be classified as an item ID;
        // otherwise a megabyte-long CLI arg would be sent verbatim over IPC.
        let huge = "a".repeat(ITEM_ID_MAX_LEN + 1);
        assert_eq!(PlayTarget::parse(&huge), PlayTarget::Store(huge));
    }

    #[test]
    fn parse_max_len_hex_is_item_id() {
        let max = "b".repeat(ITEM_ID_MAX_LEN);
        assert_eq!(PlayTarget::parse(&max), PlayTarget::ItemId(max));
    }

    /// Defense in depth: even though the parser routes long valid hex away
    /// from `ItemId`, an overlong store name must still be rejected by the
    /// IPC entry point before it hits the socket.
    #[tokio::test]
    async fn play_rejects_overlong_store_name() {
        use crate::error::MediaControlError;
        // 65+ char non-hex string parses as Store and exceeds STORE_NAME_MAX_LEN.
        let huge = "z".repeat(STORE_NAME_MAX_LEN + 1);
        let err = play(huge).await.expect_err("must reject");
        // Input validation must surface as InvalidArgument, never as a
        // misleading IPC connection failure.
        match err {
            MediaControlError::InvalidArgument(msg) => {
                assert!(
                    msg.contains("too long"),
                    "message should mention overflow: {msg}"
                );
            }
            other => panic!("expected InvalidArgument length-check error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn play_accepts_max_len_store_name() {
        use crate::error::MediaControlError;
        // Exactly at the cap. Outcome depends on socket availability; the
        // length check itself must not fire — so we must NOT see
        // InvalidArgument for input at the boundary.
        let max = "z".repeat(STORE_NAME_MAX_LEN);
        if let Err(MediaControlError::InvalidArgument(msg)) = play(max).await {
            panic!("length-check should not fire at the boundary: {msg}");
        }
    }

    /// Empty store name must be rejected — interpolating `""` would ship
    /// `script-message play-` (trailing dash) to mpv. The empty-string
    /// guard inside `validate_ipc_token_len` catches this; we lock in the
    /// surface here so a future caller can't accidentally relax it.
    #[tokio::test]
    async fn play_rejects_empty_store_name() {
        use crate::error::MediaControlError;
        let err = play(String::new()).await.expect_err("must reject empty");
        match err {
            MediaControlError::InvalidArgument(msg) => {
                assert!(
                    msg.contains("empty"),
                    "message should mention emptiness: {msg}"
                );
            }
            other => panic!("expected InvalidArgument empty-check error, got: {other:?}"),
        }
    }

    /// `parse_owned` should yield the same enum variants as `parse` while
    /// moving the buffer into `ItemId` / `Store` rather than re-allocating.
    #[test]
    fn parse_owned_matches_parse() {
        for s in [
            "next-up".to_string(),
            "twitch".to_string(),
            "jellyfin".to_string(),
            "a5c0a87b1d058d1b7e70f5406ee274e2".to_string(),
            String::new(),
        ] {
            assert_eq!(PlayTarget::parse_owned(s.clone()), PlayTarget::parse(&s));
        }
    }
}
