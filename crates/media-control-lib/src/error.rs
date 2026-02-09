//! Error types for the media-control library.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias using [`MediaControlError`].
pub type Result<T> = std::result::Result<T, MediaControlError>;

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

    /// Configuration error.
    #[error("config error: {kind}")]
    Config {
        kind: ConfigErrorKind,
        path: Option<PathBuf>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Jellyfin API error.
    #[error("jellyfin error: {kind}")]
    Jellyfin {
        kind: JellyfinErrorKind,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// No matching media window found.
    #[error("no media window found matching pattern")]
    WindowNotFound,

    /// General I/O error.
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// Regex compilation error.
    #[error("regex error")]
    Regex(#[from] regex::Error),

    /// JSON parsing error.
    #[error("JSON error")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error.
    #[error("TOML parse error")]
    Toml(#[from] toml::de::Error),

    /// HTTP request error.
    #[error("HTTP request error")]
    Http(#[from] reqwest::Error),
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

/// Specific kinds of configuration errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorKind {
    /// Configuration file not found.
    NotFound,
    /// Failed to parse configuration file.
    ParseError,
    /// Configuration validation failed.
    ValidationError,
}

impl std::fmt::Display for ConfigErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "file not found"),
            Self::ParseError => write!(f, "parse error"),
            Self::ValidationError => write!(f, "validation error"),
        }
    }
}

/// Specific kinds of Jellyfin API errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JellyfinErrorKind {
    /// Authentication failed.
    AuthFailed,
    /// No active session found.
    SessionNotFound,
    /// API request failed.
    ApiError,
    /// Credentials file not found or invalid.
    CredentialsError,
}

impl std::fmt::Display for JellyfinErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthFailed => write!(f, "authentication failed"),
            Self::SessionNotFound => write!(f, "no active session found"),
            Self::ApiError => write!(f, "API request failed"),
            Self::CredentialsError => write!(f, "credentials error"),
        }
    }
}

impl From<crate::hyprland::HyprlandError> for MediaControlError {
    fn from(err: crate::hyprland::HyprlandError) -> Self {
        use crate::hyprland::HyprlandError;
        match err {
            HyprlandError::MissingEnvVar(_) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::SocketNotFound,
                source: None,
            },
            HyprlandError::ConnectionFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ConnectionFailed,
                source: Some(e),
            },
            HyprlandError::WriteFailed(e) | HyprlandError::ReadFailed(e) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ConnectionFailed,
                source: Some(e),
            },
            HyprlandError::JsonParseFailed(_) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ParseError,
                source: None,
            },
            HyprlandError::CommandFailed(msg) => Self::HyprlandIpc {
                kind: HyprlandIpcErrorKind::ConnectionFailed,
                source: Some(std::io::Error::new(std::io::ErrorKind::Other, msg)),
            },
        }
    }
}

impl MediaControlError {
    /// Create a Hyprland IPC connection error.
    pub fn hyprland_connection(source: std::io::Error) -> Self {
        Self::HyprlandIpc {
            kind: HyprlandIpcErrorKind::ConnectionFailed,
            source: Some(source),
        }
    }

    /// Create a Hyprland IPC parse error.
    pub fn hyprland_parse() -> Self {
        Self::HyprlandIpc {
            kind: HyprlandIpcErrorKind::ParseError,
            source: None,
        }
    }

    /// Create a Hyprland socket not found error.
    pub fn hyprland_socket_not_found() -> Self {
        Self::HyprlandIpc {
            kind: HyprlandIpcErrorKind::SocketNotFound,
            source: None,
        }
    }

    /// Create a config not found error.
    pub fn config_not_found(path: impl Into<PathBuf>) -> Self {
        Self::Config {
            kind: ConfigErrorKind::NotFound,
            path: Some(path.into()),
            source: None,
        }
    }

    /// Create a config parse error.
    pub fn config_parse(path: impl Into<PathBuf>, source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Config {
            kind: ConfigErrorKind::ParseError,
            path: Some(path.into()),
            source: Some(source.into()),
        }
    }

    /// Create a config validation error.
    pub fn config_validation(msg: impl std::fmt::Display) -> Self {
        Self::Config {
            kind: ConfigErrorKind::ValidationError,
            path: None,
            source: Some(msg.to_string().into()),
        }
    }

    /// Create a Jellyfin authentication error.
    pub fn jellyfin_auth() -> Self {
        Self::Jellyfin {
            kind: JellyfinErrorKind::AuthFailed,
            source: None,
        }
    }

    /// Create a Jellyfin session not found error.
    pub fn jellyfin_session_not_found() -> Self {
        Self::Jellyfin {
            kind: JellyfinErrorKind::SessionNotFound,
            source: None,
        }
    }

    /// Create a Jellyfin API error.
    pub fn jellyfin_api(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Jellyfin {
            kind: JellyfinErrorKind::ApiError,
            source: Some(source.into()),
        }
    }

    /// Create a Jellyfin credentials error.
    pub fn jellyfin_credentials() -> Self {
        Self::Jellyfin {
            kind: JellyfinErrorKind::CredentialsError,
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err: MediaControlError = io_err.into();
        assert!(matches!(err, MediaControlError::Io(_)));
    }

    #[test]
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
    fn hyprland_errors_display_correctly() {
        let err = MediaControlError::hyprland_socket_not_found();
        assert!(err.to_string().contains("socket not found"));

        let err = MediaControlError::hyprland_parse();
        assert!(err.to_string().contains("parse"));
    }

    #[test]
    fn config_errors_display_correctly() {
        let err = MediaControlError::config_not_found("/test/path");
        assert!(err.to_string().contains("not found"));

        let err = MediaControlError::config_validation("missing field");
        assert!(err.to_string().contains("validation"));
    }

    #[test]
    fn jellyfin_errors_display_correctly() {
        let err = MediaControlError::jellyfin_auth();
        assert!(err.to_string().contains("authentication"));

        let err = MediaControlError::jellyfin_session_not_found();
        assert!(err.to_string().contains("session"));
    }

    #[test]
    fn window_not_found_displays_correctly() {
        let err = MediaControlError::WindowNotFound;
        assert!(err.to_string().contains("no media window"));
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
