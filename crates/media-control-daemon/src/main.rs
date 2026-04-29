//! Media Control Daemon
//!
//! Event-driven daemon that listens to Hyprland socket events
//! and triggers media window avoidance.
//!
//! Also supports manual triggers via FIFO for events that don't
//! emit Hyprland socket events (like `layoutmsg togglesplit`).

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use media_control_lib::commands::{self, CommandContext, runtime_dir};
use media_control_lib::config::Config;
use media_control_lib::error::MediaControlError;
use media_control_lib::hyprland::{Client, HyprlandError, runtime_socket_path};
use nix::fcntl::{Flock, FlockArg};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Per-tick snapshot of `get_clients()` plus the wall-clock time it was
/// captured. The avoid hot path is event-driven and bursts of related
/// events fire within a single debounce window, so reusing the same
/// snapshot for the whole burst saves an IPC round-trip per event without
/// changing avoidance semantics (the snapshot would be stale by the next
/// `Instant::now() - captured_at >= TTL` check anyway).
struct ClientCache {
    inner: Mutex<Option<(Instant, Vec<Client>)>>,
    /// Time-to-live for a cached snapshot. Pinned to the daemon's debounce
    /// window so the cache horizon never extends past the boundary between
    /// "this burst" and "next burst".
    ttl: Duration,
}

impl ClientCache {
    fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(None),
            ttl,
        }
    }

    /// Return a fresh client list, fetching from Hyprland only when the
    /// cache is missing or older than `ttl`. The returned `Vec` is a clone
    /// so callers can pass it as `&[Client]` without holding the lock.
    async fn get_or_refresh(&self, ctx: &CommandContext) -> Result<Vec<Client>, MediaControlError> {
        if let Some(cached) = self.try_hit() {
            return Ok(cached);
        }
        // Cache cold or stale. Drop the lock across the IPC call so
        // concurrent callers don't serialize on the round-trip.
        debug!("client cache: refetching from Hyprland");
        let fresh = ctx.hyprland.get_clients().await?;
        self.install(fresh.clone());
        Ok(fresh)
    }

    /// Cache-hit test, factored out for testability: returns the cached
    /// clone iff a snapshot exists and is younger than `ttl`. The brief
    /// std-mutex critical section does no IO; the hold-time is negligible
    /// against the full IPC round-trip we avoid by serving from cache.
    fn try_hit(&self) -> Option<Vec<Client>> {
        let guard = self.inner.lock().expect("ClientCache poisoned");
        guard.as_ref().and_then(|(captured_at, cached)| {
            let age = captured_at.elapsed();
            if age < self.ttl {
                debug!("client cache: hit (age={:?})", age);
                Some(cached.clone())
            } else {
                debug!("client cache: miss (stale, age={:?})", age);
                None
            }
        })
    }

    /// Install a snapshot at the current `Instant`. If a parallel caller
    /// also fetched and won the race, the later write replaces it — both
    /// are equally fresh so the order does not matter.
    fn install(&self, clients: Vec<Client>) {
        let mut guard = self.inner.lock().expect("ClientCache poisoned");
        *guard = Some((Instant::now(), clients));
    }
}

/// In-memory mirror of the avoider's suppress timestamp (millis since
/// UNIX epoch). The daemon updates this directly after each successful
/// `trigger_avoid`, so subsequent ticks within the suppress window can
/// short-circuit on a single atomic load instead of a per-event filesystem
/// stat + read.
///
/// The on-disk suppress file remains the cross-process IPC path: CLI
/// commands that warm the file (via `commands::suppress_avoider`) are
/// observed by the file-stat fallback in [`SuppressState::is_suppressed`].
/// This mirror is purely additive — disabling it would just put us back
/// on the file-only path, with no correctness change.
///
/// `0` is the sentinel "never warmed". Any positive value is a real
/// timestamp the comparison treats as the most recent in-memory write.
struct SuppressState {
    last_ms: AtomicU64,
}

impl SuppressState {
    fn new() -> Self {
        Self {
            last_ms: AtomicU64::new(0),
        }
    }

    /// Stamp the in-memory mirror with `now_unix_millis()`. Called by the
    /// daemon after a successful `avoid_with_clients` so future ticks in
    /// the same suppress window observe the warm state without file IO.
    fn warm(&self) {
        // `Release` so the in-memory write happens-before any subsequent
        // `Acquire` load in `is_suppressed`. In practice the loader runs
        // on the same task, so program order alone would suffice; the
        // ordering is conservative against future multi-task callers.
        self.last_ms.store(now_unix_millis(), Ordering::Release);
    }

    /// True iff the avoider should currently skip running. Consults the
    /// in-memory mirror first; falls through to the file-stat path only
    /// when the in-memory value is stale (i.e. cross-process suppress
    /// might be the only source of truth).
    async fn is_suppressed(&self, suppress_timeout_ms: u64) -> bool {
        // `Acquire` pairs with the `Release` in `warm()`.
        let stamp = self.last_ms.load(Ordering::Acquire);
        if stamp != 0 {
            let now = now_unix_millis();
            if now.saturating_sub(stamp) < suppress_timeout_ms {
                debug!(
                    "suppress: in-memory hit (age={}ms)",
                    now.saturating_sub(stamp)
                );
                return true;
            }
        }
        // In-memory cold or stale — defer to the on-disk file so
        // cross-process callers (CLI commands warming the daemon) remain
        // authoritative for *their* writes. Promote a hit back into the
        // mirror so the next tick can short-circuit.
        if let Some(file_stamp) = read_suppress_file_ms().await {
            let now = now_unix_millis();
            if now.saturating_sub(file_stamp) < suppress_timeout_ms {
                debug!(
                    "suppress: file-fallback hit (age={}ms); promoting to mirror",
                    now.saturating_sub(file_stamp)
                );
                self.last_ms.store(file_stamp, Ordering::Release);
                return true;
            }
        }
        false
    }
}

/// Wall-clock millis since UNIX epoch. Saturating so a clock skewed
/// before the epoch (impossible on a healthy system) folds to `0`
/// rather than panicking — matches the lib's `now_unix_millis`
/// semantics.
#[inline]
fn now_unix_millis() -> u64 {
    let raw = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(raw).unwrap_or(u64::MAX)
}

/// Read the suppress file's u64 millis content, returning `None` for
/// missing / unreadable / unparseable files. Mirrors the lib's
/// `should_suppress` failure-mode behaviour: every IO/parse failure is
/// treated as "no suppression" so a stale-or-broken file cannot lock
/// the avoider out indefinitely.
async fn read_suppress_file_ms() -> Option<u64> {
    let path = commands::get_suppress_file_path().ok()?;
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    content.trim().parse::<u64>().ok()
}

/// Event-driven daemon for media window avoidance.
#[derive(Parser)]
#[command(name = "media-control-daemon")]
#[command(about = "Event-driven daemon for media window avoidance")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon (default if no command given)
    Start,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
    /// Run in foreground (for systemd/debugging)
    Foreground,
}

/// Get the path to the PID file (in `$XDG_RUNTIME_DIR`, sanitized).
///
/// Propagates the underlying `runtime_dir` error (typically a missing or
/// dangerous `XDG_RUNTIME_DIR`) rather than panicking, so the daemon can
/// fail with a meaningful message instead of crashing on a misconfigured
/// session.
fn get_pid_file_path() -> Result<PathBuf, MediaControlError> {
    Ok(runtime_dir()?.join("media-control-daemon.pid"))
}

/// Get the path to the trigger FIFO.
///
/// Placed in `$XDG_RUNTIME_DIR` (per-user, mode 0700 by default on systemd
/// systems) rather than world-writable `/tmp`. This defends against symlink
/// races and pre-creation attacks that would otherwise be possible at a
/// predictable `/tmp` path on a multi-user host.
fn get_fifo_path() -> Result<PathBuf, MediaControlError> {
    Ok(runtime_dir()?.join("media-avoider-trigger.fifo"))
}

/// Get the path to Hyprland's socket2 (event stream).
///
/// `async` because `runtime_socket_path` now probes for a live Hyprland
/// instance (intent 017). Propagates the typed `HyprlandError` rather
/// than collapsing it to a `String` — preserves the error variant so
/// callers can distinguish missing-env from malformed-env from
/// no-live-instance, and lets `?` carry source chains.
async fn get_socket2_path() -> Result<PathBuf, HyprlandError> {
    runtime_socket_path(".socket2.sock").await
}

/// Read PID from the PID file.
///
/// Rejects non-positive PIDs to avoid accidentally signalling process group 0
/// or the entire session via `kill(-1, ...)` semantics.
async fn read_pid_file() -> Option<Pid> {
    let path = get_pid_file_path().ok()?;
    let content = fs::read_to_string(&path).await.ok()?;
    let pid: i32 = content.trim().parse().ok()?;
    if pid <= 1 {
        return None;
    }
    Some(Pid::from_raw(pid))
}

/// Write the given PID to the PID file with restrictive permissions.
///
/// Atomic: writes to `<path>.tmp` first, fsyncs, then `rename()`s into place.
/// `rename` on the same filesystem is atomic, so a crash mid-write leaves
/// either the old PID file or the new one — never a half-written file that
/// `read_pid_file` would misparse as "no daemon".
///
/// Uses O_CREAT|O_TRUNC|O_WRONLY with mode 0o600 so the temp file cannot be
/// read by other users on a shared host.
fn write_pid_file_for(pid: u32) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = get_pid_file_path().map_err(std::io::Error::other)?;
    let mut tmp_path = path.clone().into_os_string();
    tmp_path.push(".tmp");
    let tmp_path = PathBuf::from(tmp_path);

    // Write the new PID into a sibling temp file with 0600 perms.
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let mut file = opts.open(&tmp_path)?;
    write!(file, "{pid}")?;
    file.sync_all()?;
    drop(file);

    // Atomic swap: a concurrent reader sees either the old contents or the
    // new ones, never a partial write.
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        // Best-effort cleanup of the temp file; surface the original error.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

/// Write the current process's PID to the PID file.
///
/// Convenience wrapper around [`write_pid_file_for`]. `async` because
/// historical callers awaited it; the inner work is synchronous.
async fn write_pid_file() -> std::io::Result<()> {
    write_pid_file_for(std::process::id())
}

/// Remove the PID file.
async fn remove_pid_file() {
    if let Ok(path) = get_pid_file_path() {
        let _ = fs::remove_file(&path).await;
    }
}

/// Check if a process with the given PID is running AND is plausibly our daemon.
///
/// A bare `kill(pid, 0)` is racy: PIDs are recycled, so a stale PID file may
/// reference an unrelated process — possibly even one owned by a different
/// user. Defend against that by also checking `/proc/<pid>/comm` matches the
/// expected daemon binary name.
///
/// `/proc/<pid>/comm` outcomes:
/// - `Ok(comm)`: trusted — match against the expected name.
/// - `Err(NotFound)`: process is definitively gone, return false.
/// - `Err(other)`: identity is uncertain (permission denied, /proc not
///   mounted, transient EIO). Previously this fell back to trusting the
///   signal-0 check, which on a recycled-PID-owned-by-this-user system
///   would block daemon startup. Treat unknown identity as stale instead
///   and emit a `warn!` so operators can see we couldn't confirm.
///
/// Linux truncates `/proc/PID/comm` to 15 chars (TASK_COMM_LEN=16 inc. NUL),
/// so we accept either the full crate name or the truncated 15-char prefix.
fn is_process_running(pid: Pid) -> bool {
    if signal::kill(pid, None).is_err() {
        return false;
    }
    let comm_path = format!("/proc/{}/comm", pid.as_raw());
    let full = env!("CARGO_PKG_NAME");
    // /proc/PID/comm is truncated to 15 chars on Linux.
    let truncated: &str = if full.len() > 15 { &full[..15] } else { full };
    match std::fs::read_to_string(&comm_path) {
        Ok(comm) => {
            let c = comm.trim();
            c == full || c == truncated
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            warn!("could not verify PID {pid} identity: {e}; assuming stale");
            false
        }
    }
}

/// Trigger the avoid command using the daemon's per-tick client cache
/// and in-memory suppress mirror.
///
/// Suppress short-circuit runs first (cheapest path: a single atomic load
/// when the mirror is warm). Refetches `j/clients` only when the cache is
/// cold or older than the debounce window. After a successful avoid pass,
/// the suppress mirror is warmed so the next tick within the suppress
/// window can short-circuit without touching the file system.
async fn trigger_avoid(ctx: &CommandContext, cache: &ClientCache, suppress: &SuppressState) {
    if suppress
        .is_suppressed(u64::from(ctx.config.positioning.suppress_ms))
        .await
    {
        debug!("avoid: suppressed (daemon mirror)");
        return;
    }
    let clients = match cache.get_or_refresh(ctx).await {
        Ok(c) => c,
        Err(e) => {
            debug!("Avoid client-cache refresh failed: {}", e);
            return;
        }
    };
    if let Err(e) = commands::avoid::avoid_with_clients(ctx, &clients).await {
        debug!("Avoid error: {}", e);
        return;
    }
    // Warm the in-memory mirror: the lib's avoid path may have called
    // `suppress_avoider` (writing the file), but the next tick should
    // short-circuit without re-reading it. Setting the mirror here
    // means subsequent `is_suppressed` calls hit the atomic load path.
    suppress.warm();
}

/// Create a trigger FIFO at `path`, hardened against pre-creation attacks.
///
/// Uses `lstat` (no symlink resolution) to inspect the existing entry,
/// refuses to remove anything that isn't a real FIFO owned by us, and
/// creates the new FIFO with mode 0o600.
fn create_fifo_at(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    // Use symlink_metadata (lstat) — does NOT follow symlinks. If an attacker
    // pre-created a symlink at our path we'd otherwise remove the target.
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let ft = meta.file_type();
            if ft.is_symlink() {
                return Err(std::io::Error::other(format!(
                    "refusing to use {path:?}: it is a symlink"
                )));
            }
            if !ft.is_fifo() {
                return Err(std::io::Error::other(format!(
                    "refusing to use {path:?}: not a FIFO"
                )));
            }
            // Only remove if it's owned by us.
            let our_uid = nix::unistd::Uid::current().as_raw();
            if meta.uid() != our_uid {
                return Err(std::io::Error::other(format!(
                    "refusing to use {path:?}: owned by uid {} (expected {our_uid})",
                    meta.uid()
                )));
            }
            std::fs::remove_file(path)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    // mkfifo creates atomically; mode 0o600 is masked by umask, so a
    // restrictive process umask (say 0o077) is fine but a permissive one
    // (e.g. 0o022) would leave the FIFO group/other-readable. Force the
    // intended mode with an explicit `set_permissions` after creation
    // so the FIFO is always 0o600 regardless of inherited umask.
    //
    // The previous symlink check + per-user runtime dir means a TOCTOU
    // race here would require write access to $XDG_RUNTIME_DIR, which on
    // systemd systems is the user's own 0700 directory.
    nix::unistd::mkfifo(path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .map_err(std::io::Error::other)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// Create the trigger FIFO at the default daemon path.
fn create_fifo() -> std::io::Result<PathBuf> {
    let path = get_fifo_path().map_err(std::io::Error::other)?;
    create_fifo_at(&path)?;
    info!("Created trigger FIFO at {:?}", path);
    Ok(path)
}

/// Remove the trigger FIFO.
///
/// Uses `symlink_metadata` (lstat) instead of `path.exists()` so we never
/// follow a symlink that may have been planted at our path between the
/// check and the unlink. `remove_file` itself does NOT follow symlinks.
fn remove_fifo() {
    let Ok(path) = get_fifo_path() else { return };
    if std::fs::symlink_metadata(&path).is_ok() {
        let _ = std::fs::remove_file(&path);
    }
}

/// Listen for triggers on the FIFO and send them to the channel.
///
/// The FIFO is opened in a loop because each write closes it on the writer side.
/// Read errors are logged and bounded by [`FIFO_ERROR_BACKOFF`] to prevent a
/// hot spin on persistent failure (e.g. underlying file removed by another
/// process). EINTR is surfaced by tokio as `Interrupted`; we treat it the same
/// as any other transient error and reopen.
///
/// Sends are non-blocking via `try_send` and coalesce on `Full`: every FIFO
/// line is an idempotent "re-evaluate placement" trigger, so a flooding
/// writer can't stall the listener — if there's already a pending event in
/// the channel a second won't change behaviour. `await`ing `tx.send` here
/// would let a flood backpressure the listener and drop events behind a
/// queue we don't actually need.
async fn fifo_listener(tx: mpsc::Sender<()>) {
    use tokio::sync::mpsc::error::TrySendError;

    /// Backoff applied after any read or open error. Bounded so a missing
    /// or broken FIFO can't burn CPU.
    const FIFO_ERROR_BACKOFF: Duration = Duration::from_millis(100);

    let path = match get_fifo_path() {
        Ok(p) => p,
        Err(e) => {
            error!("FIFO listener cannot resolve path: {e}");
            return;
        }
    };

    loop {
        // Open FIFO for reading (blocks until a writer connects)
        let file = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open FIFO: {}", e);
                tokio::time::sleep(FIFO_ERROR_BACKOFF).await;
                continue;
            }
        };

        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Drain lines until EOF (writer closed) or hard error.
        loop {
            match lines.next_line().await {
                Ok(Some(_)) => {
                    debug!("Received FIFO trigger");
                    match tx.try_send(()) {
                        Ok(()) => {}
                        // Coalesce: a pending event already exists in the
                        // channel and will be processed; the duplicate is
                        // semantically identical so we drop it silently.
                        Err(TrySendError::Full(())) => {
                            debug!("FIFO trigger coalesced (channel full)");
                        }
                        Err(TrySendError::Closed(())) => return,
                    }
                }
                Ok(None) => break, // EOF — writer closed, reopen
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    // EINTR — retry the read without backoff
                    continue;
                }
                Err(e) => {
                    warn!("FIFO read error: {} — reopening after backoff", e);
                    tokio::time::sleep(FIFO_ERROR_BACKOFF).await;
                    break;
                }
            }
        }
    }
}

/// Connect to Hyprland socket2 with retries and backoff.
///
/// Path resolution happens **inside** the loop so a Hyprland restart
/// (which produces a new HIS) is recovered within one retry tick instead
/// of hammering a dead path forever (intent 017 / FR-4 — outer-loop
/// case; the inner-loop tighter coverage lands in unit 002 / bolt 029).
async fn connect_hyprland_socket() -> Result<UnixStream, HyprlandError> {
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(10);

    loop {
        match get_socket2_path().await {
            Ok(socket_path) => {
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    UnixStream::connect(&socket_path),
                )
                .await
                {
                    Ok(Ok(stream)) => {
                        info!("Connected to Hyprland socket at {:?}", socket_path);
                        return Ok(stream);
                    }
                    Ok(Err(e)) => {
                        warn!(
                            "Failed to connect to Hyprland socket {socket_path:?}: {e} (retry in {backoff:?})"
                        );
                    }
                    Err(_) => {
                        warn!(
                            "Timed out connecting to Hyprland socket {socket_path:?} (retry in {backoff:?})"
                        );
                    }
                }
            }
            Err(e) => {
                warn!("Failed to resolve Hyprland socket path: {e} (retry in {backoff:?})");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(max_backoff);
    }
}

/// Hyprland event names that warrant a re-evaluation of media window placement.
const AVOID_EVENTS: &[&str] = &[
    "workspace",
    "activewindow",
    "movewindow",
    "openwindow",
    "closewindow",
    "swapwindow",
];

/// Returns true if a Hyprland event line should trigger an avoid pass.
#[inline]
fn is_avoid_trigger(line: &str) -> bool {
    let (event, _) = line.split_once(">>").unwrap_or((line, ""));
    AVOID_EVENTS.contains(&event)
}

/// Run a single session of the event loop (until socket disconnect).
/// Returns Ok(true) to signal reconnect, Ok(false) for clean shutdown.
///
/// Returns the typed [`MediaControlError`] rather than `Box<dyn Error>` so
/// the caller (and the log line in `run_event_loop`) can pattern-match the
/// failure mode (`HyprlandIpc` connection vs `Io` etc) and so source chains
/// remain inspectable end-to-end.
async fn run_event_session(
    ctx: &CommandContext,
    cache: &ClientCache,
    suppress: &SuppressState,
    debounce_duration: Duration,
    fifo_rx: &mut mpsc::Receiver<()>,
) -> Result<bool, MediaControlError> {
    let stream = connect_hyprland_socket().await?;
    let mut lines = BufReader::new(stream).lines();
    let mut last_avoid_time = Instant::now();

    // Helper closure: apply debounce-and-trigger logic uniformly.
    async fn maybe_trigger(
        ctx: &CommandContext,
        cache: &ClientCache,
        suppress: &SuppressState,
        last: &mut Instant,
        debounce: Duration,
    ) {
        if last.elapsed() >= debounce {
            trigger_avoid(ctx, cache, suppress).await;
            *last = Instant::now();
        }
    }

    info!("Event session started");

    loop {
        tokio::select! {
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        if is_avoid_trigger(&line) {
                            maybe_trigger(ctx, cache, suppress, &mut last_avoid_time, debounce_duration).await;
                        }
                    }
                    Ok(None) => {
                        warn!("Hyprland socket closed, will reconnect");
                        return Ok(true);
                    }
                    Err(e) => {
                        error!("Hyprland socket read error: {}, will reconnect", e);
                        return Ok(true);
                    }
                }
            }

            Some(()) = fifo_rx.recv() => {
                debug!("Processing FIFO trigger");
                maybe_trigger(ctx, cache, suppress, &mut last_avoid_time, debounce_duration).await;
            }
        }
    }
}

/// RAII guard that aborts a spawned task when dropped.
///
/// Without this, dropping a `JoinHandle` only releases our handle — the
/// task continues running. We need active abort so that when
/// `run_event_loop` is cancelled by the outer `tokio::select!` (SIGINT /
/// SIGTERM) the FIFO listener cannot still be inside `File::open` against
/// a path we are about to `unlink`. That race produced a noisy error log
/// on every clean shutdown.
struct AbortOnDrop(Option<tokio::task::JoinHandle<()>>);

impl AbortOnDrop {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(handle))
    }

    /// Take the inner handle, suppressing the abort-on-drop. Used when
    /// caller wants to manage abort timing explicitly.
    fn take(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.0.take()
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

/// Run the event loop with automatic reconnection.
async fn run_event_loop() -> Result<(), MediaControlError> {
    let config = Config::load_or_warn(None);
    let debounce_ms = config.positioning.debounce_ms;
    let ctx = CommandContext::with_config(config).await?;
    let debounce_duration = Duration::from_millis(u64::from(debounce_ms));
    // TTL = the debounce window. A burst of related events fires within
    // this window and benefits from a shared snapshot; the very next
    // burst (after debounce_duration elapses) refetches naturally.
    let client_cache = ClientCache::new(debounce_duration);
    // In-memory mirror of the suppress timestamp. Cold on startup
    // (`0`), warmed by `trigger_avoid` after each successful pass.
    let suppress_state = SuppressState::new();

    // Create FIFO for manual triggers
    create_fifo()?;

    // Channel for FIFO triggers — persists across reconnections.
    // The listener's `JoinHandle` is held in an RAII abort-guard so that
    // any cancellation of this future (SIGTERM/SIGINT in the outer
    // select!) actively aborts the listener before its drop. Without the
    // active abort, the listener could still be mid-`File::open` on the
    // FIFO path that `remove_fifo()` is about to unlink, producing a
    // spurious error log every clean shutdown.
    let (fifo_tx, mut fifo_rx) = mpsc::channel::<()>(16);
    let mut fifo_listener_handle = AbortOnDrop::new(tokio::spawn(fifo_listener(fifo_tx)));

    info!("Event loop started");

    loop {
        match run_event_session(
            &ctx,
            &client_cache,
            &suppress_state,
            debounce_duration,
            &mut fifo_rx,
        )
        .await
        {
            Ok(true) => {
                // Reconnect after brief delay
                info!("Reconnecting to Hyprland socket...");
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Recreate FIFO in case it was cleaned up. Use lstat
                // (symlink_metadata) instead of `.exists()` so a symlink
                // planted at our path doesn't trick us into skipping the
                // recreate; `create_fifo_at` will then reject the symlink
                // explicitly with a clear error.
                let fifo_missing = match get_fifo_path() {
                    Ok(p) => std::fs::symlink_metadata(&p).is_err(),
                    // Path resolution itself failed — treat as missing so
                    // we attempt recreation (which will surface the same
                    // error from `create_fifo`).
                    Err(_) => true,
                };
                if fifo_missing && let Err(e) = create_fifo() {
                    error!("Failed to recreate FIFO after reconnect: {e}");
                    // Stop the listener so it doesn't burn CPU repeatedly
                    // failing to open a FIFO we can't recreate. The main
                    // loop continues handling Hyprland events; only manual
                    // FIFO triggers are lost.
                    if let Some(h) = fifo_listener_handle.take() {
                        h.abort();
                    }
                }
            }
            Ok(false) => {
                info!("Event loop ended (clean shutdown)");
                return Ok(());
            }
            Err(e) => {
                error!("Event session error: {}, retrying in 2s", e);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

/// Run the daemon in foreground mode.
async fn run_foreground() -> ExitCode {
    info!("Starting media-control-daemon in foreground mode");

    // Write PID file only if it doesn't already contain our PID. When the
    // daemon was started via `cmd_start`, the parent has already written
    // the PID file and dropped the start lock; rewriting it here is
    // redundant churn (and an unnecessary atomic-rename). When the daemon
    // is launched directly (systemd, debugging, foreground subcommand by
    // hand) the file will be missing or hold a different value, so we
    // need to claim it.
    let our_pid = std::process::id();
    let already_ours = read_pid_file()
        .await
        .map(|p| p.as_raw() == our_pid as i32)
        .unwrap_or(false);
    if !already_ours && let Err(e) = write_pid_file().await {
        error!("Failed to write PID file: {}", e);
        return ExitCode::FAILURE;
    }

    // Run event loop with graceful shutdown
    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(s) => s,
        Err(e) => {
            error!("failed to register SIGTERM handler: {e}");
            remove_pid_file().await;
            return ExitCode::FAILURE;
        }
    };

    let result = tokio::select! {
        result = run_event_loop() => result,
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down");
            Ok(())
        }
    };

    // Clean up PID file and FIFO
    remove_pid_file().await;
    remove_fifo();

    match result {
        Ok(()) => {
            info!("Daemon stopped cleanly");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!("Daemon error: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Path of the start-lock file (single source of truth).
///
/// The file persists on disk; serialization comes from `flock(LOCK_EX)` on
/// the open FD, not from the file's existence. We never unlink it, so there
/// is no "stale lock" state to recover from.
fn start_lock_path() -> Result<PathBuf, MediaControlError> {
    Ok(runtime_dir()?.join("media-control-daemon.start.lock"))
}

/// Acquire an exclusive `flock(LOCK_EX | LOCK_NB)` on the start-lock file.
///
/// # Invariant (TOCTOU-free)
///
/// The lock is the kernel-side `flock`, NOT the existence of the on-disk
/// sentinel. The file is created (O_CREAT, mode 0o600) and the lock is
/// taken in a single function with no intervening release. While the
/// returned `Flock` is held:
///
/// - any other process calling `acquire_start_lock` returns `EWOULDBLOCK`
///   (translated by `Flock::lock` into `Err`), and
/// - dropping the `Flock` (or process exit, including SIGKILL or panic)
///   atomically releases the lock — the kernel handles this on FD close,
///   so a crashed `cmd_start` can never leave behind a stuck lock.
///
/// This replaces an earlier `O_EXCL`-on-sentinel scheme that had a TOCTOU
/// window in `acquire_or_recover_start_lock`: between the staleness check
/// and the recovery-`unlink`, another `cmd_start` could swoop in and
/// release a healthy daemon's lock. With `flock`, no such recovery dance
/// is needed — the kernel never lets two processes hold LOCK_EX at once,
/// and there's no on-disk state for a crashed process to "leak".
fn acquire_start_lock() -> std::io::Result<Flock<std::fs::File>> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = start_lock_path().map_err(std::io::Error::other)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(_f, errno)| {
        // EWOULDBLOCK is the "another start in progress" case; surface it
        // explicitly so the caller can distinguish it from genuine I/O
        // errors.
        std::io::Error::from_raw_os_error(errno as i32)
    })
}

/// Returns true if `e` originated from a non-blocking `flock` losing the
/// race for the lock (i.e. another process holds it). On Linux
/// `EWOULDBLOCK` is the same value as `EAGAIN`, which is the errno
/// `flock(LOCK_EX | LOCK_NB)` returns when another holder exists.
fn is_lock_held_elsewhere(e: &std::io::Error) -> bool {
    let Some(n) = e.raw_os_error() else {
        return false;
    };
    nix::errno::Errno::from_raw(n) == nix::errno::Errno::EAGAIN
}

/// Acquire the start lock. Returns the live lock handle on success or an
/// `ExitCode::FAILURE` after logging if the lock is held by a concurrent
/// start.
///
/// No recovery branch is needed: `flock` is released by the kernel on FD
/// close (including process death), so a crashed previous start cannot leave
/// the lock stuck. A held lock therefore unambiguously means another
/// `cmd_start` is currently running.
async fn acquire_or_recover_start_lock() -> Result<Flock<std::fs::File>, ExitCode> {
    acquire_start_lock().map_err(|e| {
        if is_lock_held_elsewhere(&e) {
            error!("Daemon start already in progress (start-lock held)");
        } else {
            error!("Failed to acquire start lock: {e}");
        }
        ExitCode::FAILURE
    })
}

/// Spawn the daemon's foreground worker as a detached background process.
///
/// Two important pieces of session/PID hygiene happen here:
///
/// 1. `process_group(0)` puts the child into its own process group, so a
///    SIGHUP delivered to the parent's controlling terminal (e.g. when the
///    user closes the shell that ran `media-control-daemon start`) does
///    not propagate to the daemon. Mirrors the launch path in
///    `commands::focus`.
///
/// 2. The parent writes the PID file from `child.id()` *before* returning
///    (and therefore before the start-lock guard releases). Previously the
///    child wrote its own PID asynchronously inside `run_foreground`,
///    which left a window where a second `cmd_start` could run, see no
///    PID file, and conclude the daemon was not running — racing into a
///    second spawn. Writing from the parent closes that window: by the
///    time the lock releases, any subsequent `read_pid_file` either sees
///    our PID or the start lock is still held.
fn spawn_foreground_worker() -> ExitCode {
    use std::os::unix::process::CommandExt;

    let exe = match env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to get executable path: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut command = std::process::Command::new(&exe);
    command
        .arg("foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);

    let child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to start daemon: {e}");
            return ExitCode::FAILURE;
        }
    };

    let pid = child.id();
    if let Err(e) = write_pid_file_for(pid) {
        // Couldn't claim the PID file — the child is already running but
        // we have no way for `cmd_stop`/`cmd_status` to find it. Best
        // effort: SIGTERM the child so we don't leak it, then surface the
        // error.
        error!("Failed to write PID file for spawned daemon (PID {pid}): {e}");
        let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        return ExitCode::FAILURE;
    }

    // `info!` (not `println!`): user-facing status flows through the
    // tracing subscriber so it can be silenced/redirected uniformly.
    info!("Daemon started (PID {pid})");
    ExitCode::SUCCESS
}

/// Start the daemon in background mode.
async fn cmd_start() -> ExitCode {
    // Atomically acquire start-lock to serialize concurrent `start` calls.
    // Without this, two `start` invocations could each pass the PID-check
    // below and both spawn a daemon; the second's PID file would clobber
    // the first, leaking a daemon process.
    //
    // The lock is a kernel `flock(LOCK_EX)` on a persistent sentinel file
    // — released automatically when `_start_lock` is dropped (or when the
    // process exits, including SIGKILL or panic). No on-disk cleanup is
    // required, so the previous TOCTOU between "release stale" and
    // "re-acquire" is gone: there is no "stale" state to recover from.
    let _start_lock = match acquire_or_recover_start_lock().await {
        Ok(f) => f,
        Err(code) => return code,
    };

    // Check if already running (now serialized by the lock above).
    if let Some(pid) = read_pid_file().await {
        if is_process_running(pid) {
            error!("Daemon already running (PID {pid})");
            return ExitCode::FAILURE;
        }
        // Stale PID file, remove it
        remove_pid_file().await;
    }

    spawn_foreground_worker()
}

/// Poll-interval and total timeouts for the SIGTERM/SIGKILL wait loops in
/// [`cmd_stop`]. Tuned together: a 50 ms poll keeps shell latency low, the
/// 2 s SIGTERM window covers the daemon's clean-shutdown path (FIFO drain +
/// PID-file unlink + Hyprland socket close, all sub-second on a healthy
/// system), and the 1 s SIGKILL window leaves room for the kernel to reap
/// after KILL — which can't be ignored, so anything longer indicates the
/// process is stuck in uninterruptible sleep (D state), at which point an
/// error return is the correct answer.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);
const STOP_TERM_TIMEOUT: Duration = Duration::from_secs(2);
const STOP_KILL_TIMEOUT: Duration = Duration::from_secs(1);

/// Wait up to `total` for `pid` to stop being a live process.
///
/// Returns `true` once the PID is gone, `false` if the deadline passes and
/// the process is still alive. Polls every [`STOP_POLL_INTERVAL`] so a
/// fast-exiting daemon is detected within ~50 ms.
async fn wait_for_exit(pid: Pid, total: Duration) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if !is_process_running(pid) {
            return true;
        }
        tokio::time::sleep(STOP_POLL_INTERVAL).await;
    }
    !is_process_running(pid)
}

/// Stop the running daemon.
///
/// Sends SIGTERM, waits up to [`STOP_TERM_TIMEOUT`] for clean shutdown, then
/// escalates to SIGKILL if the daemon hasn't exited. Returns success only
/// once the PID is verifiably gone — earlier behaviour returned immediately
/// after sending SIGTERM, which let a subsequent `cmd_start` race against a
/// daemon still holding the PID file (and lose, double-spawning).
async fn cmd_stop() -> ExitCode {
    let Some(pid) = read_pid_file().await else {
        error!("Daemon not running (no PID file)");
        return ExitCode::FAILURE;
    };

    if !is_process_running(pid) {
        error!("Daemon not running (stale PID file)");
        remove_pid_file().await;
        return ExitCode::FAILURE;
    }

    // Send SIGTERM and wait for the daemon's signal handler to drive a
    // clean shutdown.
    if let Err(e) = signal::kill(pid, Signal::SIGTERM) {
        error!("Failed to send SIGTERM to PID {pid}: {e}");
        return ExitCode::FAILURE;
    }
    info!("Sent SIGTERM to daemon (PID {pid})");

    if wait_for_exit(pid, STOP_TERM_TIMEOUT).await {
        // Best-effort PID-file cleanup: the daemon's own shutdown removes
        // it, but we re-check in case it crashed between SIGTERM receipt
        // and `remove_pid_file`. `remove_pid_file` is idempotent.
        remove_pid_file().await;
        info!("Daemon stopped (PID {pid})");
        return ExitCode::SUCCESS;
    }

    // Escalate. SIGKILL is uncatchable, so failure to exit after this
    // window means the kernel hasn't reaped — D-state or similar.
    warn!("Daemon (PID {pid}) did not exit after SIGTERM; escalating to SIGKILL");
    if let Err(e) = signal::kill(pid, Signal::SIGKILL) {
        error!("Failed to send SIGKILL to PID {pid}: {e}");
        return ExitCode::FAILURE;
    }

    if wait_for_exit(pid, STOP_KILL_TIMEOUT).await {
        remove_pid_file().await;
        info!("Daemon killed (PID {pid})");
        ExitCode::SUCCESS
    } else {
        error!(
            "Daemon (PID {pid}) still running after SIGKILL + {:?} — kernel has not reaped",
            STOP_KILL_TIMEOUT
        );
        ExitCode::FAILURE
    }
}

/// Check daemon status.
async fn cmd_status() -> ExitCode {
    let Some(pid) = read_pid_file().await else {
        info!("Daemon not running (no PID file)");
        return ExitCode::FAILURE;
    };

    if is_process_running(pid) {
        info!("Daemon running (PID {pid})");
        ExitCode::SUCCESS
    } else {
        info!("Daemon not running (stale PID file for PID {pid})");
        ExitCode::FAILURE
    }
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("media_control_daemon=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialize logging for ALL subcommands. Previously only `foreground`
    // installed a tracing subscriber, so `tracing::error!` calls in
    // `cmd_start`/`cmd_stop`/`cmd_status` (and any helper they invoke)
    // were silently dropped. Initializing globally lets us emit structured
    // logs uniformly and replace the bare `eprintln!` calls in the
    // background-mode paths.
    init_logging();

    match cli.command.unwrap_or(Commands::Start) {
        Commands::Start => cmd_start().await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Foreground => run_foreground().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;

    /// Async serialization for tests in this binary that mutate
    /// `XDG_RUNTIME_DIR` across `.await`. Forwards to the lib's
    /// process-wide env mutex so daemon tests share the same lock
    /// domain as the lib's `with_isolated_runtime_dir` (and any
    /// future test-helpers in this crate that go through it).
    /// Two domains would race on `XDG_RUNTIME_DIR` mutations.
    fn env_test_lock() -> &'static tokio::sync::Mutex<()> {
        media_control_lib::commands::shared::async_env_test_mutex()
    }

    /// Avoid-trigger event matcher correctly identifies relevant events
    /// and ignores everything else. This guards against silent regression
    /// if the AVOID_EVENTS list is reordered or trimmed.
    #[test]
    fn is_avoid_trigger_matches_relevant_events() {
        for ev in AVOID_EVENTS {
            assert!(is_avoid_trigger(&format!("{ev}>>some,data")), "{ev}");
            // No payload is also valid (just the event token).
            assert!(is_avoid_trigger(ev), "{ev} (no payload)");
        }
        for ev in &["createworkspace", "monitoradded", "submap", ""] {
            assert!(!is_avoid_trigger(&format!("{ev}>>x")), "{ev}");
        }
    }

    /// FIFO creation must succeed at a fresh path and produce an actual FIFO
    /// (not a regular file or symlink). Verifies the happy path.
    #[test]
    fn create_fifo_at_creates_fifo_at_fresh_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trigger.fifo");

        create_fifo_at(&path).expect("fresh-path create should succeed");

        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(meta.file_type().is_fifo());
    }

    /// FIFO creation must REFUSE to remove an existing symlink, even one
    /// pointing at a valid target. This is the symlink-attack defense.
    #[test]
    fn create_fifo_at_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real-target");
        std::fs::write(&target, "data").unwrap();
        let link = dir.path().join("link.fifo");
        symlink(&target, &link).unwrap();

        let err = create_fifo_at(&link).expect_err("symlink must be rejected");
        assert!(
            err.to_string().contains("symlink"),
            "error should mention symlink: {err}"
        );
        // Target must still exist — we did not blindly unlink.
        assert!(target.exists());
    }

    /// FIFO creation must refuse a non-FIFO file (e.g. a regular file
    /// planted at the path).
    #[test]
    fn create_fifo_at_rejects_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-fifo");
        std::fs::write(&path, "not a fifo").unwrap();

        let err = create_fifo_at(&path).expect_err("regular file must be rejected");
        assert!(err.to_string().contains("not a FIFO"), "{err}");
    }

    /// FIFO creation must refuse a stale FIFO owned by a different uid.
    /// We can't easily create cross-uid files in tests without root, so we
    /// assert via the code path: when an existing FIFO is ours, it is
    /// removed and recreated cleanly (the inverse of the ownership check).
    #[test]
    fn create_fifo_at_replaces_our_own_existing_fifo() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replace.fifo");

        // First creation
        create_fifo_at(&path).expect("first create");
        let inode_before = std::fs::symlink_metadata(&path).unwrap().ino();

        // Second creation should succeed (we own it) and replace inode.
        create_fifo_at(&path).expect("recreate-our-own");
        let inode_after = std::fs::symlink_metadata(&path).unwrap().ino();

        assert_ne!(
            inode_before, inode_after,
            "our own existing FIFO should be removed and recreated"
        );
    }

    /// `is_process_running` must reject PIDs whose `/proc/.../comm` does
    /// not match the daemon binary name. We test this against PID 1
    /// (init/systemd) which is always alive but never our daemon.
    #[test]
    fn is_process_running_rejects_unrelated_pid() {
        // PID 1 always exists on Linux but is not our daemon.
        // `read_pid_file` filters out pid <= 1, but `is_process_running`
        // itself should still reject it via the comm check.
        let pid = Pid::from_raw(1);
        // Skip if we can't even read /proc/1/comm (e.g. unusual sandbox).
        if std::fs::read_to_string("/proc/1/comm").is_err() {
            return;
        }
        assert!(
            !is_process_running(pid),
            "PID 1 must not be identified as our daemon"
        );
    }

    /// `is_process_running` returns false for PIDs that don't exist.
    /// We pick a deliberately huge PID unlikely to be assigned.
    #[test]
    fn is_process_running_rejects_nonexistent_pid() {
        // 2^22 - 1 — above default kernel.pid_max on most systems.
        let pid = Pid::from_raw(4_194_303);
        assert!(!is_process_running(pid));
    }

    /// Build a minimal `Vec<Client>` for cache tests. We only care about
    /// identity here — geometry / focus state are irrelevant to the
    /// cache's freshness invariants.
    fn make_cache_clients(addr: &str) -> Vec<Client> {
        use media_control_lib::hyprland::Workspace;
        vec![Client {
            address: addr.to_string(),
            mapped: true,
            hidden: false,
            at: [0, 0],
            size: [100, 100],
            workspace: Workspace {
                id: 1,
                name: "1".to_string(),
            },
            floating: false,
            pinned: false,
            fullscreen: 0,
            monitor: 0,
            pid: 0,
            class: "test".to_string(),
            title: "test".to_string(),
            focus_history_id: 0,
        }]
    }

    /// A freshly-installed snapshot must be returned by `try_hit` while
    /// it is still inside the TTL window.
    #[test]
    fn client_cache_hit_within_ttl() {
        let cache = ClientCache::new(Duration::from_secs(60));
        cache.install(make_cache_clients("0xa1"));
        let hit = cache.try_hit().expect("snapshot should be fresh");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].address, "0xa1");
    }

    /// A snapshot older than the TTL must miss — the cache horizon never
    /// extends past the boundary the daemon's debounce window pins it to.
    #[test]
    fn client_cache_miss_after_ttl_expiry() {
        // Zero TTL means every snapshot is born stale.
        let cache = ClientCache::new(Duration::from_millis(0));
        cache.install(make_cache_clients("0xa1"));
        assert!(cache.try_hit().is_none(), "0-TTL snapshot must miss");
    }

    /// A cache that has never been installed must miss. The fast-path
    /// branch in `get_or_refresh` falls through to the IPC fetch in this
    /// state.
    #[test]
    fn client_cache_cold_misses() {
        let cache = ClientCache::new(Duration::from_secs(60));
        assert!(cache.try_hit().is_none(), "cold cache must miss");
    }

    /// Re-installing a snapshot replaces the prior one and resets the
    /// captured-at timestamp. Verified by installing a stale snapshot in
    /// a zero-TTL cache (which would always miss), then re-installing
    /// with a generous TTL and asserting the hit returns the latest data.
    #[test]
    fn client_cache_install_replaces_snapshot() {
        let cache = ClientCache::new(Duration::from_secs(60));
        cache.install(make_cache_clients("0xa1"));
        cache.install(make_cache_clients("0xb2"));
        let hit = cache.try_hit().expect("re-installed snapshot should hit");
        assert_eq!(hit[0].address, "0xb2", "must return the latest install");
    }

    /// A cold suppress mirror (never warmed) plus an unset runtime dir
    /// (so file-fallback also misses) must report not-suppressed. This
    /// is the default state on daemon startup.
    #[tokio::test]
    async fn suppress_state_cold_returns_not_suppressed() {
        // Hold the env-mutex so we can clear XDG_RUNTIME_DIR safely.
        let _g = env_test_lock().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: single-threaded test under the env-mutex
        unsafe {
            env::remove_var("XDG_RUNTIME_DIR");
        }

        let state = SuppressState::new();
        let suppressed = state.is_suppressed(60_000).await;

        // SAFETY: restore env before the assert (which can panic).
        unsafe {
            if let Some(v) = original {
                env::set_var("XDG_RUNTIME_DIR", v);
            }
        }
        assert!(
            !suppressed,
            "cold mirror + missing XDG_RUNTIME_DIR must not suppress"
        );
    }

    /// `warm()` flips the in-memory mirror so the next `is_suppressed`
    /// short-circuits on the atomic load — no file IO needed. This is
    /// the syscall-elimination path Story 006 was designed for.
    #[tokio::test]
    async fn suppress_state_warm_skips_file_io() {
        // Hold the env-mutex to block parallel file-touching tests.
        let _g = env_test_lock().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        // Point at a directory that *does not contain* a suppress file.
        // If `warm()` worked, `is_suppressed` returns true without ever
        // reading the (nonexistent) file. If it ignored the warm signal,
        // the file lookup would miss and we'd see false.
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test under the env-mutex
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        let state = SuppressState::new();
        state.warm();
        let suppressed = state.is_suppressed(60_000).await;

        // SAFETY: restore env before the assert.
        unsafe {
            if let Some(v) = original {
                env::set_var("XDG_RUNTIME_DIR", v);
            } else {
                env::remove_var("XDG_RUNTIME_DIR");
            }
        }
        assert!(suppressed, "warm mirror must short-circuit on atomic load");
    }

    /// An external write to the suppress file (cross-process: CLI command
    /// warming the daemon) must be observed via the file-fallback path
    /// even when the in-memory mirror is cold. Story 006 explicitly
    /// preserves this IPC channel.
    #[tokio::test]
    async fn suppress_state_observes_external_file_write() {
        let _g = env_test_lock().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        // Write a recent timestamp into the suppress file path the lib uses.
        let path = commands::get_suppress_file_path().expect("path resolves with XDG set");
        let now_ms = now_unix_millis();
        tokio::fs::write(&path, now_ms.to_string()).await.unwrap();

        let state = SuppressState::new(); // mirror is cold
        let suppressed = state.is_suppressed(60_000).await;

        // SAFETY: restore env before assert
        unsafe {
            if let Some(v) = original {
                env::set_var("XDG_RUNTIME_DIR", v);
            } else {
                env::remove_var("XDG_RUNTIME_DIR");
            }
        }
        assert!(
            suppressed,
            "cold mirror must fall through to fresh file stamp"
        );
    }

    /// Stale file timestamps (older than the suppress window) must not
    /// suppress. Boundary check that the file-fallback path applies the
    /// same `now - stamp < timeout_ms` logic as the in-memory check.
    #[tokio::test]
    async fn suppress_state_rejects_stale_file_timestamp() {
        let _g = env_test_lock().lock().await;
        let original = env::var("XDG_RUNTIME_DIR").ok();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        let path = commands::get_suppress_file_path().expect("path resolves");
        // Timestamp from 1970 — definitely stale.
        tokio::fs::write(&path, "1").await.unwrap();

        let state = SuppressState::new();
        let suppressed = state.is_suppressed(60_000).await;

        // SAFETY: restore env
        unsafe {
            if let Some(v) = original {
                env::set_var("XDG_RUNTIME_DIR", v);
            } else {
                env::remove_var("XDG_RUNTIME_DIR");
            }
        }
        assert!(!suppressed, "stale file timestamp must not suppress");
    }

    /// `read_pid_file` must reject pid <= 1 to defend against accidentally
    /// signalling the entire process group via kill(0)/kill(-1) semantics.
    #[tokio::test]
    async fn read_pid_file_rejects_low_pids() {
        // Hold the async env-mutex for the whole body — `runtime_dir()`
        // (called by `get_pid_file_path()`) reads `XDG_RUNTIME_DIR` from
        // the process env, which we mutate here. Any future test that
        // also touches `XDG_RUNTIME_DIR` will serialize through this.
        let _g = env_test_lock().lock().await;

        // Set XDG_RUNTIME_DIR to a temp dir for isolation.
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test
        let original = env::var("XDG_RUNTIME_DIR").ok();
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        let pid_path = get_pid_file_path().expect("pid path resolves with XDG set");
        for bad in &["0", "1", "-1", "-9999", "not a number", ""] {
            tokio::fs::write(&pid_path, bad).await.unwrap();
            assert!(read_pid_file().await.is_none(), "pid {bad:?} must reject");
        }

        // Sanity check: a plausible pid round-trips.
        tokio::fs::write(&pid_path, "12345").await.unwrap();
        assert_eq!(read_pid_file().await.map(Pid::as_raw), Some(12345));

        // Restore env
        unsafe {
            if let Some(v) = original {
                env::set_var("XDG_RUNTIME_DIR", v);
            } else {
                env::remove_var("XDG_RUNTIME_DIR");
            }
        }
    }

    /// Locks in FR-4 (intent 017 unit 002): when the daemon's connect-retry
    /// loop starts with no live Hyprland and a fresh instance comes up
    /// mid-retry, the next loop iteration must re-resolve and connect to it
    /// — not stay stuck on the dead path.
    ///
    /// Production code already does this: `connect_hyprland_socket` calls
    /// `get_socket2_path().await` inside the loop body (which delegates
    /// through the resolver added in bolt 028). This test exists to keep
    /// that property locked against a future refactor that hoists the
    /// resolve back outside the loop.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connect_loop_picks_up_live_instance_mid_retry() {
        use media_control_lib::test_helpers::{
            InstancePolicy, MockHyprlandInstance, with_isolated_runtime_dir,
        };
        use std::time::Duration;

        with_isolated_runtime_dir(|runtime_dir| async move {
            // Clear HYPRLAND_INSTANCE_SIGNATURE so the resolver doesn't
            // honor a stale env hint from the test runner. SAFETY:
            // `with_isolated_runtime_dir` holds the env mutex for this
            // closure body; no parallel test is mutating env right now.
            let saved_his = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
            unsafe {
                env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
            }
            struct HisGuard(Option<String>);
            impl Drop for HisGuard {
                fn drop(&mut self) {
                    unsafe {
                        if let Some(v) = self.0.take() {
                            env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v);
                        }
                    }
                }
            }
            let _his_guard = HisGuard(saved_his);

            // No HIS dirs under runtime_dir yet → resolver returns
            // NoLiveInstance → connect_hyprland_socket warns and starts a
            // 500ms backoff.
            let connect_task =
                tokio::spawn(async { connect_hyprland_socket().await });

            // Sleep into the first backoff window. Anything in (0ms, 500ms)
            // works; 200ms leaves clear room before the next iteration.
            tokio::time::sleep(Duration::from_millis(200)).await;

            // Install a live mock mid-backoff. The next iteration of
            // connect_hyprland_socket will resolve to this instance and
            // succeed on UnixStream::connect to .socket2.sock.
            let mock = MockHyprlandInstance::new(
                &runtime_dir,
                "test-instance-1",
                InstancePolicy::LiveWithClients,
            )
            .await;

            // Expected wall time ≈ 500ms (rest of first backoff) plus
            // one resolve+connect (sub-100ms). 5s timeout is generous.
            let result =
                tokio::time::timeout(Duration::from_secs(5), connect_task)
                    .await
                    .expect("connect_hyprland_socket did not return within 5s")
                    .expect("connect_hyprland_socket task panicked");

            let stream = result.expect("connect_hyprland_socket returned Err");
            // Connection established against the mid-loop-installed mock.
            // The successful return type is the assertion; drop and clean up.
            drop(stream);
            drop(mock);
        })
        .await;
    }

    /// Locks in the FR-4 EOF-then-reconnect cycle: daemon connected to peer
    /// A, peer A dies (socket close → EOF on the daemon's read side), daemon
    /// reconnects, peer B is up at a new HIS, resolver picks B and connect
    /// succeeds. This is the structural substitute for the real-Hyprland
    /// `kill -9` test — it cannot run against a live Hyprland from inside a
    /// session hosted by that Hyprland (the kill would also terminate the
    /// test runner). Validates the EOF assumption against `tokio::net::
    /// UnixStream` and the resolver's per-iteration re-scan against
    /// `${runtime_dir}/hypr/`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connect_loop_recovers_after_peer_close() {
        use media_control_lib::test_helpers::{
            InstancePolicy, MockHyprlandInstance, with_isolated_runtime_dir,
        };
        use std::time::Duration;
        use tokio::io::{AsyncBufReadExt, BufReader};

        with_isolated_runtime_dir(|runtime_dir| async move {
            let saved_his = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
            unsafe {
                env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
            }
            struct HisGuard(Option<String>);
            impl Drop for HisGuard {
                fn drop(&mut self) {
                    unsafe {
                        if let Some(v) = self.0.take() {
                            env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v);
                        }
                    }
                }
            }
            let _his_guard = HisGuard(saved_his);

            // Phase 1: peer A is up. Daemon connects.
            let mock_a = MockHyprlandInstance::new(
                &runtime_dir,
                "instance-pre-kill",
                InstancePolicy::LiveWithClients,
            )
            .await;
            let stream_a =
                connect_hyprland_socket().await.expect("connect to mock A");

            // The mock's `.socket2.sock` server is accept-and-drop, so the
            // daemon's read side should see EOF on first read. This is the
            // protocol-level invariant that `run_event_session` relies on
            // (Hyprland death closes the events socket → daemon reads
            // `Ok(None)` and returns to reconnect).
            let mut lines = BufReader::new(stream_a).lines();
            let line = tokio::time::timeout(
                Duration::from_secs(2),
                lines.next_line(),
            )
            .await
            .expect("EOF should arrive within 2s")
            .expect("read errored before EOF");
            assert!(
                line.is_none(),
                "expected EOF from peer A's socket close, got line: {line:?}"
            );

            // Phase 2: simulate Hyprland restart with NEW HIS. Drop peer A
            // (listener tasks aborted; socket files linger, future connects
            // refused), bring up peer B at a different HIS path.
            drop(mock_a);
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _mock_b = MockHyprlandInstance::new(
                &runtime_dir,
                "instance-post-kill",
                InstancePolicy::LiveWithClients,
            )
            .await;

            // Phase 3: daemon's reconnect path. Resolver scans the runtime
            // dir, sees both `instance-pre-kill` (Dead — listener gone) and
            // `instance-post-kill` (LiveWithClients), picks the latter,
            // connect succeeds.
            let stream_b = tokio::time::timeout(
                Duration::from_secs(5),
                connect_hyprland_socket(),
            )
            .await
            .expect("reconnect-after-EOF did not return within 5s")
            .expect("connect_hyprland_socket returned Err post-restart");

            drop(stream_b);
        })
        .await;
    }
}
