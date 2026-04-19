//! Play subcommand — start playback via mpv-shim IPC.
//!
//! Targets:
//! - `next-up`: play next-up from the currently active store
//! - `<store-name>`: switch to that store and play its next-up (twitch, jellyfin, pinchflat, etc.)
//! - `<item-id>`: play a specific item by hex ID (shim auto-detects store)

use super::{send_mpv_script_message, send_mpv_script_message_with_args};

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
}

/// Start playback via mpv-shim IPC.
///
/// All playback delegation goes through the shim — media-control is just
/// the control plane, not the resolution engine.
///
/// # Errors
///
/// - Returns `mpv_no_socket` if the mpv IPC socket is unavailable.
/// - Returns an `InvalidArgument` error if the parsed `Store` name exceeds
///   `STORE_NAME_MAX_LEN` (defends the IPC path against unbounded CLI input).
pub async fn play(target_str: &str) -> crate::error::Result<()> {
    match PlayTarget::parse(target_str) {
        PlayTarget::NextUp => send_mpv_script_message("play-next-up").await,
        PlayTarget::Store(name) => {
            if name.len() > STORE_NAME_MAX_LEN {
                return Err(crate::error::MediaControlError::mpv_connection_failed(
                    format!(
                        "play target too long: {} bytes (max {STORE_NAME_MAX_LEN})",
                        name.len()
                    ),
                ));
            }
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
        use crate::error::{MediaControlError, MpvIpcErrorKind};
        // 65+ char non-hex string parses as Store and exceeds STORE_NAME_MAX_LEN.
        let huge = "z".repeat(STORE_NAME_MAX_LEN + 1);
        let err = play(&huge).await.expect_err("must reject");
        // The length check uses ConnectionFailed with a "too long" message.
        match err {
            MediaControlError::MpvIpc { kind, message } => {
                assert_eq!(kind, MpvIpcErrorKind::ConnectionFailed);
                assert!(
                    message.contains("too long"),
                    "message should mention overflow: {message}"
                );
            }
            other => panic!("expected MpvIpc length-check error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn play_accepts_max_len_store_name() {
        use crate::error::{MediaControlError, MpvIpcErrorKind};
        // Exactly at the cap. Outcome depends on socket availability; the
        // length check itself must not fire.
        let max = "z".repeat(STORE_NAME_MAX_LEN);
        // If a connection error fires, it must be a real socket failure,
        // not the length check.
        if let Err(MediaControlError::MpvIpc { kind, message }) = play(&max).await
            && kind == MpvIpcErrorKind::ConnectionFailed
        {
            assert!(
                !message.contains("too long"),
                "length-check should not fire at the boundary: {message}"
            );
        }
    }
}
