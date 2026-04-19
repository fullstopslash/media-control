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

use serde::{Deserialize, Deserializer, Serialize};
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use regex::Regex;

/// Errors that can occur during Hyprland IPC operations.
#[derive(Debug, Error)]
pub enum HyprlandError {
    #[error("missing environment variable: {0}")]
    MissingEnvVar(&'static str),

    /// The named environment variable is set, but its content failed
    /// validation (path traversal, NUL byte, separator, empty, ...).
    /// Distinct from `MissingEnvVar` so log readers can tell "unset" from
    /// "dangerously set".
    #[error("invalid environment variable: {0}")]
    InvalidEnvVar(&'static str),

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

/// Lazily-compiled validator for Hyprland window addresses.
///
/// Hyprland addresses are pointers rendered as `0x` followed by 1–32 hex
/// digits. Anything else (including a value carrying a `;` to inject a
/// second IPC command) is rejected at the deserialisation boundary so the
/// `*_action` helpers — which interpolate the address into batch strings
/// like `"focuswindow address:{addr}"` — cannot be tricked into dispatching
/// attacker-controlled commands.
fn address_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Pre-validated literal — `expect` is sound: this regex is fixed at
        // build time and exercised by `is_valid_address` tests.
        Regex::new(r"^0x[0-9A-Fa-f]{1,32}$").expect("address regex must compile")
    })
}

/// Returns `true` iff `addr` matches the canonical Hyprland address shape
/// `^0x[0-9A-Fa-f]{1,32}$`.
///
/// Used by the `Client::address` deserialiser and as a debug-assertion
/// guard inside the `*_action` builders.
#[must_use]
pub(crate) fn is_valid_address(addr: &str) -> bool {
    address_regex().is_match(addr)
}

/// Custom serde deserialiser for `Client::address`.
///
/// Validates the incoming string against [`is_valid_address`]. A non-matching
/// value is replaced with an empty string and a `tracing::warn!` is emitted —
/// any subsequent `dispatch focuswindow address:` becomes a harmless no-op
/// (Hyprland silently ignores an empty address) instead of dispatching the
/// injected payload.
fn deserialize_address<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    if is_valid_address(&raw) {
        Ok(raw)
    } else {
        tracing::warn!("Hyprland returned non-hex window address: {raw}; treating as unknown");
        Ok(String::new())
    }
}

/// Window/client data from Hyprland's `j/clients` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    /// Window address. Validated at deserialisation against
    /// `^0x[0-9A-Fa-f]{1,32}$`; malformed values are replaced with `""` so
    /// downstream `dispatch focuswindow address:{addr}` interpolation can
    /// never inject a second IPC command. See [`deserialize_address`].
    #[serde(deserialize_with = "deserialize_address")]
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

impl Client {
    /// True iff the client is mapped and not hidden — the canonical
    /// "user-visible window" predicate used by every focus / overlap query.
    ///
    /// Centralised so the `c.mapped && !c.hidden` pattern lives in exactly
    /// one place and a future visibility flag (e.g. `urgent`, workspace
    /// occlusion) can extend it without hunting down every filter site.
    #[inline]
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.mapped && !self.hidden
    }

    /// True iff this client is the currently-focused window
    /// (`focus_history_id == 0`).
    #[inline]
    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.focus_history_id == 0
    }

    /// True iff this client has *ever* been focused — i.e. its history id is
    /// non-negative. Hyprland uses `-1` for windows that were created but
    /// never received focus (these should be excluded from focus-restore
    /// candidates).
    #[inline]
    #[must_use]
    pub fn has_focus_history(&self) -> bool {
        self.focus_history_id >= 0
    }
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

/// Build the absolute path to one of Hyprland's per-instance Unix sockets
/// (e.g. `.socket.sock` for commands, `.socket2.sock` for events).
///
/// Sanitizes both env vars *and* the `name` argument to defend against
/// path-traversal injection: the runtime dir must be absolute and free of
/// `..`; the instance signature and `name` must each be a single non-empty
/// component free of separators and `..`.
///
/// # Errors
///
/// Returns [`HyprlandError::MissingEnvVar`] if `XDG_RUNTIME_DIR` or
/// `HYPRLAND_INSTANCE_SIGNATURE` are unset or contain unsafe components,
/// or if `name` is empty / contains separators / contains `..`.
pub fn runtime_socket_path(name: &str) -> Result<PathBuf> {
    /// Reject empty / multi-component / traversal-laden path components.
    /// NUL bytes are rejected because most filesystems treat them as a
    /// terminator, which would silently truncate the resulting path.
    fn is_safe_component(s: &str) -> bool {
        !s.is_empty()
            && !s.contains('/')
            && !s.contains('\\')
            && !s.contains('\0')
            && !s.contains("..")
            && s != "."
    }

    // The `name` argument is supplied by the caller, not the environment.
    // Treat a bad value as a validation failure on the env var that would
    // otherwise hold this kind of component (the instance signature).
    if !is_safe_component(name) {
        return Err(HyprlandError::InvalidEnvVar("HYPRLAND_INSTANCE_SIGNATURE"));
    }

    let runtime_dir =
        env::var("XDG_RUNTIME_DIR").map_err(|_| HyprlandError::MissingEnvVar("XDG_RUNTIME_DIR"))?;
    let instance_sig = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| HyprlandError::MissingEnvVar("HYPRLAND_INSTANCE_SIGNATURE"))?;

    let runtime = PathBuf::from(&runtime_dir);
    if !runtime.is_absolute()
        || runtime
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        // Var was set, but its content is unsafe — this is `Invalid`, not `Missing`.
        return Err(HyprlandError::InvalidEnvVar("XDG_RUNTIME_DIR"));
    }
    // Reuse `is_safe_component` for instance_sig — same threat model
    // (path-traversal, NUL-byte truncation, separator injection). Also
    // reject signatures starting with `.` so an attacker can't squat on a
    // hidden directory the user might not notice in `ls`.
    if !is_safe_component(&instance_sig) || instance_sig.starts_with('.') {
        return Err(HyprlandError::InvalidEnvVar("HYPRLAND_INSTANCE_SIGNATURE"));
    }

    Ok(runtime.join("hypr").join(instance_sig).join(name))
}

impl HyprlandClient {
    /// Create a new client from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if `XDG_RUNTIME_DIR` or `HYPRLAND_INSTANCE_SIGNATURE` are not set.
    pub fn new() -> Result<Self> {
        Ok(Self {
            socket_path: runtime_socket_path(".socket.sock")?,
        })
    }

    /// Create a client with a custom socket path.
    ///
    /// Useful for testing with a mock server or connecting to a non-standard socket.
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Per-attempt timeout. The full `command()` may make two attempts so
    /// the worst-case wall time is roughly `2 * COMMAND_TIMEOUT + RETRY_DELAY`.
    const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
    /// Delay between the initial attempt and the single retry.
    const RETRY_DELAY: Duration = Duration::from_millis(50);

    /// Run `command_inner` with a single per-attempt timeout, mapping
    /// elapsed → `ConnectionFailed(TimedOut)` so the caller can decide
    /// whether to retry.
    async fn command_attempt(&self, cmd: &str, label: &'static str) -> Result<String> {
        tokio::time::timeout(Self::COMMAND_TIMEOUT, self.command_inner(cmd))
            .await
            .map_err(|_| {
                HyprlandError::ConnectionFailed(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    label,
                ))
            })?
    }

    /// Send a raw command and return the response.
    ///
    /// This is the low-level method that all other methods use.
    /// The socket protocol is:
    /// 1. Connect to Unix socket
    /// 2. Write command bytes
    /// 3. Read response (may be empty, "ok", or JSON)
    ///
    /// Times out after 2 seconds per attempt. Retries once after 50ms only on
    /// `ConnectionFailed` — covers transient socket-busy during compositor
    /// transitions (e.g. fullscreen toggle) where the connect itself fails
    /// before any bytes have been written.
    ///
    /// `WriteFailed` and `ReadFailed` are *not* retried: a partial write may
    /// have already dispatched the command on the compositor side, and
    /// re-sending a non-idempotent dispatch (e.g. `dispatch fullscreen`)
    /// would toggle state twice. `CommandFailed` (semantic errors from
    /// Hyprland) are also never retried.
    pub async fn command(&self, cmd: &str) -> Result<String> {
        match self
            .command_attempt(cmd, "Hyprland IPC timed out after 2s")
            .await
        {
            Ok(resp) => Ok(resp),
            Err(HyprlandError::ConnectionFailed(_)) => {
                tokio::time::sleep(Self::RETRY_DELAY).await;
                self.command_attempt(cmd, "Hyprland IPC timed out after 2s (attempt 2)")
                    .await
            }
            Err(e) => Err(e),
        }
    }

    /// Hard cap on the response read. Real Hyprland responses (even a fully
    /// populated `j/clients` payload) are well under this; the cap exists so
    /// a misbehaving or malicious peer can't drive us OOM.
    const MAX_RESPONSE_BYTES: u64 = 65_536;

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

        // Bound the read so a runaway peer can't exhaust memory; 64 KiB is
        // far more headroom than any real Hyprland response needs.
        let mut response = String::new();
        stream
            .take(Self::MAX_RESPONSE_BYTES)
            .read_to_string(&mut response)
            .await
            .map_err(HyprlandError::ReadFailed)?;

        Ok(response)
    }

    /// Check if a Hyprland IPC response indicates success.
    ///
    /// Accepts an empty body or a body whose every line is exactly `"ok"`
    /// (one line per command in a `[[BATCH]]` response). This is stricter
    /// than the previous `starts_with("ok\n")` check, which would accept
    /// partial-failure batches like `"ok\nerror: bad\nok\n"` because the
    /// first command happened to succeed. It also rejects substring-style
    /// false positives like `"oklahoma"` and `"okok"`.
    #[inline]
    fn is_success(response: &str) -> bool {
        if response.is_empty() {
            return true;
        }
        // `lines()` strips the trailing `\n` and skips an empty trailing
        // record, so `"ok\n"` and `"ok\nok\n"` both yield only `"ok"` lines.
        // An empty intermediate line (e.g. `"ok\n\nok\n"`) is treated as
        // failure — Hyprland never emits that shape for success.
        let mut had_line = false;
        for line in response.lines() {
            if line != "ok" {
                return false;
            }
            had_line = true;
        }
        had_line
    }

    /// Send a command and require a success response.
    ///
    /// On failure, the returned error includes both the failed command and
    /// the response from Hyprland for debuggability.
    async fn command_ok(&self, cmd: &str) -> Result<()> {
        let response = self.command(cmd).await?;
        if Self::is_success(&response) {
            Ok(())
        } else {
            Err(HyprlandError::CommandFailed(format!(
                "cmd={cmd:?} response={response:?}"
            )))
        }
    }

    /// Execute a dispatch command.
    ///
    /// Wraps the action with `dispatch` prefix and validates the response.
    /// On failure, the returned error includes the full dispatched command
    /// string and Hyprland's response.
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
    pub async fn dispatch(&self, action: &str) -> Result<()> {
        self.command_ok(&format!("dispatch {action}")).await
    }

    /// Execute multiple commands in a batch.
    ///
    /// Uses `[[BATCH]]` prefix with semicolon-separated commands.
    /// More efficient than multiple individual commands.
    ///
    /// Each entry is sent verbatim — callers needing `dispatch ` prefixing
    /// should prefer [`Self::dispatch_batch`] which centralises that.
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
        self.command_ok(&format!("[[BATCH]]{}", commands.join("; ")))
            .await
    }

    /// Execute multiple dispatch *actions* in a batch.
    ///
    /// Each entry is the bare action body (e.g. `pin address:0xabc`); this
    /// method prepends the `dispatch ` token to each before joining. Pairs
    /// with the bare-action helpers in `commands::*_action` so callers no
    /// longer need to thread the literal `dispatch ` token through their
    /// `format!()` calls.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use media_control_lib::hyprland::HyprlandClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = HyprlandClient::new()?;
    /// client.dispatch_batch(&[
    ///     "movewindowpixel exact 100 200,address:0x12345678",
    ///     "resizewindowpixel exact 640 360,address:0x12345678",
    /// ]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn dispatch_batch(&self, actions: &[&str]) -> Result<()> {
        if actions.is_empty() {
            return Ok(());
        }
        // The trailing `format!("[[BATCH]]{joined}")` reallocates anyway, so
        // hand-tuned capacity here just adds noise. Build the joined body
        // with a plain `String` and let `format!` size the final buffer.
        let mut joined = String::new();
        for (i, a) in actions.iter().enumerate() {
            if i > 0 {
                joined.push_str("; ");
            }
            joined.push_str("dispatch ");
            joined.push_str(a);
        }
        self.command_ok(&format!("[[BATCH]]{joined}")).await
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
            "address": "0xabc123",
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
            "address": "0xdeadbeef",
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

    // --- Address validation tests ---

    #[test]
    fn is_valid_address_accepts_canonical_forms() {
        assert!(is_valid_address("0x1"));
        assert!(is_valid_address("0xABCDEF"));
        assert!(is_valid_address("0xabcdef0123456789"));
        // Exactly 32 hex digits — the upper bound.
        assert!(is_valid_address(&format!("0x{}", "f".repeat(32))));
    }

    #[test]
    fn is_valid_address_rejects_non_hex_and_injection() {
        // Empty / missing prefix.
        assert!(!is_valid_address(""));
        assert!(!is_valid_address("abc"));
        assert!(!is_valid_address("0x"));
        // Non-hex character.
        assert!(!is_valid_address("0xpip123"));
        assert!(!is_valid_address("0xjelly"));
        // 33 hex digits — exceeds the upper bound.
        assert!(!is_valid_address(&format!("0x{}", "f".repeat(33))));
        // The canonical injection vector this guard defends against.
        assert!(!is_valid_address("0xABC;dispatch exec rm ~"));
        assert!(!is_valid_address("0xABC dispatch exec foo"));
        assert!(!is_valid_address("0xABC\ndispatch exec foo"));
    }

    #[test]
    fn deserialize_address_accepts_valid_hex() {
        let json = r#"{
            "address": "0x55a1b2c3d4e5",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [100, 100],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "pinned": false,
            "fullscreen": 0,
            "monitor": 0,
            "class": "x",
            "title": "y",
            "focusHistoryID": 0
        }"#;
        let client: Client = serde_json::from_str(json).expect("parse");
        assert_eq!(client.address, "0x55a1b2c3d4e5");
    }

    #[test]
    fn deserialize_address_replaces_injection_payload_with_empty() {
        // A malicious / buggy compositor that returns an injection payload
        // must be neutralised at the deserialisation boundary so the bare
        // address never reaches `format!("focuswindow address:{addr}")`.
        let json = r#"{
            "address": "0xABC;dispatch exec rm ~",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [100, 100],
            "workspace": {"id": 1, "name": "1"},
            "floating": false,
            "pinned": false,
            "fullscreen": 0,
            "monitor": 0,
            "class": "x",
            "title": "y",
            "focusHistoryID": 0
        }"#;
        let client: Client = serde_json::from_str(json).expect("parse");
        assert_eq!(client.address, "", "injection payload must be neutralised");
    }

    // --- is_success tests ---

    #[test]
    fn is_success_accepts_empty_response() {
        assert!(HyprlandClient::is_success(""));
    }

    #[test]
    fn is_success_accepts_bare_ok() {
        assert!(HyprlandClient::is_success("ok"));
        assert!(HyprlandClient::is_success("ok\n"));
    }

    #[test]
    fn is_success_accepts_multi_ok_batch_response() {
        // Hyprland returns one `ok\n` per command in a `[[BATCH]]` response.
        assert!(HyprlandClient::is_success("ok\nok\nok\n"));
        assert!(HyprlandClient::is_success("ok\nok"));
    }

    #[test]
    fn is_success_rejects_error_text() {
        assert!(!HyprlandClient::is_success("error: unknown command"));
    }

    #[test]
    fn is_success_rejects_substring_false_positives() {
        // The previous `starts_with("ok")` check would have accepted these.
        assert!(!HyprlandClient::is_success("oklahoma"));
        assert!(!HyprlandClient::is_success("okok"));
        assert!(!HyprlandClient::is_success("ok ok"));
    }

    #[test]
    fn is_success_rejects_partial_failure_in_batch() {
        // Even if the first command succeeded, a later `error: ...` line in
        // the batch response must surface as a failure.
        assert!(!HyprlandClient::is_success("ok\nerror: bad\nok\n"));
    }
}
