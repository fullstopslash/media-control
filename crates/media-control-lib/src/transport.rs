//! Trigger-socket transport for the media-control daemon.
//!
//! Single source of truth for the daemon's `SOCK_DGRAM` trigger socket
//! path and the connectionless `kick` send used by the CLI and any
//! external script that needs to wake the avoider.
//!
//! # Wire format (FR-9, intent 018)
//!
//! - **0-byte datagram**: canonical "re-evaluate placement" kick. MUST
//!   always remain valid.
//! - **Length ≥ 1**: byte 0 is the protocol version. `0x01` is reserved
//!   for a future v1 envelope (likely UTF-8 JSON). All other version
//!   bytes are reserved. The daemon ignores all non-empty datagrams in
//!   this release and emits a single `debug!` log line per receipt.
//!
//! The CLI MUST NOT expose any flag that would generate a non-empty
//! datagram in this release; `--reason` and friends are reserved for the
//! v1 envelope and are not parsed today.

use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};

use crate::commands::shared::runtime_dir;
use crate::error::{MediaControlError, Result};

/// Filename of the daemon's trigger socket inside `$XDG_RUNTIME_DIR`.
///
/// Centralized as one constant so a future rename touches one place. Both
/// the daemon's `bind` and the CLI's [`kick`] resolve through
/// [`socket_path`] and therefore through this filename.
pub const SOCKET_FILENAME: &str = "media-control-daemon.sock";

/// Resolve the absolute path to the daemon's trigger socket.
///
/// Single source of truth used by both the daemon (for `bind`) and the
/// CLI's [`kick`] (for `send_to`). Propagates `runtime_dir` errors so
/// callers can distinguish "missing/invalid `$XDG_RUNTIME_DIR`" from
/// transport-layer failures.
pub fn socket_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join(SOCKET_FILENAME))
}

/// Outcome of a [`kick`] attempt.
///
/// `DaemonDown` is treated as success at the keybind layer (per FR-4 /
/// FR-5 in intent 018): a missing daemon must not produce error output
/// for a keybind shell. The variant is preserved so callers that want to
/// surface the distinction (scripts, tests) can do so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KickOutcome {
    /// Datagram delivered to the daemon's bound socket (or queued in the
    /// kernel buffer for it).
    Delivered,
    /// Daemon is not running. Either `ECONNREFUSED` from a stale socket
    /// or `ENOENT` because the socket file does not exist. Silent at the
    /// keybind layer.
    DaemonDown,
}

/// Send a 0-byte canonical kick datagram to the daemon's trigger socket.
///
/// Connectionless `send_to` on a `SOCK_DGRAM` socket — never blocks
/// waiting for a reader. The kernel either delivers, drops, or returns
/// `ECONNREFUSED`/`ENOENT` synchronously. This is the FR-5 mechanism:
/// `media-control kick` must exit ≤ 100ms regardless of daemon state.
///
/// Sync (not async) by deliberate choice: the CLI's `kick` path doesn't
/// otherwise need a tokio runtime, and runtime startup would burn the
/// FR-5 latency budget for a single 3-syscall operation.
///
/// # Errors
///
/// - [`MediaControlError::Io`] for any error other than the
///   `DaemonDown` cases (permission denied, etc.). Caller should report
///   to stderr and exit non-zero.
/// - [`MediaControlError::InvalidArgument`] from [`socket_path`] when
///   `$XDG_RUNTIME_DIR` is unset or invalid.
pub fn kick() -> Result<KickOutcome> {
    let path = socket_path()?;
    kick_to(&path)
}

/// Implementation detail of [`kick`] — separated so tests can inject the
/// target path without mutating `$XDG_RUNTIME_DIR` (which would race with
/// other tests through the lib's env-mutation mutex).
fn kick_to(path: &Path) -> Result<KickOutcome> {
    let sock = UnixDatagram::unbound()?;
    match sock.send_to(&[], path) {
        Ok(_) => Ok(KickOutcome::Delivered),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(KickOutcome::DaemonDown)
        }
        Err(e) => Err(MediaControlError::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kick_to` returns `DaemonDown` when no socket exists at the path.
    /// This is the primary FR-4 "daemon-down silent" classification: a
    /// missing socket file (the keybind-against-stopped-daemon case) must
    /// not surface as a hard error.
    #[test]
    fn kick_to_classifies_missing_socket_as_daemon_down() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.sock");
        let outcome = kick_to(&path).expect("missing socket must not error");
        assert_eq!(outcome, KickOutcome::DaemonDown);
    }

    /// `kick_to` returns `Delivered` when a `SOCK_DGRAM` socket is bound
    /// at the path. Round-trip happy-path proof of the wire contract.
    #[test]
    fn kick_to_delivers_to_bound_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.sock");
        let _server = UnixDatagram::bind(&path).expect("bind server socket");

        let outcome = kick_to(&path).expect("bound socket must accept");
        assert_eq!(outcome, KickOutcome::Delivered);
    }

    /// The receiver side observes a 0-byte payload. Locks the FR-9 wire
    /// contract from the sender direction: `kick()` always sends 0 bytes.
    #[test]
    fn kick_to_sends_zero_byte_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.sock");
        let server = UnixDatagram::bind(&path).expect("bind server");
        server
            .set_nonblocking(true)
            .expect("set server nonblocking");

        kick_to(&path).expect("kick should deliver");

        let mut buf = [0u8; 16];
        let (n, _) = server.recv_from(&mut buf).expect("server should recv");
        assert_eq!(n, 0, "kick must send a 0-byte canonical kick");
    }

    /// `socket_path` joins onto `$XDG_RUNTIME_DIR` using the canonical
    /// filename constant. Catches drift between the daemon's bind path
    /// and the CLI's send path.
    #[tokio::test]
    async fn socket_path_uses_canonical_filename() {
        let _g = crate::commands::shared::async_env_test_mutex().lock().await;
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: env mutation is serialized via the lib's process-wide
        // async test mutex, acquired above.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        let resolved = socket_path().expect("socket_path");
        assert_eq!(resolved, dir.path().join(SOCKET_FILENAME));
        assert_eq!(SOCKET_FILENAME, "media-control-daemon.sock");
    }
}
