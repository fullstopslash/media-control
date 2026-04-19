//! Media Control Daemon
//!
//! Event-driven daemon that listens to Hyprland socket events
//! and triggers media window avoidance.
//!
//! Also supports manual triggers via FIFO for events that don't
//! emit Hyprland socket events (like `layoutmsg togglesplit`).

use std::env;
use std::path::PathBuf;
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
fn is_process_running(pid: Pid) -> bool {
    if signal::kill(pid, None).is_err() {
        return false;
    }
    // Best-effort identity check — only trust kill if comm matches.
    let comm_path = format!("/proc/{}/comm", pid.as_raw());
    match std::fs::read_to_string(&comm_path) {
        Ok(comm) => comm.trim() == env!("CARGO_PKG_NAME") || comm.trim() == "media-control-d",
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

/// Create the trigger FIFO if it doesn't exist.
///
/// Hardened against symlink attacks: uses `lstat` (no symlink resolution) to
/// check the existing entry, refuses to remove anything that isn't a real
/// FIFO owned by us, and creates the new FIFO with mode 0o600.
fn create_fifo() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let path = get_fifo_path();

    // Use symlink_metadata (lstat) — does NOT follow symlinks. If an attacker
    // pre-created a symlink at our path we'd otherwise remove the target.
    match std::fs::symlink_metadata(&path) {
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
            std::fs::remove_file(&path)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    // mkfifo creates atomically; mode 0o600 is masked by umask but caller
    // controls that. The previous symlink check + per-user runtime dir means
    // a TOCTOU race here would require write access to $XDG_RUNTIME_DIR,
    // which on systemd systems is the user's own 0700 directory.
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .map_err(std::io::Error::other)?;

    info!("Created trigger FIFO at {:?}", path);
    Ok(path)
}

/// Remove the trigger FIFO.
fn remove_fifo() {
    let path = get_fifo_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

/// Listen for triggers on the FIFO and send them to the channel.
///
/// The FIFO is opened in a loop because each write closes it on the writer side.
async fn fifo_listener(tx: mpsc::Sender<()>) {
    let path = get_fifo_path();

    loop {
        // Open FIFO for reading (blocks until a writer connects)
        let file = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to open FIFO: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Read any data (we don't care about content, just the trigger)
        while let Ok(Some(_)) = lines.next_line().await {
            debug!("Received FIFO trigger");
            if tx.send(()).await.is_err() {
                return; // Channel closed, shutdown
            }
        }

        // Writer closed, loop to reopen
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

/// Run a single session of the event loop (until socket disconnect).
/// Returns Ok(true) to signal reconnect, Ok(false) for clean shutdown.
async fn run_event_session(
    ctx: &CommandContext,
    debounce_duration: Duration,
    fifo_rx: &mut mpsc::Receiver<()>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let stream = connect_hyprland_socket().await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    let mut last_avoid_time = Instant::now();

    info!("Event session started");

    loop {
        tokio::select! {
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let (event, _data) = line.split_once(">>").unwrap_or((&line, ""));

                        match event {
                            "workspace" | "activewindow" | "movewindow" | "openwindow" | "closewindow" | "swapwindow" => {
                                if last_avoid_time.elapsed() >= debounce_duration {
                                    trigger_avoid(ctx).await;
                                    last_avoid_time = Instant::now();
                                }
                            }
                            _ => {}
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
                if last_avoid_time.elapsed() >= debounce_duration {
                    debug!("Processing FIFO trigger");
                    trigger_avoid(ctx).await;
                    last_avoid_time = Instant::now();
                }
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

/// Start the daemon in background mode.
async fn cmd_start() -> ExitCode {
    // Check if already running
    if let Some(pid) = read_pid_file().await {
        if is_process_running(pid) {
            eprintln!("Daemon already running (PID {})", pid);
            return ExitCode::FAILURE;
        }
        // Stale PID file, remove it
        remove_pid_file().await;
    }

    // Get the current executable path
    let exe = match env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to get executable path: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Spawn self with "foreground" command
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
            eprintln!("Failed to start daemon: {}", e);
            ExitCode::FAILURE
        }
    }
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
