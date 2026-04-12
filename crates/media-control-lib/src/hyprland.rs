//! Hyprland IPC client for window management.
//!
//! Provides async communication with Hyprland's Unix socket for window queries and commands.
//! This replaces the bash script's socat-based communication with native Rust async I/O.
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::hyprland::HyprlandClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = HyprlandClient::new()?;
//!
//! // Get all windows
//! let clients = client.get_clients().await?;
//! for c in &clients {
//!     println!("{}: {} ({})", c.address, c.title, c.class);
//! }
//!
//! // Dispatch a command
//! client.dispatch("focuswindow address:0x12345678").await?;
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Errors that can occur during Hyprland IPC operations.
#[derive(Debug, Error)]
pub enum HyprlandError {
    #[error("missing environment variable: {0}")]
    MissingEnvVar(&'static str),

    #[error("socket connection failed: {0}")]
    ConnectionFailed(#[source] std::io::Error),

    #[error("socket write failed: {0}")]
    WriteFailed(#[source] std::io::Error),

    #[error("socket read failed: {0}")]
    ReadFailed(#[source] std::io::Error),

    #[error("JSON parse failed: {0}")]
    JsonParseFailed(#[source] serde_json::Error),

    #[error("command failed: {0}")]
    CommandFailed(String),
}

/// Result type for Hyprland operations.
pub type Result<T> = std::result::Result<T, HyprlandError>;

/// Workspace data embedded in Client and Monitor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
}

/// Window/client data from Hyprland's `j/clients` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub address: String,
    pub mapped: bool,
    pub hidden: bool,
    pub at: [i32; 2],
    pub size: [i32; 2],
    pub workspace: Workspace,
    pub floating: bool,
    pub pinned: bool,
    /// Fullscreen state: 0 = none, 1 = maximized, 2 = fullscreen
    #[serde(default)]
    pub fullscreen: u8,
    pub monitor: i32,
    #[serde(default)]
    pub pid: i32,
    pub class: String,
    pub title: String,
    #[serde(rename = "focusHistoryID")]
    pub focus_history_id: i32,
}

/// Monitor data from Hyprland's `j/monitors` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub id: i32,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub focused: bool,
    pub active_workspace: Workspace,
}

/// Async client for Hyprland IPC communication.
///
/// Connects to Hyprland's Unix socket at `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`.
#[derive(Debug, Clone)]
pub struct HyprlandClient {
    socket_path: PathBuf,
}

impl HyprlandClient {
    /// Create a new client from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if `XDG_RUNTIME_DIR` or `HYPRLAND_INSTANCE_SIGNATURE` are not set.
    pub fn new() -> Result<Self> {
        let runtime_dir = env::var("XDG_RUNTIME_DIR")
            .map_err(|_| HyprlandError::MissingEnvVar("XDG_RUNTIME_DIR"))?;
        let instance_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .map_err(|_| HyprlandError::MissingEnvVar("HYPRLAND_INSTANCE_SIGNATURE"))?;

        let socket_path = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(instance_sig)
            .join(".socket.sock");

        Ok(Self { socket_path })
    }

    /// Create a client with a custom socket path.
    ///
    /// Useful for testing with a mock server or connecting to a non-standard socket.
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send a raw command and return the response.
    ///
    /// This is the low-level method that all other methods use.
    /// The socket protocol is:
    /// 1. Connect to Unix socket
    /// 2. Write command bytes
    /// 3. Read response (may be empty, "ok", or JSON)
    ///
    /// Times out after 2 seconds if Hyprland is unresponsive.
    /// Retries once after 50ms on connection failure (covers transient
    /// socket busy during compositor transitions like fullscreen toggle).
    pub async fn command(&self, cmd: &str) -> Result<String> {
        const TIMEOUT: Duration = Duration::from_secs(2);

        let result = tokio::time::timeout(TIMEOUT, self.command_inner(cmd))
            .await
            .map_err(|_| {
                HyprlandError::ConnectionFailed(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Hyprland IPC timed out after 2s",
                ))
            })?;

        match result {
            Ok(resp) => Ok(resp),
            Err(HyprlandError::ConnectionFailed(_)) => {
                // Retry once after brief pause — Hyprland socket can refuse
                // connections during compositor transitions
                tokio::time::sleep(Duration::from_millis(50)).await;
                tokio::time::timeout(TIMEOUT, self.command_inner(cmd))
                    .await
                    .map_err(|_| {
                        HyprlandError::ConnectionFailed(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "Hyprland IPC timed out after 2s (retry)",
                        ))
                    })?
            }
            Err(e) => Err(e),
        }
    }

    async fn command_inner(&self, cmd: &str) -> Result<String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(HyprlandError::ConnectionFailed)?;

        stream
            .write_all(cmd.as_bytes())
            .await
            .map_err(HyprlandError::WriteFailed)?;

        // Shutdown write half to signal end of command
        stream
            .shutdown()
            .await
            .map_err(HyprlandError::WriteFailed)?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(HyprlandError::ReadFailed)?;

        Ok(response)
    }

    /// Execute a dispatch command.
    ///
    /// Wraps the action with `dispatch` prefix and validates the response.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use media_control_lib::hyprland::HyprlandClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HyprlandClient::new()?;
    /// client.dispatch("focuswindow address:0x12345678").await?;
    /// client.dispatch("movewindowpixel exact 100 200,address:0x12345678").await?;
    /// # Ok(())
    /// # }
    /// ```
    /// Check if a Hyprland IPC response indicates success.
    #[inline]
    fn is_success(response: &str) -> bool {
        response.is_empty() || response.starts_with("ok")
    }

    /// Send a command and require a success response.
    async fn command_ok(&self, cmd: &str) -> Result<()> {
        let response = self.command(cmd).await?;
        if Self::is_success(&response) {
            Ok(())
        } else {
            Err(HyprlandError::CommandFailed(response))
        }
    }

    pub async fn dispatch(&self, action: &str) -> Result<()> {
        self.command_ok(&format!("dispatch {action}")).await
    }

    /// Execute multiple commands in a batch.
    ///
    /// Uses `[[BATCH]]` prefix with semicolon-separated commands.
    /// More efficient than multiple individual commands.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use media_control_lib::hyprland::HyprlandClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HyprlandClient::new()?;
    /// client.batch(&[
    ///     "dispatch movewindowpixel exact 100 200,address:0x12345678",
    ///     "dispatch resizewindowpixel exact 640 360,address:0x12345678",
    /// ]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn batch(&self, commands: &[&str]) -> Result<()> {
        if commands.is_empty() {
            return Ok(());
        }
        self.command_ok(&format!("[[BATCH]]{}", commands.join("; "))).await
    }

    /// Get all window clients.
    ///
    /// Queries Hyprland's `j/clients` endpoint and parses the JSON response.
    pub async fn get_clients(&self) -> Result<Vec<Client>> {
        let response = self.command("j/clients").await?;

        if response.is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str(&response).map_err(HyprlandError::JsonParseFailed)
    }

    /// Get the currently active/focused window.
    ///
    /// Returns `None` if no window is focused (e.g., focus on desktop).
    pub async fn get_active_window(&self) -> Result<Option<Client>> {
        let response = self.command("j/activewindow").await?;

        // Empty response or empty object means no active window
        if response.is_empty() || response.trim() == "{}" {
            return Ok(None);
        }

        let client: Client =
            serde_json::from_str(&response).map_err(HyprlandError::JsonParseFailed)?;

        // Additional check: empty address indicates no real window
        if client.address.is_empty() {
            return Ok(None);
        }

        Ok(Some(client))
    }

    /// Get all monitors.
    ///
    /// Queries Hyprland's `j/monitors` endpoint and parses the JSON response.
    pub async fn get_monitors(&self) -> Result<Vec<Monitor>> {
        let response = self.command("j/monitors").await?;

        if response.is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str(&response).map_err(HyprlandError::JsonParseFailed)
    }

    /// Get the focused monitor.
    ///
    /// Returns the monitor where `focused == true`.
    pub async fn get_focused_monitor(&self) -> Result<Option<Monitor>> {
        let monitors = self.get_monitors().await?;
        Ok(monitors.into_iter().find(|m| m.focused))
    }

    /// Set a keyword (config option) temporarily.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use media_control_lib::hyprland::HyprlandClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HyprlandClient::new()?;
    /// client.keyword("cursor:no_warps", "true").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn keyword(&self, key: &str, value: &str) -> Result<()> {
        self.command_ok(&format!("keyword {key} {value}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_deserialization() {
        let json = r#"{
            "address": "0x55a1b2c3d4e5",
            "mapped": true,
            "hidden": false,
            "at": [100, 200],
            "size": [640, 360],
            "workspace": {"id": 1, "name": "1"},
            "floating": true,
            "pinned": true,
            "fullscreen": 0,
            "monitor": 0,
            "class": "mpv",
            "title": "video.mp4 - mpv",
            "focusHistoryID": 0
        }"#;

        let client: Client = serde_json::from_str(json).expect("failed to parse client");

        assert_eq!(client.address, "0x55a1b2c3d4e5");
        assert!(client.mapped);
        assert!(!client.hidden);
        assert_eq!(client.at, [100, 200]);
        assert_eq!(client.size, [640, 360]);
        assert_eq!(client.workspace.id, 1);
        assert_eq!(client.workspace.name, "1");
        assert!(client.floating);
        assert!(client.pinned);
        assert_eq!(client.fullscreen, 0);
        assert_eq!(client.monitor, 0);
        assert_eq!(client.class, "mpv");
        assert_eq!(client.title, "video.mp4 - mpv");
        assert_eq!(client.focus_history_id, 0);
    }

    #[test]
    fn test_client_fullscreen_states() {
        // Test fullscreen = 1 (maximized)
        let json = r#"{
            "address": "0x1",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [1920, 1080],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "pinned": false,
            "fullscreen": 1,
            "monitor": 0,
            "class": "firefox",
            "title": "Mozilla Firefox",
            "focusHistoryID": 1
        }"#;

        let client: Client = serde_json::from_str(json).expect("failed to parse");
        assert_eq!(client.fullscreen, 1);

        // Test fullscreen = 2 (actual fullscreen)
        let json2 = json.replace("\"fullscreen\": 1", "\"fullscreen\": 2");
        let client2: Client = serde_json::from_str(&json2).expect("failed to parse");
        assert_eq!(client2.fullscreen, 2);
    }

    #[test]
    fn test_client_missing_fullscreen_defaults_to_zero() {
        // Some older Hyprland versions may omit fullscreen field
        let json = r#"{
            "address": "0x1",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [640, 480],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "pinned": false,
            "monitor": 0,
            "class": "kitty",
            "title": "Terminal",
            "focusHistoryID": 2
        }"#;

        let client: Client = serde_json::from_str(json).expect("failed to parse");
        assert_eq!(client.fullscreen, 0);
    }

    #[test]
    fn test_monitor_deserialization() {
        let json = r#"{
            "id": 0,
            "name": "DP-1",
            "width": 2560,
            "height": 1440,
            "x": 0,
            "y": 0,
            "focused": true,
            "activeWorkspace": {"id": 1, "name": "1"}
        }"#;

        let monitor: Monitor = serde_json::from_str(json).expect("failed to parse monitor");

        assert_eq!(monitor.id, 0);
        assert_eq!(monitor.name, "DP-1");
        assert_eq!(monitor.width, 2560);
        assert_eq!(monitor.height, 1440);
        assert_eq!(monitor.x, 0);
        assert_eq!(monitor.y, 0);
        assert!(monitor.focused);
        assert_eq!(monitor.active_workspace.id, 1);
        assert_eq!(monitor.active_workspace.name, "1");
    }

    #[test]
    fn test_clients_array_deserialization() {
        let json = r#"[
            {
                "address": "0x1",
                "mapped": true,
                "hidden": false,
                "at": [0, 0],
                "size": [800, 600],
                "workspace": {"id": 1, "name": "1"},
                "floating": false,
                "pinned": false,
                "fullscreen": 0,
                "monitor": 0,
                "class": "kitty",
                "title": "Terminal",
                "focusHistoryID": 0
            },
            {
                "address": "0x2",
                "mapped": true,
                "hidden": false,
                "at": [100, 100],
                "size": [640, 360],
                "workspace": {"id": 1, "name": "1"},
                "floating": true,
                "pinned": true,
                "fullscreen": 0,
                "monitor": 0,
                "class": "mpv",
                "title": "video.mp4",
                "focusHistoryID": 1
            }
        ]"#;

        let clients: Vec<Client> = serde_json::from_str(json).expect("failed to parse clients");

        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].class, "kitty");
        assert_eq!(clients[1].class, "mpv");
        assert!(clients[1].floating);
        assert!(clients[1].pinned);
    }

    #[test]
    fn test_monitors_array_deserialization() {
        let json = r#"[
            {
                "id": 0,
                "name": "DP-1",
                "width": 2560,
                "height": 1440,
                "x": 0,
                "y": 0,
                "focused": false,
                "activeWorkspace": {"id": 1, "name": "1"}
            },
            {
                "id": 1,
                "name": "HDMI-A-1",
                "width": 1920,
                "height": 1080,
                "x": 2560,
                "y": 0,
                "focused": true,
                "activeWorkspace": {"id": 2, "name": "2"}
            }
        ]"#;

        let monitors: Vec<Monitor> = serde_json::from_str(json).expect("failed to parse monitors");

        assert_eq!(monitors.len(), 2);
        assert!(!monitors[0].focused);
        assert!(monitors[1].focused);
        assert_eq!(monitors[1].x, 2560);
    }

    #[test]
    fn test_workspace_equality() {
        let ws1 = Workspace {
            id: 1,
            name: "main".to_string(),
        };
        let ws2 = Workspace {
            id: 1,
            name: "main".to_string(),
        };
        let ws3 = Workspace {
            id: 2,
            name: "other".to_string(),
        };

        assert_eq!(ws1, ws2);
        assert_ne!(ws1, ws3);
    }

    #[test]
    fn test_batch_command_formatting() {
        // Test that batch commands would be formatted correctly
        let commands = [
            "dispatch movewindowpixel exact 100 200,address:0x1",
            "dispatch resizewindowpixel exact 640 360,address:0x1",
        ];

        let batch_cmd = format!("[[BATCH]]{}", commands.join("; "));

        assert_eq!(
            batch_cmd,
            "[[BATCH]]dispatch movewindowpixel exact 100 200,address:0x1; dispatch resizewindowpixel exact 640 360,address:0x1"
        );
    }

    #[test]
    fn test_dispatch_command_formatting() {
        let action = "focuswindow address:0x12345678";
        let cmd = format!("dispatch {action}");

        assert_eq!(cmd, "dispatch focuswindow address:0x12345678");
    }

    #[test]
    fn test_empty_clients_response() {
        let json = "[]";
        let clients: Vec<Client> = serde_json::from_str(json).expect("failed to parse");
        assert!(clients.is_empty());
    }

    #[test]
    fn test_picture_in_picture_window() {
        let json = r#"{
            "address": "0xpip123",
            "mapped": true,
            "hidden": false,
            "at": [1600, 900],
            "size": [320, 180],
            "workspace": {"id": 1, "name": "1"},
            "floating": true,
            "pinned": true,
            "fullscreen": 0,
            "monitor": 0,
            "class": "firefox",
            "title": "Picture-in-Picture",
            "focusHistoryID": 5
        }"#;

        let client: Client = serde_json::from_str(json).expect("failed to parse");

        assert_eq!(client.title, "Picture-in-Picture");
        assert!(client.floating);
        assert!(client.pinned);
    }

    #[test]
    fn test_jellyfin_media_player_window() {
        let json = r#"{
            "address": "0xjelly",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [1920, 1080],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "pinned": false,
            "fullscreen": 2,
            "monitor": 0,
            "class": "com.github.iwalton3.jellyfin-media-player",
            "title": "Jellyfin Media Player",
            "focusHistoryID": 0
        }"#;

        let client: Client = serde_json::from_str(json).expect("failed to parse");

        assert!(client.class.contains("jellyfin"));
        assert_eq!(client.fullscreen, 2);
    }
}
