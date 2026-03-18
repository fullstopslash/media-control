//! Test infrastructure for media-control.
//!
//! Provides a mock Hyprland IPC server, command capture, and test context
//! builders for end-to-end command testing without a running Hyprland instance.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::commands::CommandContext;
use crate::config::Config;
use crate::hyprland::{Client, HyprlandClient, Monitor, Workspace};

/// Response storage: maps command keys to a sequence of responses.
/// Each call consumes the first response; once only one remains, it repeats.
type ResponseMap = HashMap<String, Vec<String>>;

/// A mock Hyprland IPC server for testing.
///
/// Listens on a temporary Unix socket and responds to commands with
/// configurable canned responses. Records all received commands for
/// later assertion.
///
/// Supports response sequences: `set_response` sets a single (repeating) response,
/// `set_response_sequence` sets multiple responses consumed in order.
pub struct MockHyprland {
    socket_path: PathBuf,
    commands: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<ResponseMap>>,
    _handle: JoinHandle<()>,
}

impl MockHyprland {
    /// Start a new mock server on a temporary Unix socket.
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");
        let socket_path = dir.path().join("mock-hyprland.sock");

        let listener = UnixListener::bind(&socket_path).expect("bind mock socket");

        let commands: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let responses: Arc<Mutex<ResponseMap>> = Arc::new(Mutex::new(HashMap::new()));

        let cmd_clone = Arc::clone(&commands);
        let resp_clone = Arc::clone(&responses);

        let handle = tokio::spawn(async move {
            // Keep temp dir alive for the lifetime of the server task
            let _dir = dir;

            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };

                let cmd_ref = Arc::clone(&cmd_clone);
                let resp_ref = Arc::clone(&resp_clone);

                tokio::spawn(async move {
                    // Read full command (client shuts down write half when done)
                    let mut buf = Vec::new();
                    if stream.read_to_end(&mut buf).await.is_err() {
                        return;
                    }

                    let command = String::from_utf8_lossy(&buf).to_string();

                    // Record the command
                    cmd_ref.lock().await.push(command.clone());

                    // Look up response (may consume from sequence)
                    let response = {
                        let mut map = resp_ref.lock().await;
                        find_response(&mut map, &command)
                    };

                    // Write response
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        Self {
            socket_path,
            commands,
            responses,
            _handle: handle,
        }
    }

    /// Create a `HyprlandClient` connected to this mock server.
    pub fn client(&self) -> HyprlandClient {
        HyprlandClient::with_socket_path(self.socket_path.clone())
    }

    /// Create a `CommandContext` using this mock server with the given config.
    pub fn context(&self, config: Config) -> CommandContext {
        CommandContext::for_test(self.client(), config)
            .expect("build test context")
    }

    /// Create a `CommandContext` using this mock server with default config.
    pub fn default_context(&self) -> CommandContext {
        self.context(Config::default())
    }

    /// Set a single (repeating) response for a command key.
    pub async fn set_response(&self, command: &str, response: &str) {
        self.responses
            .lock()
            .await
            .insert(command.to_string(), vec![response.to_string()]);
    }

    /// Set a sequence of responses for a command key.
    ///
    /// Each call to this command consumes the next response in order.
    /// Once only one remains, it repeats indefinitely.
    pub async fn set_response_sequence(&self, command: &str, responses: Vec<String>) {
        assert!(!responses.is_empty(), "response sequence must not be empty");
        self.responses
            .lock()
            .await
            .insert(command.to_string(), responses);
    }

    /// Get all captured commands in order.
    pub async fn captured_commands(&self) -> Vec<String> {
        self.commands.lock().await.clone()
    }

    /// Clear captured commands.
    pub async fn clear_commands(&self) {
        self.commands.lock().await.clear();
    }

    /// Get the socket path (for direct use if needed).
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

/// Find the best matching response for a command, consuming from sequences.
///
/// Priority: exact match > prefix match > default "ok".
/// If a sequence has more than one response, the first is consumed.
/// The last response in a sequence always repeats.
fn find_response(map: &mut ResponseMap, command: &str) -> String {
    // 1. Try exact match
    if let Some(responses) = map.get_mut(command) {
        return consume_response(responses);
    }

    // 2. Try prefix match
    // Need to find the key first, then access mutably (borrow checker)
    let prefix_key = map
        .keys()
        .find(|key| command.starts_with(key.as_str()))
        .cloned();

    if let Some(key) = prefix_key {
        if let Some(responses) = map.get_mut(&key) {
            return consume_response(responses);
        }
    }

    // 3. Default
    "ok".to_string()
}

/// Consume the next response from a sequence.
/// If more than one remains, removes and returns the first.
/// If exactly one remains, returns it without removing (repeats forever).
fn consume_response(responses: &mut Vec<String>) -> String {
    if responses.len() > 1 {
        responses.remove(0)
    } else {
        responses[0].clone()
    }
}

/// Serialize a slice of `Client` structs to JSON for mock responses.
pub fn make_clients_json(clients: &[Client]) -> String {
    serde_json::to_string(clients).expect("serialize clients")
}

/// Serialize a single `Client` to JSON for mock `j/activewindow` responses.
pub fn make_active_window_json(client: &Client) -> String {
    serde_json::to_string(client).expect("serialize client")
}

/// Serialize a slice of `Monitor` structs to JSON for mock responses.
pub fn make_monitors_json(monitors: &[Monitor]) -> String {
    serde_json::to_string(monitors).expect("serialize monitors")
}

/// Create a test `Client` with common defaults.
pub fn make_test_client(
    address: &str,
    class: &str,
    title: &str,
    pinned: bool,
    floating: bool,
) -> Client {
    Client {
        address: address.to_string(),
        mapped: true,
        hidden: false,
        at: [100, 100],
        size: [640, 360],
        workspace: Workspace {
            id: 1,
            name: "1".to_string(),
        },
        floating,
        pinned,
        fullscreen: 0,
        monitor: 0,
        class: class.to_string(),
        title: title.to_string(),
        focus_history_id: 0,
    }
}

/// Create a test `Client` with full control over all fields.
pub fn make_test_client_full(
    address: &str,
    class: &str,
    title: &str,
    pinned: bool,
    floating: bool,
    fullscreen: u8,
    workspace_id: i32,
    monitor: i32,
    focus_history_id: i32,
    at: [i32; 2],
    size: [i32; 2],
) -> Client {
    Client {
        address: address.to_string(),
        mapped: true,
        hidden: false,
        at,
        size,
        workspace: Workspace {
            id: workspace_id,
            name: workspace_id.to_string(),
        },
        floating,
        pinned,
        fullscreen,
        monitor,
        class: class.to_string(),
        title: title.to_string(),
        focus_history_id,
    }
}

/// Create a test `Monitor` with common defaults.
pub fn make_test_monitor(id: i32, focused: bool) -> Monitor {
    Monitor {
        id,
        name: format!("DP-{id}"),
        width: 1920,
        height: 1080,
        x: id * 1920,
        y: 0,
        focused,
        active_workspace: Workspace {
            id: 1,
            name: "1".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_server_responds_to_exact_command() {
        let mock = MockHyprland::start().await;
        mock.set_response("j/clients", "[]").await;

        let client = mock.client();
        let clients = client.get_clients().await.unwrap();
        assert!(clients.is_empty());
    }

    #[tokio::test]
    async fn mock_server_captures_commands() {
        let mock = MockHyprland::start().await;

        let client = mock.client();
        let _ = client.command("j/clients").await;
        let _ = client.command("j/monitors").await;

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], "j/clients");
        assert_eq!(cmds[1], "j/monitors");
    }

    #[tokio::test]
    async fn mock_server_default_response_is_ok() {
        let mock = MockHyprland::start().await;

        let client = mock.client();
        let resp = client
            .command("dispatch focuswindow address:0x1")
            .await
            .unwrap();
        assert_eq!(resp, "ok");
    }

    #[tokio::test]
    async fn mock_server_prefix_matching() {
        let mock = MockHyprland::start().await;
        mock.set_response("dispatch", "ok").await;
        mock.set_response("j/clients", r#"[{"address":"0x1","mapped":true,"hidden":false,"at":[0,0],"size":[100,100],"workspace":{"id":1,"name":"1"},"floating":false,"pinned":false,"fullscreen":0,"monitor":0,"class":"test","title":"test","focusHistoryID":0}]"#).await;

        let client = mock.client();

        // Prefix match: "dispatch focuswindow..." matches "dispatch"
        client.dispatch("focuswindow address:0x1").await.unwrap();

        // Exact match: "j/clients" matches exactly
        let clients = client.get_clients().await.unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].address, "0x1");
    }

    #[tokio::test]
    async fn mock_server_batch_commands() {
        let mock = MockHyprland::start().await;

        let client = mock.client();
        client
            .batch(&[
                "dispatch movewindowpixel exact 100 200,address:0x1",
                "dispatch resizewindowpixel exact 640 360,address:0x1",
            ])
            .await
            .unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].starts_with("[[BATCH]]"));
    }

    #[tokio::test]
    async fn mock_server_clear_commands() {
        let mock = MockHyprland::start().await;

        let client = mock.client();
        let _ = client.command("j/clients").await;
        assert_eq!(mock.captured_commands().await.len(), 1);

        mock.clear_commands().await;
        assert!(mock.captured_commands().await.is_empty());
    }

    #[tokio::test]
    async fn mock_server_response_sequence() {
        let mock = MockHyprland::start().await;
        mock.set_response_sequence(
            "j/clients",
            vec!["first".to_string(), "second".to_string(), "last".to_string()],
        )
        .await;

        let client = mock.client();

        let r1 = client.command("j/clients").await.unwrap();
        assert_eq!(r1, "first");

        let r2 = client.command("j/clients").await.unwrap();
        assert_eq!(r2, "second");

        // "last" repeats
        let r3 = client.command("j/clients").await.unwrap();
        assert_eq!(r3, "last");

        let r4 = client.command("j/clients").await.unwrap();
        assert_eq!(r4, "last");
    }

    #[tokio::test]
    async fn context_for_test_works() {
        let mock = MockHyprland::start().await;
        mock.set_response("j/clients", "[]").await;

        let ctx = mock.default_context();
        let clients = ctx.hyprland.get_clients().await.unwrap();
        assert!(clients.is_empty());
    }

    #[tokio::test]
    async fn context_with_custom_config() {
        let mock = MockHyprland::start().await;

        let mut config = Config::default();
        config.positions.x_right = 9999;

        let ctx = mock.context(config);
        assert_eq!(ctx.config.positions.x_right, 9999);
    }

    #[test]
    fn make_clients_json_roundtrips() {
        let clients = vec![make_test_client("0x1", "mpv", "video.mp4", true, true)];
        let json = make_clients_json(&clients);
        let parsed: Vec<Client> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].address, "0x1");
        assert_eq!(parsed[0].class, "mpv");
    }

    #[test]
    fn make_monitors_json_roundtrips() {
        let monitors = vec![make_test_monitor(0, true)];
        let json = make_monitors_json(&monitors);
        let parsed: Vec<Monitor> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, 0);
        assert!(parsed[0].focused);
    }

    #[test]
    fn consume_response_single_repeats() {
        let mut responses = vec!["only".to_string()];
        assert_eq!(consume_response(&mut responses), "only");
        assert_eq!(consume_response(&mut responses), "only");
        assert_eq!(responses.len(), 1);
    }

    #[test]
    fn consume_response_sequence_drains() {
        let mut responses = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(consume_response(&mut responses), "a");
        assert_eq!(responses.len(), 2);
        assert_eq!(consume_response(&mut responses), "b");
        assert_eq!(responses.len(), 1);
        // Last one repeats
        assert_eq!(consume_response(&mut responses), "c");
        assert_eq!(consume_response(&mut responses), "c");
        assert_eq!(responses.len(), 1);
    }
}
