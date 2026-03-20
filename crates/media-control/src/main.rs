//! Media Control CLI
//!
//! Command-line interface for managing media windows in Hyprland.
//!
//! # Usage
//!
//! ```bash
//! # Toggle fullscreen
//! media-control fullscreen
//!
//! # Move window (use direction names or vim-style keys)
//! media-control move right
//! media-control move l  # vim-style: h=left, j=down, k=up, l=right
//!
//! # Close media window
//! media-control close
//!
//! # Toggle pin-and-float mode
//! media-control pin-and-float
//!
//! # Jellyfin integration
//! media-control mark-watched
//! media-control mark-watched-and-stop
//! media-control mark-watched-and-next
//!
//! # Chapter navigation
//! media-control chapter next
//! media-control chapter prev
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

    /// Skip to next item via strategy (no mark watched)
    SkipNext,

    /// Skip to previous item (no mark watched)
    SkipPrev,

    /// Navigate chapters in mpv
    Chapter {
        /// Direction: next or prev
        direction: String,
    },

    /// Play a Jellyfin item (next-up, recent-pinchflat, or item ID)
    Play {
        /// What to play: next-up, recent-pinchflat, or a Jellyfin item ID
        target: String,
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

    // Setup logging (off by default, enabled with -v)
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("media_control=debug")
            .init();
    }

    if let Err(e) = run(cli).await {
        eprintln!("media-control: {e}");
        // Fire-and-forget desktop notification
        let _ = std::process::Command::new("notify-send")
            .args(["-u", "critical", "media-control", &format!("{e}")])
            .spawn();
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Load config (use override path if provided)
    let config = match &cli.config {
        Some(path) => Config::load_from_path(path)?,
        None => Config::load().unwrap_or_else(|e| {
            tracing::debug!("Config load failed ({e}), using defaults");
            Config::default()
        }),
    };

    let ctx = CommandContext::with_config(config)?;

    // Route to command
    match cli.command {
        Commands::Fullscreen => {
            commands::fullscreen::fullscreen(&ctx).await?;
        }
        Commands::Move { direction } => {
            let dir = commands::move_window::Direction::from_str(&direction)
                .ok_or_else(|| "Direction must be left, right, up, down (or h, j, k, l)")?;
            commands::move_window::move_window(&ctx, dir).await?;
        }
        Commands::Close => {
            commands::close::close(&ctx).await?;
        }
        Commands::Focus { launch } => {
            commands::focus::focus_or_launch(&ctx, launch.as_deref()).await?;
        }
        Commands::Avoid => {
            commands::avoid::avoid(&ctx).await?;
        }
        Commands::PinAndFloat => {
            commands::pin::pin_and_float(&ctx).await?;
        }
        Commands::MarkWatched => {
            commands::mark_watched::mark_watched(&ctx).await?;
        }
        Commands::MarkWatchedAndStop => {
            commands::mark_watched::mark_watched_and_stop(&ctx).await?;
        }
        Commands::MarkWatchedAndNext => {
            commands::mark_watched::mark_watched_and_next(&ctx).await?;
        }
        Commands::SkipNext => {
            commands::mark_watched::skip_next(&ctx).await?;
        }
        Commands::SkipPrev => {
            commands::mark_watched::skip_prev(&ctx).await?;
        }
        Commands::Chapter { direction } => {
            let dir = match direction.as_str() {
                "next" => commands::chapter::ChapterDirection::Next,
                "prev" => commands::chapter::ChapterDirection::Prev,
                _ => return Err("Direction must be 'next' or 'prev'".into()),
            };
            commands::chapter::chapter(&ctx, dir).await?;
        }
        Commands::Play { target } => {
            commands::play::play(&ctx, &target).await?;
        }
        Commands::Status { .. } => unreachable!(), // handled before config loading
        Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}
