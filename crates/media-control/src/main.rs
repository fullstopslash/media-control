//! Media Control CLI
//!
//! Command-line interface for managing media windows in Hyprland.
//!
//! # Usage
//!
//! ```bash
//! # Window management
//! media-control fullscreen
//! media-control move right            # or vim-style: h, j, k, l
//! media-control close
//! media-control focus --launch "..."  # focus or spawn fallback
//! media-control avoid                 # usually invoked by the daemon
//! media-control pin-and-float
//! media-control minify                # toggle smaller window mode
//!
//! # Library/store control (delegated to mpv-shim via IPC)
//! media-control mark-watched
//! media-control mark-watched-and-stop
//! media-control mark-watched-and-next
//! media-control next                  # next item via per-library strategy
//! media-control prev
//! media-control next-series           # series-level navigation
//! media-control prev-series
//! media-control keep                  # tag as keep
//! media-control favorite              # toggle favorite
//! media-control delete                # remove/unfollow/delete (store-specific)
//! media-control add-o                 # increment Stash o-counter
//!
//! # Playback / mpv IPC
//! media-control chapter next          # next or prev
//! media-control seek 50               # jump to 50% (0-100)
//! media-control play next-up          # or store name (jellyfin, twitch, …) or item id
//! media-control random show           # optional store-specific type
//! media-control status --json         # machine-readable status (waybar)
//!
//! # Tooling
//! media-control completions zsh       # bash | zsh | fish | elvish | powershell
//! ```

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use media_control_lib::{
    commands::{self, CommandContext},
    config::Config,
};

#[derive(Parser)]
#[command(name = "media-control")]
#[command(about = "Manage media windows in Hyprland")]
#[command(version)]
struct Cli {
    /// Enable verbose/debug logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Override config file path
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Toggle fullscreen for media window
    Fullscreen,

    /// Move media window to screen edge
    Move {
        /// Direction: left, right, up, down (or vim-style: h, j, k, l)
        direction: String,
    },

    /// Close media window gracefully
    Close,

    /// Focus media window, or launch if not found
    Focus {
        /// Command to launch if no media window exists (executed via sh -c)
        #[arg(long, short)]
        launch: Option<String>,
    },

    /// Trigger window avoidance (usually called by daemon)
    Avoid,

    /// Toggle pinned floating mode
    PinAndFloat,

    /// Mark current Jellyfin item as watched
    MarkWatched,

    /// Mark watched and stop playback
    MarkWatchedAndStop,

    /// Mark watched and advance queue
    MarkWatchedAndNext,

    /// Next item via per-library strategy (no mark watched)
    Next,

    /// Previous item via per-library strategy (no mark watched)
    Prev,

    /// Jump to next series/collection
    NextSeries,

    /// Return to previous series/collection
    PrevSeries,

    /// Tag current item as "keep" (prevents auto-deletion)
    Keep,

    /// Toggle favorite on current item
    Favorite,

    /// Delete current item (Jellyfin: remove, Twitch: unfollow, Stash: delete scene)
    Delete,

    /// Increment o-counter for current Stash scene
    AddO,

    /// Toggle minified mode (smaller media window)
    Minify,

    /// Navigate chapters in mpv
    Chapter {
        /// Direction: next or prev
        direction: String,
    },

    /// Seek to an absolute percentage position (0-100)
    Seek {
        /// Percentage position (0=start, 100=end)
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },

    /// Start playback via mpv-shim (next-up, store name, or item ID)
    Play {
        /// What to play: `next-up`, a store name (jellyfin, twitch, pinchflat, ...),
        /// or a 32+ character hex item ID
        target: String,
    },

    /// Pick and play a random item from the active store
    Random {
        /// Optional type filter (store-specific: e.g. show/series/movie for Jellyfin,
        /// scene/performer/studio for Stash)
        #[arg(name = "TYPE")]
        random_type: Option<String>,
    },

    /// Show current playback status
    Status {
        /// Output as JSON (for waybar/scripting)
        #[arg(long)]
        json: bool,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Handle completions early (no config needed)
    if let Commands::Completions { shell } = cli.command {
        clap_complete::generate(
            shell,
            &mut Cli::command(),
            "media-control",
            &mut std::io::stdout(),
        );
        return;
    }

    // Setup logging. Default to `warn` so things like a broken config file
    // are visible without `-v`; `-v` flips us up to `debug`. Done BEFORE
    // the status branch so `-v media-control status` is also instrumented.
    //
    // `RUST_LOG`, when set, overrides the verbose flag — gives operators a
    // direct escape hatch matching the daemon's behavior, e.g.
    // `RUST_LOG=media_control=trace,reqwest=info media-control status`.
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if cli.verbose {
            "media_control=debug"
        } else {
            "media_control=warn"
        })
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Handle status early (no config/context needed — just mpv IPC)
    if let Commands::Status { json } = cli.command {
        match commands::status::status(json).await {
            Ok(true) => return,
            Ok(false) => std::process::exit(1),
            Err(e) => {
                eprintln!("media-control: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = run(cli).await {
        eprintln!("media-control: {e}");
        notify_error(&e.to_string()).await;
        std::process::exit(1);
    }
}

/// Emit a desktop notification for a fatal error.
///
/// We `await child.wait()` (under a timeout) so the notification has
/// actually been dispatched to dbus before we exit — a bare `spawn()`
/// followed by an immediate `process::exit` can drop the notification on
/// slower systems because the parent's exit reaps the child before
/// notify-send completes its dbus call. `notify-send` typically returns
/// within a few ms, but we still cap the total wait so we never block exit
/// indefinitely if dbus is wedged.
///
/// Uses `tokio::process::Command` end-to-end: `child.kill()` and
/// `child.wait()` on the std API are blocking syscalls and would park a
/// tokio worker thread for the full timeout. The async variants integrate
/// with the runtime so other tasks keep making progress while we wait.
async fn notify_error(message: &str) {
    use std::time::Duration;
    use tokio::process::Command;

    let mut child = match Command::new("notify-send")
        .args(["-u", "critical", "media-control", message])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        // notify-send is optional — silently skip if not installed.
        Err(_) => return,
    };

    // Bounded wait: a single timeout instead of a polling loop. If we hit
    // the deadline notify-send is wedged — kill it and reap so it does not
    // linger as a zombie after we exit.
    match tokio::time::timeout(Duration::from_millis(500), child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Load config (use override path if provided). `load_or_warn` keeps
    // the CLI usable even when the user's config is broken — emits a
    // `warn!` and falls back to defaults instead of refusing to start.
    let config = Config::load_or_warn(cli.config.as_deref());

    let ctx = CommandContext::with_config(config)?;

    // Route to command
    match cli.command {
        Commands::Fullscreen => {
            commands::fullscreen::fullscreen(&ctx).await?;
        }
        Commands::Move { direction } => {
            let dir = commands::move_window::Direction::parse(&direction)
                .ok_or("Direction must be left, right, up, down (or h, j, k, l)")?;
            commands::move_window::move_window(&ctx, dir).await?;
        }
        Commands::Close => {
            commands::close::close(&ctx).await?;
        }
        Commands::Focus { launch } => {
            // `focus_or_launch` returns Ok(true) if a window was focused (or
            // launched), Ok(false) if no media window exists and no `--launch`
            // fallback was supplied. Surface the latter as a non-zero exit so
            // callers (e.g. waybar/keybinds) can chain conditionally.
            if !commands::focus::focus_or_launch(&ctx, launch.as_deref()).await? {
                std::process::exit(1);
            }
        }
        Commands::Avoid => {
            commands::avoid::avoid(&ctx).await?;
        }
        Commands::PinAndFloat => {
            commands::pin::pin_and_float(&ctx).await?;
        }
        Commands::MarkWatched => {
            commands::mark_watched::mark_watched().await?;
        }
        Commands::MarkWatchedAndStop => {
            commands::mark_watched::mark_watched_and_stop().await?;
        }
        Commands::MarkWatchedAndNext => {
            commands::mark_watched::mark_watched_and_next().await?;
        }
        Commands::Next => {
            commands::mark_watched::next().await?;
        }
        Commands::Prev => {
            commands::mark_watched::prev().await?;
        }
        Commands::NextSeries => {
            commands::mark_watched::next_series().await?;
        }
        Commands::PrevSeries => {
            commands::mark_watched::prev_series().await?;
        }
        Commands::Keep => {
            commands::keep::keep().await?;
        }
        Commands::Favorite => {
            commands::keep::favorite().await?;
        }
        Commands::Delete => {
            commands::keep::delete().await?;
        }
        Commands::AddO => {
            commands::keep::add_o().await?;
        }
        Commands::Minify => {
            commands::minify::minify(&ctx).await?;
        }
        Commands::Seek { percent } => {
            commands::seek::seek(percent).await?;
        }
        Commands::Chapter { direction } => {
            let dir = commands::chapter::ChapterDirection::parse(&direction)
                .ok_or_else(|| format!("unknown chapter direction: {direction}"))?;
            commands::chapter::chapter(dir).await?;
        }
        Commands::Play { target } => {
            // Pass the owned String so `PlayTarget::parse_owned` can move
            // the buffer into the `Store` / `ItemId` variant instead of
            // re-allocating from a borrowed slice.
            commands::play::play(target).await?;
        }
        Commands::Random { random_type } => {
            commands::random::random(random_type.as_deref()).await?;
        }
        Commands::Status { .. } => unreachable!(), // handled before config loading
        Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}
