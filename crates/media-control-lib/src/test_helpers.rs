//! Test infrastructure for media-control.
//!
//! Provides a mock Hyprland IPC server, command capture, and test context
//! builders for end-to-end command testing without a running Hyprland instance.

use std::collections::HashMap;
use std::future::Future;
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
        CommandContext::for_test(self.client(), config).expect("build test context")
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

    if let Some(key) = prefix_key
        && let Some(responses) = map.get_mut(&key)
    {
        return consume_response(responses);
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
        pid: 0,
        class: class.to_string(),
        title: title.to_string(),
        focus_history_id: 0,
    }
}

/// Create a test `Client` with full control over all fields.
#[allow(clippy::too_many_arguments)]
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
        pid: 0,
        class: class.to_string(),
        title: title.to_string(),
        focus_history_id,
    }
}

/// Chainable builder for synthetic [`Client`] values, replacing the
/// 12-positional-argument [`make_test_client_full`] at most call sites.
///
/// Construct with [`ClientBuilder::new`] (address + class + title — the
/// fields nearly every test actually cares about), then chain setters for
/// the rest. Defaults match `make_test_client` (mapped, visible, workspace
/// 1, monitor 0, focused, geometry `[100, 100, 640, 360]`).
#[must_use]
pub struct ClientBuilder {
    inner: Client,
}

impl ClientBuilder {
    /// Start a builder with the three identity fields.
    pub fn new(address: &str, class: &str, title: &str) -> Self {
        Self {
            inner: make_test_client(address, class, title, false, false),
        }
    }

    /// Mark this client as pinned.
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.inner.pinned = pinned;
        self
    }

    /// Mark this client as floating.
    pub fn floating(mut self, floating: bool) -> Self {
        self.inner.floating = floating;
        self
    }

    /// Set the Hyprland fullscreen state (0/1/2/3).
    pub fn fullscreen(mut self, state: u8) -> Self {
        self.inner.fullscreen = state;
        self
    }

    /// Set the workspace id (workspace name is derived as the same string).
    pub fn workspace(mut self, id: i32) -> Self {
        self.inner.workspace = Workspace {
            id,
            name: id.to_string(),
        };
        self
    }

    /// Set the monitor id. Use `-1` for scratchpad windows.
    pub fn monitor(mut self, id: i32) -> Self {
        self.inner.monitor = id;
        self
    }

    /// Set the focus history id (0 = currently focused, 1 = previous, ...).
    pub fn focus_history(mut self, id: i32) -> Self {
        self.inner.focus_history_id = id;
        self
    }

    /// Set position `[x, y]`.
    pub fn at(mut self, at: [i32; 2]) -> Self {
        self.inner.at = at;
        self
    }

    /// Set size `[w, h]`.
    pub fn size(mut self, size: [i32; 2]) -> Self {
        self.inner.size = size;
        self
    }

    /// Materialize the [`Client`].
    #[must_use]
    pub fn build(self) -> Client {
        self.inner
    }
}

/// Two-client scenario used by the avoider's single-workspace tests:
/// a focused non-media window (firefox) plus a pinned/floating media
/// window (mpv) at `mpv_at`. Returns the JSON the mock server expects.
///
/// Centralized here so adding a new single-workspace test doesn't grow
/// another 30-line scaffolding block in the test module.
pub fn scenario_single_workspace_json(mpv_at: [i32; 2]) -> String {
    let clients = [
        ClientBuilder::new("0xb1", "firefox", "Browser")
            .focus_history(0)
            .at([0, 0])
            .size([1920, 1080])
            .build(),
        ClientBuilder::new("0xd1", "mpv", "video.mp4")
            .pinned(true)
            .floating(true)
            .focus_history(1)
            .at(mpv_at)
            .size([640, 360])
            .build(),
    ];
    make_clients_json(&clients)
}

/// RAII guard that restores `XDG_RUNTIME_DIR` when dropped.
///
/// Held by [`with_isolated_runtime_dir`]. The drop runs on every exit path
/// (panic, early return), so a failing assert can never leak our temp dir
/// into a sibling test's environment.
struct RuntimeDirGuard {
    original: Option<String>,
}

impl Drop for RuntimeDirGuard {
    fn drop(&mut self) {
        // SAFETY: the outer `with_isolated_runtime_dir` holds the
        // `async_env_test_mutex` for the entire scope this guard is
        // alive in, so no other thread is racing us on env mutation.
        unsafe {
            match self.original.take() {
                Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
                None => std::env::remove_var("XDG_RUNTIME_DIR"),
            }
        }
    }
}

/// Run `f` with `XDG_RUNTIME_DIR` pointing at a fresh tempdir and the
/// process-wide env mutex held for the entire body.
///
/// The returned future is polled to completion before the temp dir is
/// dropped and the env var is restored. The env-mutex serializes against
/// every other test that mutates `XDG_RUNTIME_DIR` (or the on-disk suppress
/// file), eliminating the cross-test races that previously made this dance
/// flake.
///
/// The mutex acquire must happen before `set_var` and the restore happens
/// in `RuntimeDirGuard::drop`, so a panic inside `f` is panic-safe — the
/// guard runs on unwind, restores the env, and the mutex releases naturally.
///
/// # Panics
///
/// Panics if creating the tempdir fails, which only happens if the OS is
/// out of file descriptors / disk space — not a test invariant.
pub async fn with_isolated_runtime_dir<F, Fut, R>(f: F) -> R
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = R>,
{
    // Hold the same async mutex `commands::shared::async_env_test_mutex`
    // exposes — the lib's other suppress / runtime-dir tests serialize
    // through that, and this helper joins the same lock domain.
    let _g = crate::commands::shared::async_env_test_mutex().lock().await;

    let original = std::env::var("XDG_RUNTIME_DIR").ok();
    let runtime = tempfile::tempdir().expect("create temp runtime dir");
    let path = runtime.path().to_path_buf();

    // SAFETY: single-threaded test under the async env mutex held above.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", &path);
    }

    // RAII restore: runs on every exit path including panic in `f`.
    let _restore = RuntimeDirGuard { original };

    f(path).await
}

/// Build a `Config` with `suppress_ms = 0` so suppression never blocks tests.
///
/// The shared on-disk suppress file races across parallel tests touching the
/// avoider; setting `suppress_ms = 0` forces every `should_suppress()` check
/// to return false, eliminating that flake source. Originally duplicated in
/// `commands::{avoid,fullscreen}::tests::test_config`; centralised here so a
/// new test module can opt in with one import.
#[must_use]
pub fn test_config_no_suppress() -> Config {
    let mut c = Config::default();
    c.positioning.suppress_ms = 0;
    c
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

    /// `dispatch_batch` must prepend `dispatch ` to each bare action and
    /// join with `; ` inside the `[[BATCH]]` envelope. Locks the wire format
    /// so a future refactor cannot silently break Hyprland's parser.
    #[tokio::test]
    async fn mock_server_dispatch_batch_prefixes_each_action() {
        let mock = MockHyprland::start().await;
        let client = mock.client();
        client
            .dispatch_batch(&[
                "movewindowpixel exact 100 200,address:0x1",
                "resizewindowpixel exact 640 360,address:0x1",
            ])
            .await
            .unwrap();

        let cmds = mock.captured_commands().await;
        assert_eq!(cmds.len(), 1);
        let got = &cmds[0];
        assert!(got.starts_with("[[BATCH]]"), "missing batch prefix: {got}");
        assert_eq!(
            got,
            "[[BATCH]]dispatch movewindowpixel exact 100 200,address:0x1; dispatch resizewindowpixel exact 640 360,address:0x1",
            "wire format drift in dispatch_batch"
        );
    }

    /// Empty `dispatch_batch` must be a no-op (no socket call) so callers can
    /// build dynamic batches without an explicit emptiness guard.
    #[tokio::test]
    async fn mock_server_dispatch_batch_empty_is_noop() {
        let mock = MockHyprland::start().await;
        let client = mock.client();
        client.dispatch_batch(&[]).await.unwrap();
        assert!(mock.captured_commands().await.is_empty());
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
            vec![
                "first".to_string(),
                "second".to_string(),
                "last".to_string(),
            ],
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
