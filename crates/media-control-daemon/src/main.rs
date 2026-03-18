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
use media_control_lib::commands::{self, CommandContext};
use media_control_lib::config::Config;
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

/// Get the path to the PID file.
fn get_pid_file_path() -> PathBuf {
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("media-control-daemon.pid")
}

/// Get the path to the trigger FIFO.
///
/// This FIFO allows manual triggering of the avoider for events that
/// don't emit Hyprland socket events (like `layoutmsg togglesplit`).
/// Located in /tmp to avoid leaving files on disk (tmpfs).
fn get_fifo_path() -> PathBuf {
    PathBuf::from("/tmp/media-avoider-trigger.fifo")
}

/// Get the path to Hyprland's socket2.
fn get_socket2_path() -> Result<PathBuf, String> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR not set")?;
    let instance_sig =
        env::var("HYPRLAND_INSTANCE_SIGNATURE").map_err(|_| "HYPRLAND_INSTANCE_SIGNATURE not set")?;

    Ok(PathBuf::from(runtime_dir)
        .join("hypr")
        .join(instance_sig)
        .join(".socket2.sock"))
}

/// Read PID from the PID file.
async fn read_pid_file() -> Option<Pid> {
    let path = get_pid_file_path();
    let content = fs::read_to_string(&path).await.ok()?;
    let pid: i32 = content.trim().parse().ok()?;
    Some(Pid::from_raw(pid))
}

/// Write current PID to the PID file.
async fn write_pid_file() -> std::io::Result<()> {
    let path = get_pid_file_path();
    let pid = std::process::id();
    fs::write(&path, pid.to_string()).await
}

/// Remove the PID file.
async fn remove_pid_file() {
    let path = get_pid_file_path();
    let _ = fs::remove_file(&path).await;
}

/// Check if a process with the given PID is running.
fn is_process_running(pid: Pid) -> bool {
    // Sending signal 0 checks if process exists without actually signaling it
    signal::kill(pid, None).is_ok()
}

/// Trigger the avoid command.
async fn trigger_avoid(ctx: &CommandContext) {
    if let Err(e) = commands::avoid::avoid(ctx).await {
        debug!("Avoid error: {}", e);
    }
}

/// Create the trigger FIFO if it doesn't exist.
fn create_fifo() -> std::io::Result<PathBuf> {
    let path = get_fifo_path();

    // Remove stale FIFO if it exists
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    // Create new FIFO using nix
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

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

/// Run the event loop, listening to Hyprland socket2 events and FIFO triggers.
async fn run_event_loop() -> Result<(), Box<dyn std::error::Error>> {
    // Load config and create context once (reused for all avoid triggers)
    let config = Config::load().unwrap_or_default();
    let debounce_ms = config.positioning.debounce_ms;
    let ctx = CommandContext::with_config(config.clone())?;

    let socket_path = get_socket2_path()?;
    info!("Connecting to Hyprland socket at {:?}", socket_path);

    let stream = UnixStream::connect(&socket_path).await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    // Create FIFO for manual triggers
    create_fifo()?;

    // Channel for FIFO triggers
    let (fifo_tx, mut fifo_rx) = mpsc::channel::<()>(16);

    // Spawn FIFO listener task
    tokio::spawn(fifo_listener(fifo_tx));

    // Debounce state
    let mut last_avoid_time = Instant::now();
    let debounce_duration = Duration::from_millis(u64::from(debounce_ms));

    info!("Event loop started, listening for Hyprland events and FIFO triggers");

    loop {
        tokio::select! {
            // Handle Hyprland socket events
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        // Parse event: "eventname>>data"
                        let (event, _data) = line.split_once(">>").unwrap_or((&line, ""));

                        // Events that trigger avoidance
                        match event {
                            "workspace" | "activewindow" | "movewindow" | "openwindow" | "closewindow" | "swapwindow" => {
                                if last_avoid_time.elapsed() >= debounce_duration {
                                    trigger_avoid(&ctx).await;
                                    last_avoid_time = Instant::now();
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => {
                        info!("Hyprland socket closed");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from Hyprland socket: {}", e);
                        break;
                    }
                }
            }

            // Handle FIFO triggers (for togglesplit etc.)
            Some(()) = fifo_rx.recv() => {
                if last_avoid_time.elapsed() >= debounce_duration {
                    debug!("Processing FIFO trigger");
                    trigger_avoid(&ctx).await;
                    last_avoid_time = Instant::now();
                }
            }
        }
    }

    info!("Event loop ended");
    Ok(())
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
    let result = tokio::select! {
        result = run_event_loop() => result,
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down");
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
