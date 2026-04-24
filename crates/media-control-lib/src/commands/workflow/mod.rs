//! Workflow commands and their mpv-IPC plumbing.
//!
//! Hosts every command that talks to mpv-shim or Jellyfin via the mpv IPC
//! socket — chapter, seek, play, random, status, keep, mark_watched —
//! plus the socket-discovery, validation, exchange, and retry helpers
//! they share.

pub mod chapter;
pub mod keep;
pub mod mark_watched;
pub mod play;
pub mod random;
pub mod seek;
pub mod status;

use std::env;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::error::Result;

/// Default mpv IPC socket path (mpv-shim).
pub(crate) const MPV_IPC_SOCKET_DEFAULT: &str = "/tmp/mpv-shim";

/// Fallback mpv IPC socket path (legacy).
const MPV_IPC_SOCKET_FALLBACK: &str = "/tmp/mpvctl-jshim";

/// Timeout for connecting to and writing to a socket (local Unix socket — fast).
const SOCKET_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// Timeout for reading a response from mpv.
const SOCKET_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// Delay between retry attempts.
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

/// Check that a path exists and is a Unix socket.
///
/// Uses `symlink_metadata` (lstat) — does NOT follow symlinks. This defends
/// against an attacker placing a symlink at a predictable `/tmp` path that
/// points to a file or socket they control.
fn is_unix_socket(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false)
}

/// Connect to a Unix socket and write `payload\n`, returning the open stream.
///
/// Bounded by `SOCKET_CONNECT_TIMEOUT`. Caller is responsible for
/// validating that `path` is a real Unix socket via [`is_unix_socket`]
/// before calling — defends against symlink-to-regular-file in /tmp.
async fn connect_and_write(
    path: &Path,
    payload: &str,
    append_newline: bool,
) -> std::io::Result<UnixStream> {
    timeout(SOCKET_CONNECT_TIMEOUT, async {
        let mut stream = UnixStream::connect(path).await?;
        stream.write_all(payload.as_bytes()).await?;
        if append_newline {
            stream.write_all(b"\n").await?;
        }
        Ok::<_, std::io::Error>(stream)
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "socket connect timeout"))?
}

/// Validate an mpv IPC socket path candidate sourced from env input.
///
/// Mirrors the constraints applied to `XDG_RUNTIME_DIR` in [`runtime_dir`]:
/// the path must be absolute and contain no `..` components. Existence is
/// NOT checked here (the socket may legitimately not yet exist when the
/// caller probes the candidate list); the lstat-based [`is_unix_socket`]
/// downstream catches that case.
///
/// # Errors
///
/// Returns [`MediaControlError::InvalidArgument`] when the supplied
/// `MPV_IPC_SOCKET` is empty, relative, or contains `..` components.
fn validate_mpv_socket_path(raw: &str) -> Result<PathBuf> {
    if raw.is_empty() {
        return Err(crate::error::MediaControlError::invalid_argument(
            "MPV_IPC_SOCKET must not be empty",
        ));
    }
    let p = PathBuf::from(raw);
    if !p.is_absolute() {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "MPV_IPC_SOCKET must be an absolute path (got {raw:?})"
        )));
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "MPV_IPC_SOCKET must not contain `..` components (got {raw:?})"
        )));
    }
    Ok(p)
}

/// Get the ordered list of mpv IPC socket paths to try.
///
/// `MPV_IPC_SOCKET` (when set) is validated through
/// [`validate_mpv_socket_path`] — the same constraints `runtime_dir`
/// applies to `XDG_RUNTIME_DIR`. An invalid value is logged once and
/// dropped from the candidate list rather than poisoning the whole call;
/// the hardcoded fallbacks still get a chance.
fn mpv_socket_paths() -> Vec<String> {
    let mut paths = Vec::with_capacity(3);
    if let Ok(env_path) = env::var("MPV_IPC_SOCKET") {
        match validate_mpv_socket_path(&env_path) {
            Ok(p) => {
                // `from(PathBuf).to_string_lossy().into_owned()` would
                // smuggle non-UTF8 bytes through with `?` placeholders; the
                // validator already rejected unusable shapes, so a direct
                // String round-trip is safe here.
                if let Some(s) = p.to_str() {
                    paths.push(s.to_string());
                } else {
                    tracing::warn!(
                        "MPV_IPC_SOCKET contains non-UTF8 bytes; ignoring and using fallbacks"
                    );
                }
            }
            Err(e) => {
                tracing::warn!("ignoring invalid MPV_IPC_SOCKET: {e}");
            }
        }
    }
    paths.push(MPV_IPC_SOCKET_DEFAULT.to_string());
    paths.push(MPV_IPC_SOCKET_FALLBACK.to_string());
    paths
}

/// Low-level: connect to a single validated socket, send payload, optionally read response.
///
/// Returns the parsed JSON response if `read_response` is true, or `None` if fire-and-forget.
/// Skips non-socket paths. Uses SOCKET_CONNECT_TIMEOUT for connect+write,
/// SOCKET_RESPONSE_TIMEOUT for reading.
async fn mpv_ipc_exchange(
    socket_path: &str,
    payload: &str,
    read_response: bool,
) -> std::result::Result<Option<serde_json::Value>, ()> {
    let path = Path::new(socket_path);

    if !is_unix_socket(path) {
        // Use lstat-based exists so a dangling symlink is reported, but a
        // missing path stays quiet (typical case during startup).
        if std::fs::symlink_metadata(path).is_ok() {
            tracing::warn!("skipping {socket_path}: not a socket");
        }
        return Err(());
    }

    // Connect + write with timeout
    let mut stream = match connect_and_write(path, payload, true).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            tracing::warn!("timeout connecting to {socket_path}");
            return Err(());
        }
        Err(e) => {
            tracing::debug!("connect/write to {socket_path} failed: {e}");
            return Err(());
        }
    };

    if !read_response {
        return Ok(None);
    }

    // Read response with timeout, skipping mpv event lines.
    // Reuse a single buffer to avoid per-line heap allocation when mpv
    // floods events between our request and its response.
    let mut reader = BufReader::new(&mut stream);
    let read_result = timeout(SOCKET_RESPONSE_TIMEOUT, async {
        let mut buf = String::with_capacity(256);
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await?;
            if n == 0 {
                // EOF — mpv closed the connection
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "mpv closed connection",
                ));
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&buf) {
                // Skip unsolicited event messages
                if val.get("event").is_some() && val.get("error").is_none() {
                    continue;
                }
                return Ok::<_, std::io::Error>(val);
            }
            // Unparseable line — skip
        }
    })
    .await;

    // Distinguish the three failure modes in the log so a wedged-mpv
    // condition (timeout) is observable from the same log stream as a
    // closed-socket condition (EOF) or a malformed line that bubbled all
    // the way out (parse). The public return shape is preserved
    // (`Ok(None)`) so callers don't have to learn three new error
    // variants — the diagnostic lives in the trace, not the type.
    match read_result {
        Ok(Ok(val)) => Ok(Some(val)),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            tracing::debug!("mpv {socket_path}: EOF before response (mpv closed the connection)");
            Ok(None)
        }
        Ok(Err(e)) => {
            tracing::debug!("mpv {socket_path}: read error: {e}");
            Ok(None)
        }
        Err(_) => {
            tracing::debug!(
                "mpv {socket_path}: response read timed out after {}ms (socket alive but mpv hung)",
                SOCKET_RESPONSE_TIMEOUT.as_millis()
            );
            Ok(None)
        }
    }
}

/// Send a script-message to mpv via IPC socket (fire-and-forget).
///
/// Writes the command and closes — does NOT read the response. During rapid
/// fire, mpv floods the socket with event lines (file-loaded, property-change,
/// etc.) and reading through them to find the response adds 10-50ms+ per call.
/// Script-messages are delivered asynchronously by mpv regardless of response.
pub async fn send_mpv_script_message(message: &str) -> Result<()> {
    send_mpv_script_message_with_args(message, &[]).await
}

/// Send a multi-argument script-message to mpv (fire-and-forget).
pub async fn send_mpv_script_message_with_args(message: &str, args: &[&str]) -> Result<()> {
    let mut parts: Vec<&str> = vec!["script-message", message];
    parts.extend_from_slice(args);
    let payload = serde_json::json!({"command": parts}).to_string();
    send_mpv_ipc_command(&payload).await
}

/// Validate that a CLI-supplied IPC token is non-empty and does not exceed
/// `max_len` bytes.
///
/// Returns [`crate::error::MediaControlError::InvalidArgument`] (not
/// `MpvIpc/ConnectionFailed`) when the cap is exceeded or the value is
/// empty — the connection was never attempted, the input was rejected.
/// Centralised so every IPC-bound CLI argument enforces the cap identically
/// and emits the same error shape for callers/tests.
///
/// Empty values are rejected because they would interpolate into a
/// malformed IPC payload (e.g. `script-message play-` with a trailing `-`,
/// or `script-message random ` with a trailing empty arg). Treating
/// "missing" as a distinct case is the caller's job — pass an `Option` and
/// branch before calling this.
#[inline]
pub(crate) fn validate_ipc_token_len(label: &str, value: &str, max_len: usize) -> Result<()> {
    if value.is_empty() {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "{label} must not be empty"
        )));
    }
    if value.len() > max_len {
        return Err(crate::error::MediaControlError::invalid_argument(format!(
            "{label} too long: {} bytes (max {max_len})",
            value.len()
        )));
    }
    Ok(())
}

/// Try each candidate mpv socket path in turn, with optional retry passes.
///
/// Returns the first `Some(T)` produced by any (path, attempt) pair; if
/// all combinations fail, emits a single `tracing::debug!` listing every
/// attempted path so the caller's "no socket" error has actionable context
/// in the logs.
///
/// # Iteration order
///
/// `for path { for attempt { … } }` — exhaust retries on each path before
/// moving to the next. The other order (`for attempt { for path { … } }`)
/// burns every retry on a wedged path 1 before path 2 is ever tried,
/// turning a stale-but-present socket into a long latency spike for a
/// caller that only needs the live fallback. With this ordering, a stale
/// path 1 still gives the live path 2 the very first attempt on its first
/// pass — at the cost of `RETRY_DELAY` between attempts on the same
/// path, which is the desired latency budget for a single bad endpoint.
///
/// # Timing
///
/// Per-path timeouts are applied inside `op`, not here, so total wall time
/// scales as `paths.len() * (retries + 1) * <op timeout> + paths.len() *
/// retries * RETRY_DELAY`. With the default 50ms connect + 50ms response
/// budget, 3 paths, and `retries=1`, the worst case is
/// `3 * 2 * 100ms + 3 * 1 * 25ms = 675ms`.
async fn try_mpv_paths<T, F, Fut>(retries: u8, mut op: F) -> Option<T>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let paths = mpv_socket_paths();
    for path in &paths {
        for attempt in 0..=retries {
            if attempt > 0 {
                tokio::time::sleep(RETRY_DELAY).await;
            }
            // Clone the owned String to sidestep the
            // closure-borrow-vs-future-lifetime tangle: `op` returns a future
            // that may outlive the `&path` borrow, so we hand it an owned copy.
            if let Some(v) = op(path.clone()).await {
                return Some(v);
            }
        }
    }
    tracing::debug!(
        "mpv IPC failed across {} path(s) after {} attempt(s): {:?}",
        paths.len(),
        u32::from(retries) + 1,
        paths
    );
    None
}

/// Send a raw JSON command to mpv via IPC socket (fire-and-forget).
///
/// Tries multiple socket paths with retry. Does not read the response —
/// avoids blocking on mpv's event flood during rapid-fire commands.
pub async fn send_mpv_ipc_command(payload: &str) -> Result<()> {
    let ok = try_mpv_paths(1, |path| async move {
        mpv_ipc_exchange(&path, payload, false)
            .await
            .ok()
            .map(|_| ())
    })
    .await;

    ok.ok_or_else(crate::error::MediaControlError::mpv_no_socket)
}

/// Query an mpv property and return its value.
///
/// Sends a `get_property` command to mpv and returns the `data` field from
/// the response. Single attempt across all candidate paths (no retry pass)
/// — designed for fast status queries.
///
/// # Timing
///
/// Per-path timeout is `SOCKET_CONNECT_TIMEOUT + SOCKET_RESPONSE_TIMEOUT`
/// (currently 100 ms). With up to 3 candidate sockets the worst-case wall
/// time is ~300 ms; in the common single-socket case it's bounded by the
/// per-path timeout.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use media_control_lib::commands::query_mpv_property;
/// let title = query_mpv_property("media-title").await?;
/// # Ok(())
/// # }
/// ```
pub async fn query_mpv_property(property: &str) -> Result<serde_json::Value> {
    let payload = serde_json::json!({"command": ["get_property", property]}).to_string();

    // Single attempt across paths (no retry) — caller wants a fast lookup.
    let result = try_mpv_paths(0, |path| {
        let payload = &payload;
        async move {
            match mpv_ipc_exchange(&path, payload, true).await {
                Ok(Some(resp)) => Some(resp),
                _ => None,
            }
        }
    })
    .await;

    let resp = result.ok_or_else(crate::error::MediaControlError::mpv_no_socket)?;

    let err_str = resp.get("error").and_then(|e| e.as_str());
    if err_str == Some("success") {
        Ok(resp.get("data").cloned().unwrap_or(serde_json::Value::Null))
    } else {
        Err(crate::error::MediaControlError::mpv_connection_failed(
            format!(
                "mpv error for {property:?}: {}",
                err_str.unwrap_or("unknown")
            ),
        ))
    }
}

/// Send a payload to a *specific* mpv socket (fire-and-forget, no response read).
///
/// Bypasses [`mpv_socket_paths`]'s discovery list — the caller already knows
/// which socket they want (e.g. the shim socket when closing a shim window).
/// Returns `true` on successful write, `false` on any failure.
pub(crate) async fn send_to_mpv_socket(socket_path: &str, payload: &str) -> bool {
    mpv_ipc_exchange(socket_path, payload, false).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::shared::async_env_test_mutex;
    use super::*;
    use std::env;

    /// Helper to safely set an environment variable in tests.
    ///
    /// # Safety
    ///
    /// This is only safe in single-threaded test contexts. Tests modifying
    /// environment variables should use `#[serial_test::serial]` or similar.
    unsafe fn set_env(key: &str, value: &str) {
        // SAFETY: Caller guarantees single-threaded context
        unsafe { env::set_var(key, value) };
    }

    /// Helper to safely remove an environment variable in tests.
    ///
    /// # Safety
    ///
    /// This is only safe in single-threaded test contexts.
    unsafe fn remove_env(key: &str) {
        // SAFETY: Caller guarantees single-threaded context
        unsafe { env::remove_var(key) };
    }

    #[test]
    fn socket_validation_skips_regular_file() {
        use std::os::unix::fs::FileTypeExt;

        // Create a regular file — should NOT be identified as a socket
        let dir = tempfile::tempdir().unwrap();
        let fake_socket = dir.path().join("fake-socket");
        std::fs::write(&fake_socket, "not a socket").unwrap();

        let meta = std::fs::metadata(&fake_socket).unwrap();
        assert!(
            !meta.file_type().is_socket(),
            "regular file should not be identified as socket"
        );
    }

    #[test]
    fn socket_validation_detects_real_socket() {
        use std::os::unix::fs::FileTypeExt;

        // Create a real Unix socket
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test-socket");
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let meta = std::fs::metadata(&socket_path).unwrap();
        assert!(
            meta.file_type().is_socket(),
            "Unix socket should be identified as socket"
        );
    }

    #[test]
    fn socket_validation_handles_nonexistent() {
        let result = std::fs::metadata("/tmp/definitely-nonexistent-socket-path-12345");
        assert!(result.is_err(), "nonexistent path should fail metadata");
    }

    #[tokio::test]
    async fn send_mpv_ipc_command_succeeds_with_real_socket() {
        // Create a real Unix socket listener and verify the command gets through
        use tokio::io::AsyncBufReadExt;
        use tokio::net::UnixListener;

        // Hold the async env-mutex across all `.await`s in this test so a
        // parallel test cannot rewrite MPV_IPC_SOCKET between our set_env
        // call and the internal `mpv_socket_paths()` env::var read inside
        // `send_mpv_script_message`. The guard is `Send` so this is safe.
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("test-mpv-socket");
        let listener = UnixListener::bind(&socket_path).unwrap();

        // SAFETY: Test is single-threaded
        unsafe {
            set_env("MPV_IPC_SOCKET", socket_path.to_str().unwrap());
        }

        // Spawn a task to accept the connection and verify the command arrived
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut reader = tokio::io::BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            // Verify we received the command
            assert!(line.contains("script-message"));
            assert!(line.contains("test-cmd"));
            // No response needed — fire-and-forget
        });

        let result = send_mpv_script_message("test-cmd").await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");

        handle.await.unwrap();

        // SAFETY: Restore
        unsafe {
            if let Some(val) = original {
                set_env("MPV_IPC_SOCKET", &val);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }
    }

    /// Security: socket validation must NOT follow symlinks. A symlink
    /// pointing to a real socket should be rejected, since an attacker who
    /// controls /tmp could plant a symlink targeting a socket they own.
    #[test]
    fn socket_validation_rejects_symlink_to_socket() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let real_sock = dir.path().join("real.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&real_sock).unwrap();

        let symlink_path = dir.path().join("via-symlink.sock");
        symlink(&real_sock, &symlink_path).unwrap();

        // is_unix_socket uses lstat; a symlink (even pointing at a real
        // socket) must NOT pass.
        assert!(
            !is_unix_socket(&symlink_path),
            "symlink to socket must be rejected by lstat-based check"
        );
        // The real socket (no symlink in the path) should pass.
        assert!(is_unix_socket(&real_sock));
    }

    /// Helper duplicates clean up after themselves; verify symlink to a
    /// regular file is also rejected (would otherwise be silently followed
    /// by std::fs::metadata).
    #[test]
    fn socket_validation_rejects_symlink_to_regular_file() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let regular = dir.path().join("regular.txt");
        std::fs::write(&regular, "data").unwrap();
        let link = dir.path().join("link.sock");
        symlink(&regular, &link).unwrap();

        assert!(!is_unix_socket(&link));
    }

    /// Verify try_mpv_paths returns None when no socket responds — sanity
    /// check on the shared retry helper.
    ///
    /// Holds the async env-mutex across the await so a parallel
    /// `MPV_IPC_SOCKET`-mutating test (e.g.
    /// `send_mpv_ipc_command_succeeds_with_real_socket`) cannot rewrite the
    /// var mid-flight and confuse our assertion.
    #[tokio::test]
    async fn try_mpv_paths_returns_none_when_no_socket_works() {
        let _g = async_env_test_mutex().lock().await;
        let original = env::var("MPV_IPC_SOCKET").ok();
        // SAFETY: single-threaded test
        unsafe {
            set_env(
                "MPV_IPC_SOCKET",
                "/tmp/definitely-nonexistent-mpv-socket-xyz",
            );
        }
        let result: Option<()> = try_mpv_paths(0, |_path| async { None }).await;
        assert!(result.is_none());

        // SAFETY: restore
        unsafe {
            if let Some(v) = original {
                set_env("MPV_IPC_SOCKET", &v);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }
    }

    /// Retry-exhaustion path: when every candidate mpv socket fails,
    /// `send_mpv_ipc_command` must surface the typed
    /// [`crate::error::MediaControlError::MpvIpc`] variant (NoSocket
    /// kind) — not a generic IO error or success. Guards the failure
    /// contract callers like `chapter` and `seek` rely on.
    ///
    /// Skips when the host has a live socket at one of the hardcoded
    /// fallback paths (`/tmp/mpv-shim` or `/tmp/mpvctl-jshim`) — the test
    /// can't simulate a no-socket world if a real mpv-shim is running on
    /// the dev box. Asserting against a pre-existing socket would either
    /// succeed spuriously or send a no-op `script-message` to the user's
    /// real mpv instance.
    #[tokio::test]
    async fn send_mpv_ipc_command_returns_no_socket_when_all_paths_fail() {
        use crate::error::{MediaControlError, MpvIpcErrorKind};

        let _g = async_env_test_mutex().lock().await;

        // Bail if any of the hardcoded fallback sockets are live on this host.
        if is_unix_socket(Path::new(MPV_IPC_SOCKET_DEFAULT))
            || is_unix_socket(Path::new(MPV_IPC_SOCKET_FALLBACK))
        {
            eprintln!(
                "skipping: host has a live mpv socket at one of {MPV_IPC_SOCKET_DEFAULT:?}/{MPV_IPC_SOCKET_FALLBACK:?}"
            );
            return;
        }

        let original = env::var("MPV_IPC_SOCKET").ok();

        // Point env at a path that does not exist. With the host-socket
        // skip above, all three candidates should fail and we should see
        // NoSocket.
        unsafe {
            set_env(
                "MPV_IPC_SOCKET",
                "/tmp/mpc-audit-nonexistent-socket-91827465",
            );
        }

        let result = send_mpv_ipc_command(r#"{"command":["no-op"]}"#).await;

        // SAFETY: restore env before any assert that might panic.
        unsafe {
            if let Some(v) = original {
                set_env("MPV_IPC_SOCKET", &v);
            } else {
                remove_env("MPV_IPC_SOCKET");
            }
        }

        match result {
            Err(MediaControlError::MpvIpc { kind, .. }) => {
                assert_eq!(
                    kind,
                    MpvIpcErrorKind::NoSocket,
                    "expected NoSocket; got {kind:?}"
                );
            }
            Ok(()) => panic!("send_mpv_ipc_command should fail when no socket exists"),
            Err(e) => panic!("expected MpvIpc/NoSocket; got {e:?}"),
        }
    }
}
