//! Error types for the media-control library.
//!
//! `MediaControlError` is the unified error type returned by command-level
//! APIs. Subsystem errors (`config::ConfigError`, `jellyfin::JellyfinError`,
//! `hyprland::HyprlandError`) bridge into it via `#[from]` so the original
//! source — and its `Display`/`source()` chain — is preserved end-to-end.

use thiserror::Error;

/// Result type alias using [`MediaControlError`].
pub type Result<T> = std::result::Result<T, MediaControlError>;

/// Specific kinds of mpv IPC errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpvIpcErrorKind {
    /// No valid mpv IPC socket found.
    NoSocket,
    /// Connection or write timed out.
    Timeout,
    /// Connection failed on all socket paths.
    ConnectionFailed,
    /// mpv returned an error response.
    ResponseError,
}

impl std::fmt::Display for MpvIpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSocket => write!(f, "no mpv IPC socket found"),
            Self::Timeout => write!(f, "connection timed out"),
            Self::ConnectionFailed => write!(f, "connection failed"),
            Self::ResponseError => write!(f, "mpv returned an error"),
        }
    }
}

/// Specific kinds of Hyprland IPC errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyprlandIpcErrorKind {
    /// Failed to connect to Hyprland socket.
    ConnectionFailed,
    /// Failed to parse response from Hyprland.
    ParseError,
    /// Socket path not found (HYPRLAND_INSTANCE_SIGNATURE not set).
    SocketNotFound,
}

impl std::fmt::Display for HyprlandIpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed => write!(f, "failed to connect to Hyprland socket"),
            Self::ParseError => write!(f, "failed to parse Hyprland response"),
            Self::SocketNotFound => write!(f, "Hyprland socket not found"),
        }
    }
}

/// Errors that can occur during media control operations.
#[derive(Debug, Error)]
pub enum MediaControlError {
    /// Hyprland IPC communication error.
    #[error("hyprland IPC error: {kind}")]
    HyprlandIpc {
        kind: HyprlandIpcErrorKind,
        #[source]
        source: Option<std::io::Error>,
    },

    /// Configuration error (typed source, no `Box<dyn Error>`).
    #[error("config error: {0}")]
    Config(#[from] crate::config::ConfigError),

    /// Jellyfin API error (typed source, no `Box<dyn Error>`).
    #[error("jellyfin error: {0}")]
    Jellyfin(#[from] crate::jellyfin::JellyfinError),

    /// No matching media window found.
    #[error("no media window found matching pattern")]
    WindowNotFound,

    /// mpv IPC communication error.
    #[error("mpv IPC error: {kind}")]
    MpvIpc {
        kind: MpvIpcErrorKind,
        message: String,
    },

    /// General I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Regex compilation error.
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    /// HTTP request error.
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
}

impl From<crate::hyprland::HyprlandError> for MediaControlError {
    fn from(err: crate::hyprland::HyprlandError) -> Self {
        use crate::hyprland::HyprlandError;
        match err {
            HyprlandError::MissingEnvVar(_) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::SocketNotFound,
                source: None,
            },
            HyprlandError::ConnectionFailed(e)
            | HyprlandError::WriteFailed(e)
            | HyprlandError::ReadFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ConnectionFailed,
                source: Some(e),
            },
            HyprlandError::JsonParseFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ParseError,
                source: Some(std::io::Error::other(e.to_string())),
            },
            HyprlandError::CommandFailed(msg) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ParseError,
                source: Some(std::io::Error::other(msg)),
            },
        }
    }
}

impl MediaControlError {
    /// Create an mpv IPC no-socket error.
    pub fn mpv_no_socket() -> Self {
        Self::MpvIpc {
            kind: MpvIpcErrorKind::NoSocket,
            message:
                "no mpv IPC socket found (tried $MPV_IPC_SOCKET, /tmp/mpv-shim, /tmp/mpvctl-jshim)"
                    .into(),
        }
    }

    /// Create an mpv IPC connection failed error.
    pub fn mpv_connection_failed(msg: impl Into<String>) -> Self {
        Self::MpvIpc {
            kind: MpvIpcErrorKind::ConnectionFailed,
            message: msg.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err: MediaControlError = io_err.into();
        assert!(matches!(err, MediaControlError::Io(_)));
    }

    #[test]
    #[allow(clippy::invalid_regex)]
    fn regex_error_converts() {
        let regex_err = regex::Regex::new("[invalid").unwrap_err();
        let err: MediaControlError = regex_err.into();
        assert!(matches!(err, MediaControlError::Regex(_)));
    }

    #[test]
    fn json_error_converts() {
        let json_err: serde_json::Error = serde_json::from_str::<String>("not json").unwrap_err();
        let err: MediaControlError = json_err.into();
        assert!(matches!(err, MediaControlError::Json(_)));
    }

    #[test]
    fn toml_error_converts() {
        let toml_err: toml::de::Error = toml::from_str::<String>("not = valid { toml").unwrap_err();
        let err: MediaControlError = toml_err.into();
        assert!(matches!(err, MediaControlError::Toml(_)));
    }

    #[test]
    fn config_error_bridges_via_from_and_preserves_source() {
        // ConfigError::Parse wraps a real toml::de::Error; the bridge must
        // preserve the chain so callers can still inspect the cause.
        let toml_err: toml::de::Error = toml::from_str::<String>("not = valid { toml").unwrap_err();
        let cfg_err = crate::config::ConfigError::Parse(toml_err);
        let err: MediaControlError = cfg_err.into();
        assert!(matches!(err, MediaControlError::Config(_)));
        // source() should yield the inner ConfigError, then its toml source.
        assert!(err.source().is_some(), "Config variant must expose source");
    }

    #[test]
    fn jellyfin_error_bridges_via_from_and_preserves_source() {
        let jf_err = crate::jellyfin::JellyfinError::NoMpvSession;
        let err: MediaControlError = jf_err.into();
        assert!(matches!(err, MediaControlError::Jellyfin(_)));
        // The Display chain must mention "no active MPV session" — proving
        // the inner error wasn't lost during the bridge.
        assert!(err.to_string().contains("no active MPV session"));
    }

    #[test]
    fn hyprland_io_source_preserved() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "boom");
        let err = MediaControlError::HyprlandIpc {
            kind: HyprlandIpcErrorKind::ConnectionFailed,
            source: Some(io_err),
        };
        let src = err.source().expect("source must be present");
        assert!(src.to_string().contains("boom"));
    }

    #[test]
    fn window_not_found_displays_correctly() {
        let err = MediaControlError::WindowNotFound;
        assert!(err.to_string().contains("no media window"));
    }

    #[test]
    fn mpv_ipc_errors_display_correctly() {
        let err = MediaControlError::mpv_no_socket();
        assert!(err.to_string().contains("no mpv IPC socket found"));

        let err = MediaControlError::mpv_connection_failed("test failure");
        assert!(err.to_string().contains("connection failed"));
        // Inner message must surface in the chain (check via Debug since
        // mpv variants encode the message in a non-source field).
        assert!(format!("{err:?}").contains("test failure"));
    }

    #[test]
    fn mpv_ipc_error_kind_display() {
        assert_eq!(
            MpvIpcErrorKind::NoSocket.to_string(),
            "no mpv IPC socket found"
        );
        assert_eq!(MpvIpcErrorKind::Timeout.to_string(), "connection timed out");
        assert_eq!(
            MpvIpcErrorKind::ConnectionFailed.to_string(),
            "connection failed"
        );
        assert_eq!(
            MpvIpcErrorKind::ResponseError.to_string(),
            "mpv returned an error"
        );
    }

    #[test]
    fn hyprland_error_kind_display() {
        assert_eq!(
            HyprlandIpcErrorKind::ConnectionFailed.to_string(),
            "failed to connect to Hyprland socket"
        );
        assert_eq!(
            HyprlandIpcErrorKind::ParseError.to_string(),
            "failed to parse Hyprland response"
        );
        assert_eq!(
            HyprlandIpcErrorKind::SocketNotFound.to_string(),
            "Hyprland socket not found"
        );
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MediaControlError>();
    }

    #[test]
    fn result_type_works() {
        fn returns_result() -> Result<i32> {
            Ok(42)
        }
        assert_eq!(returns_result().unwrap(), 42);

        fn returns_error() -> Result<i32> {
            Err(MediaControlError::WindowNotFound)
        }
        assert!(returns_error().is_err());
    }
}
