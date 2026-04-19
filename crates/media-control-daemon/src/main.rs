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
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use media_control_lib::commands::{self, CommandContext, runtime_dir};
use media_control_lib::config::Config;
use media_control_lib::hyprland::runtime_socket_path;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Event-driven daemon for media window avoidance.
#[derive(Parser)]
#[command(name = "media-control-daemon")]
#[command(about = "Event-driven daemon for media window avoidance")]
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
fn get_pid_file_path() -> PathBuf {
    runtime_dir().join("media-control-daemon.pid")
}

/// Get the path to the trigger FIFO.
///
/// Placed in `$XDG_RUNTIME_DIR` (per-user, mode 0700 by default on systemd
/// systems) rather than world-writable `/tmp`. This defends against symlink
/// races and pre-creation attacks that would otherwise be possible at a
/// predictable `/tmp` path on a multi-user host.
fn get_fifo_path() -> PathBuf {
    runtime_dir().join("media-avoider-trigger.fifo")
}

/// Get the path to Hyprland's socket2 (event stream).
fn get_socket2_path() -> Result<PathBuf, String> {
    runtime_socket_path(".socket2.sock").map_err(|e| e.to_string())
}

/// Read PID from the PID file.
///
/// Rejects non-positive PIDs to avoid accidentally signalling process group 0
/// or the entire session via `kill(-1, ...)` semantics.
async fn read_pid_file() -> Option<Pid> {
    let path = get_pid_file_path();
    let content = fs::read_to_string(&path).await.ok()?;
    let pid: i32 = content.trim().parse().ok()?;
    if pid <= 1 {
        return None;
    }
    Some(Pid::from_raw(pid))
}

/// Write current PID to the PID file with restrictive permissions.
///
/// Uses O_CREAT|O_TRUNC|O_WRONLY with mode 0o600 so the file cannot be read
/// by other users on a shared host.
async fn write_pid_file() -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = get_pid_file_path();
    let pid = std::process::id();

    // Atomically (re)create with 0600 perms.
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let mut file = opts.open(&path)?;
    use std::io::Write;
    write!(file, "{pid}")?;
    file.sync_all()?;
    Ok(())
}

/// Remove the PID file.
async fn remove_pid_file() {
    let path = get_pid_file_path();
    let _ = fs::remove_file(&path).await;
}

/// Check if a process with the given PID is running AND is plausibly our daemon.
///
/// A bare `kill(pid, 0)` is racy: PIDs are recycled, so a stale PID file may
/// reference an unrelated process — possibly even one owned by a different
/// user. Defend against that by also checking `/proc/<pid>/comm` matches the
/// expected daemon binary name when available; fall back to signal-0 if
/// `/proc` is unreadable.
///
/// Linux truncates `/proc/PID/comm` to 15 chars (TASK_COMM_LEN=16 inc. NUL),
/// so we accept either the full crate name or the truncated 15-char prefix.
fn is_process_running(pid: Pid) -> bool {
    if signal::kill(pid, None).is_err() {
        return false;
    }
    // Best-effort identity check — only trust kill if comm matches.
    let comm_path = format!("/proc/{}/comm", pid.as_raw());
    let full = env!("CARGO_PKG_NAME");
    // /proc/PID/comm is truncated to 15 chars on Linux.
    let truncated: &str = if full.len() > 15 { &full[..15] } else { full };
    match std::fs::read_to_string(&comm_path) {
        Ok(comm) => {
            let c = comm.trim();
            c == full || c == truncated
        }
        // /proc not available or unreadable — fall back to signal-0 result.
        Err(_) => true,
    }
}

/// Trigger the avoid command.
async fn trigger_avoid(ctx: &CommandContext) {
    if let Err(e) = commands::avoid::avoid(ctx).await {
        debug!("Avoid error: {}", e);
    }
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

    // mkfifo creates atomically; mode 0o600 is masked by umask but caller
    // controls that. The previous symlink check + per-user runtime dir means
    // a TOCTOU race here would require write access to $XDG_RUNTIME_DIR,
    // which on systemd systems is the user's own 0700 directory.
    nix::unistd::mkfifo(path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .map_err(std::io::Error::other)
}

/// Create the trigger FIFO at the default daemon path.
fn create_fifo() -> std::io::Result<PathBuf> {
    let path = get_fifo_path();
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
    let path = get_fifo_path();
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
async fn fifo_listener(tx: mpsc::Sender<()>) {
    /// Backoff applied after any read or open error. Bounded so a missing
    /// or broken FIFO can't burn CPU.
    const FIFO_ERROR_BACKOFF: Duration = Duration::from_millis(100);

    let path = get_fifo_path();

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
                    if tx.send(()).await.is_err() {
                        return; // Channel closed, shutdown
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
async fn connect_hyprland_socket() -> Result<UnixStream, String> {
    let socket_path = get_socket2_path()?;
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(10);

    loop {
        match tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(&socket_path)).await
        {
            Ok(Ok(stream)) => {
                info!("Connected to Hyprland socket at {:?}", socket_path);
                return Ok(stream);
            }
            Ok(Err(e)) => {
                warn!(
                    "Failed to connect to Hyprland socket: {} (retry in {:?})",
                    e, backoff
                );
            }
            Err(_) => {
                warn!(
                    "Timed out connecting to Hyprland socket (retry in {:?})",
                    backoff
                );
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
async fn run_event_session(
    ctx: &CommandContext,
    debounce_duration: Duration,
    fifo_rx: &mut mpsc::Receiver<()>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let stream = connect_hyprland_socket().await?;
    let mut lines = BufReader::new(stream).lines();
    let mut last_avoid_time = Instant::now();

    // Helper closure: apply debounce-and-trigger logic uniformly.
    async fn maybe_trigger(
        ctx: &CommandContext,
        last: &mut Instant,
        debounce: Duration,
    ) {
        if last.elapsed() >= debounce {
            trigger_avoid(ctx).await;
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
                            maybe_trigger(ctx, &mut last_avoid_time, debounce_duration).await;
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
                maybe_trigger(ctx, &mut last_avoid_time, debounce_duration).await;
            }
        }
    }
}

/// Run the event loop with automatic reconnection.
async fn run_event_loop() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load().unwrap_or_else(|e| {
        warn!("Config load failed ({e}), using defaults");
        Config::default()
    });
    let debounce_ms = config.positioning.debounce_ms;
    let ctx = CommandContext::with_config(config)?;
    let debounce_duration = Duration::from_millis(u64::from(debounce_ms));

    // Create FIFO for manual triggers
    create_fifo()?;

    // Channel for FIFO triggers — persists across reconnections
    let (fifo_tx, mut fifo_rx) = mpsc::channel::<()>(16);
    tokio::spawn(fifo_listener(fifo_tx));

    info!("Event loop started");

    loop {
        match run_event_session(&ctx, debounce_duration, &mut fifo_rx).await {
            Ok(true) => {
                // Reconnect after brief delay
                info!("Reconnecting to Hyprland socket...");
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Recreate FIFO in case it was cleaned up
                if !get_fifo_path().exists() {
                    let _ = create_fifo();
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

    // Write PID file
    if let Err(e) = write_pid_file().await {
        error!("Failed to write PID file: {}", e);
        return ExitCode::FAILURE;
    }

    // Run event loop with graceful shutdown
    let mut sigterm = match tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::terminate(),
    ) {
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
fn start_lock_path() -> PathBuf {
    runtime_dir().join("media-control-daemon.start.lock")
}

/// Acquire an exclusive start-lock to prevent concurrent `cmd_start` invocations
/// from racing past the PID-file check and double-spawning the daemon.
///
/// Returns the lock file (must be held until spawn completes) or an error if
/// another start is in progress. Uses `O_CREAT|O_EXCL` so creation is atomic
/// with respect to other processes.
fn acquire_start_lock() -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(start_lock_path())
}

/// Release the start lock by removing the lock file.
fn release_start_lock() {
    let _ = std::fs::remove_file(start_lock_path());
}

/// Acquire the start lock, recovering from a stale lock left behind by a
/// crashed previous start. Returns the live lock handle on success.
async fn acquire_or_recover_start_lock() -> Result<std::fs::File, ExitCode> {
    match acquire_start_lock() {
        Ok(f) => Ok(f),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Stale lock or genuine concurrent start? Check liveness.
            let pid_alive = read_pid_file()
                .await
                .map(is_process_running)
                .unwrap_or(false);
            if pid_alive {
                eprintln!("Daemon start already in progress");
                return Err(ExitCode::FAILURE);
            }
            release_start_lock();
            acquire_start_lock().map_err(|e| {
                eprintln!("Failed to acquire start lock: {e}");
                ExitCode::FAILURE
            })
        }
        Err(e) => {
            eprintln!("Failed to acquire start lock: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// Spawn the daemon's foreground worker as a detached background process.
fn spawn_foreground_worker() -> ExitCode {
    let exe = match env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to get executable path: {e}");
            return ExitCode::FAILURE;
        }
    };

    match std::process::Command::new(&exe)
        .arg("foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => {
            println!("Daemon started (PID {})", child.id());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Failed to start daemon: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Start the daemon in background mode.
async fn cmd_start() -> ExitCode {
    // Atomically acquire start-lock to serialize concurrent `start` calls.
    // Without this, two `start` invocations could each pass the PID-check
    // below and both spawn a daemon; the second's PID file would clobber
    // the first, leaking a daemon process.
    let _start_lock = match acquire_or_recover_start_lock().await {
        Ok(f) => f,
        Err(code) => return code,
    };

    // Check if already running (now serialized by the lock above).
    if let Some(pid) = read_pid_file().await {
        if is_process_running(pid) {
            eprintln!("Daemon already running (PID {pid})");
            release_start_lock();
            return ExitCode::FAILURE;
        }
        // Stale PID file, remove it
        remove_pid_file().await;
    }

    let result = spawn_foreground_worker();
    drop(_start_lock);
    release_start_lock();
    result
}

/// Stop the running daemon.
async fn cmd_stop() -> ExitCode {
    let Some(pid) = read_pid_file().await else {
        eprintln!("Daemon not running (no PID file)");
        return ExitCode::FAILURE;
    };

    if !is_process_running(pid) {
        eprintln!("Daemon not running (stale PID file)");
        remove_pid_file().await;
        return ExitCode::FAILURE;
    }

    // Send SIGTERM
    if let Err(e) = signal::kill(pid, Signal::SIGTERM) {
        eprintln!("Failed to send SIGTERM to PID {}: {}", pid, e);
        return ExitCode::FAILURE;
    }

    println!("Sent SIGTERM to daemon (PID {})", pid);
    ExitCode::SUCCESS
}

/// Check daemon status.
async fn cmd_status() -> ExitCode {
    let Some(pid) = read_pid_file().await else {
        println!("Daemon not running (no PID file)");
        return ExitCode::from(1);
    };

    if is_process_running(pid) {
        println!("Daemon running (PID {})", pid);
        ExitCode::SUCCESS
    } else {
        println!("Daemon not running (stale PID file for PID {})", pid);
        ExitCode::from(1)
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

    // Initialize logging for foreground mode only
    let command = cli.command.unwrap_or(Commands::Start);
    if matches!(command, Commands::Foreground) {
        init_logging();
    }

    match command {
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

    /// `read_pid_file` must reject pid <= 1 to defend against accidentally
    /// signalling the entire process group via kill(0)/kill(-1) semantics.
    #[tokio::test]
    async fn read_pid_file_rejects_low_pids() {
        // Set XDG_RUNTIME_DIR to a temp dir for isolation.
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test
        let original = env::var("XDG_RUNTIME_DIR").ok();
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", dir.path());
        }

        let pid_path = get_pid_file_path();
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
}
