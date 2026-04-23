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
///
/// Only kinds that production code actually constructs are exposed; the
/// previous `Timeout`/`ResponseError` variants were never produced and were
/// removed to keep the surface honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MpvIpcErrorKind {
    /// No valid mpv IPC socket found.
    NoSocket,
    /// Connection failed on all socket paths.
    ConnectionFailed,
}

impl std::fmt::Display for MpvIpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSocket => write!(f, "no mpv IPC socket found"),
            Self::ConnectionFailed => write!(f, "connection failed"),
        }
    }
}

/// Specific kinds of Hyprland IPC errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HyprlandIpcErrorKind {
    /// Failed to connect to Hyprland socket.
    ConnectionFailed,
    /// I/O failure on an already-established Hyprland socket
    /// (write or read after a successful connect). Distinct from
    /// `ConnectionFailed`, which is reserved for connect-time failures.
    IoFailed,
    /// Failed to parse response from Hyprland.
    ParseError,
    /// Socket path not found (HYPRLAND_INSTANCE_SIGNATURE not set).
    SocketNotFound,
    /// Hyprland accepted the request but replied with a non-OK status
    /// (semantic rejection — distinct from a parse failure).
    Rejected,
}

impl std::fmt::Display for HyprlandIpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed => write!(f, "failed to connect to Hyprland socket"),
            Self::IoFailed => write!(f, "Hyprland socket I/O failed"),
            Self::ParseError => write!(f, "failed to parse Hyprland response"),
            Self::SocketNotFound => write!(f, "Hyprland socket not found"),
            Self::Rejected => write!(f, "Hyprland rejected command"),
        }
    }
}

/// Errors that can occur during media control operations.
#[derive(Debug, Error)]
#[non_exhaustive]
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
    ///
    /// `message` carries actionable detail (e.g. the offending input length,
    /// the attempted socket paths) and is included in `Display` so users can
    /// diagnose without cracking open `Debug`.
    #[error("mpv IPC error: {kind}: {message}")]
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

    /// Invalid argument supplied by the caller.
    ///
    /// Used when input fails validation *before* any IPC or external call is
    /// attempted (e.g. a CLI token exceeds a length cap). Distinct from
    /// `MpvIpc`/`HyprlandIpc` so callers and logs can tell "user input was
    /// rejected" apart from "remote endpoint failed".
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

impl From<crate::hyprland::HyprlandError> for MediaControlError {
    fn from(err: crate::hyprland::HyprlandError) -> Self {
        use crate::hyprland::HyprlandError;
        match err {
            // Preserve the missing env-var name in the source chain so
            // operators can tell from the log alone which env var was the
            // culprit (XDG_RUNTIME_DIR vs HYPRLAND_INSTANCE_SIGNATURE).
            HyprlandError::MissingEnvVar(name) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::SocketNotFound,
                source: Some(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("missing environment variable: {name}"),
                )),
            },
            // Distinct from `MissingEnvVar`: the var *is* set but its content
            // failed validation (path traversal, NUL byte, separator, ...).
            // Operators reading the log can tell "you forgot to set it" from
            // "you set it to something dangerous".
            HyprlandError::InvalidEnvVar(name) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::SocketNotFound,
                source: Some(std::io::Error::other(format!(
                    "invalid environment variable: {name}"
                ))),
            },
            HyprlandError::ConnectionFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ConnectionFailed,
                source: Some(e),
            },
            // Write/Read failures occur on an already-established stream;
            // keeping them distinct from `ConnectionFailed` lets callers and
            // logs tell "could not connect" apart from "lost the line mid-IO".
            HyprlandError::WriteFailed(e) | HyprlandError::ReadFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::IoFailed,
                source: Some(e),
            },
            // Route JSON parse failures into the typed `Json` variant so the
            // original `serde_json::Error` (with its line/column position) is
            // preserved end-to-end instead of being flattened to a string.
            HyprlandError::JsonParseFailed(e) => Self::Json(e),
            // Hyprland accepted the IPC request but replied non-OK — that's a
            // semantic rejection, not a transport or parse failure.
            HyprlandError::CommandFailed(msg) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::Rejected,
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

    /// Create an invalid-argument error for input that failed pre-IPC validation.
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::InvalidArgument(msg.into())
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
        assert_eq!(
            MpvIpcErrorKind::ConnectionFailed.to_string(),
            "connection failed"
        );
    }

    #[test]
    fn hyprland_error_kind_display() {
        assert_eq!(
            HyprlandIpcErrorKind::ConnectionFailed.to_string(),
            "failed to connect to Hyprland socket"
        );
        assert_eq!(
            HyprlandIpcErrorKind::IoFailed.to_string(),
            "Hyprland socket I/O failed"
        );
        assert_eq!(
            HyprlandIpcErrorKind::ParseError.to_string(),
            "failed to parse Hyprland response"
        );
        assert_eq!(
            HyprlandIpcErrorKind::SocketNotFound.to_string(),
            "Hyprland socket not found"
        );
        assert_eq!(
            HyprlandIpcErrorKind::Rejected.to_string(),
            "Hyprland rejected command"
        );
    }

    #[test]
    fn hyprland_write_failed_bridges_to_io_failed_kind() {
        // `WriteFailed` must surface as `IoFailed`, not `ConnectionFailed`:
        // by the time write fails, the connect has already succeeded.
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "write boom");
        let hypr_err = crate::hyprland::HyprlandError::WriteFailed(io_err);
        let err: MediaControlError = hypr_err.into();
        match err {
            MediaControlError::HyprlandIpc { kind, source } => {
                assert_eq!(kind, HyprlandIpcErrorKind::IoFailed);
                let src = source.expect("io::Error source must be preserved");
                assert_eq!(src.kind(), std::io::ErrorKind::BrokenPipe);
                assert!(src.to_string().contains("write boom"));
            }
            other => panic!("expected HyprlandIpc, got {other:?}"),
        }
    }

    #[test]
    fn hyprland_read_failed_bridges_to_io_failed_kind() {
        // Same bridge symmetry for `ReadFailed`: the source io::Error must
        // pass through with its kind and message intact.
        let io_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "read boom");
        let hypr_err = crate::hyprland::HyprlandError::ReadFailed(io_err);
        let err: MediaControlError = hypr_err.into();
        match err {
            MediaControlError::HyprlandIpc { kind, source } => {
                assert_eq!(kind, HyprlandIpcErrorKind::IoFailed);
                let src = source.expect("io::Error source must be preserved");
                assert_eq!(src.kind(), std::io::ErrorKind::UnexpectedEof);
                assert!(src.to_string().contains("read boom"));
            }
            other => panic!("expected HyprlandIpc, got {other:?}"),
        }
    }

    #[test]
    fn hyprland_connection_failed_still_bridges_to_connection_failed_kind() {
        // Regression guard: the `ConnectionFailed` arm must NOT be folded
        // into `IoFailed` after the split.
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no connect");
        let hypr_err = crate::hyprland::HyprlandError::ConnectionFailed(io_err);
        let err: MediaControlError = hypr_err.into();
        match err {
            MediaControlError::HyprlandIpc { kind, source } => {
                assert_eq!(kind, HyprlandIpcErrorKind::ConnectionFailed);
                let src = source.expect("io::Error source must be preserved");
                assert_eq!(src.kind(), std::io::ErrorKind::ConnectionRefused);
            }
            other => panic!("expected HyprlandIpc, got {other:?}"),
        }
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MediaControlError>();
    }

    #[test]
    fn config_io_error_display_includes_underlying_message() {
        // The Config(_) bridge must surface the inner Display so users can
        // diagnose missing/permission-denied config files from the log alone.
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "perm boom");
        let cfg: crate::config::ConfigError = io.into();
        let err: MediaControlError = cfg.into();
        let msg = err.to_string();
        assert!(msg.contains("config error"), "got: {msg}");
        // The chain must reach the io message; Display flattens via the
        // inner ConfigError::Io's `{0}` substitution.
        assert!(msg.contains("perm boom"), "io message lost: {msg}");
    }

    #[test]
    fn jellyfin_credentials_too_large_displays_with_size() {
        // Size cap errors must show the offending size + cap so the user can
        // diagnose without reaching for a debugger.
        let jf = crate::jellyfin::JellyfinError::CredentialsTooLarge {
            size: 999_999,
            max: 65_536,
        };
        let err: MediaControlError = jf.into();
        let msg = err.to_string();
        assert!(msg.contains("999999"), "size missing: {msg}");
        assert!(msg.contains("65536"), "cap missing: {msg}");
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
