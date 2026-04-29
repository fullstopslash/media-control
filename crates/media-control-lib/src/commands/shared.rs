//! Items shared between window and workflow command groups.
//!
//! `CommandContext` is the per-invocation handle every command receives.
//! `runtime_dir` resolves and validates `XDG_RUNTIME_DIR` for any code
//! that needs a tmpfs path. `async_env_test_mutex` serialises tests that
//! mutate process-wide environment.

use std::env;
use std::path::PathBuf;

use crate::config::Config;
use crate::error::Result;
use crate::hyprland::HyprlandClient;
use crate::window::WindowMatcher;

/// Shared context for command execution.
///
/// Holds the Hyprland client, configuration, and window matcher.
/// Commands receive this context to access shared resources.
pub struct CommandContext {
    /// Hyprland IPC client for window operations.
    pub hyprland: HyprlandClient,
    /// Loaded configuration.
    pub config: Config,
    /// Compiled window matcher from config patterns.
    pub window_matcher: WindowMatcher,
}

impl CommandContext {
    /// Create a command context for testing with a custom Hyprland client and config.
    ///
    /// This bypasses environment variable lookups and config file reading,
    /// allowing tests to provide a mock Hyprland socket and custom configuration.
    #[cfg(test)]
    pub fn for_test(hyprland: HyprlandClient, config: Config) -> Result<Self> {
        let window_matcher = WindowMatcher::new(&config.patterns);
        Ok(Self {
            hyprland,
            config,
            window_matcher,
        })
    }

    /// Create a new command context with configuration loaded from default path.
    ///
    /// `async` because [`HyprlandClient::new`] now probes for a live
    /// Hyprland instance (intent 017).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration file cannot be read or parsed
    /// - No reachable Hyprland instance is found
    /// - Any pattern regex fails to compile
    pub async fn new() -> Result<Self> {
        // `ConfigError` bridges via `#[from]` — preserves the typed source
        // chain (path, regex, range failures) instead of `Box<dyn Error>`.
        let config = Config::load()?;
        Self::with_config(config).await
    }

    /// Create a new command context with the provided configuration.
    ///
    /// `async` because [`HyprlandClient::new`] now probes for a live
    /// Hyprland instance (intent 017).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No reachable Hyprland instance is found
    /// - Any pattern regex fails to compile
    pub async fn with_config(config: Config) -> Result<Self> {
        // Use the existing `From<HyprlandError>` bridge so the typed source
        // chain (env-var name, IO error, etc.) is preserved end-to-end
        // instead of being flattened into a stringified `NotFound`.
        let hyprland = HyprlandClient::new().await?;
        let window_matcher = WindowMatcher::new(&config.patterns);

        Ok(Self {
            hyprland,
            config,
            window_matcher,
        })
    }
}

/// Get the runtime directory from `$XDG_RUNTIME_DIR`.
///
/// Sanitizes the env value to defend against path-traversal injection:
/// the path must be absolute, contain no `..` components, and exist as a
/// directory.
///
/// # Errors
///
/// Returns [`MediaControlError::InvalidArgument`] when `XDG_RUNTIME_DIR`
/// is unset, empty, relative, contains `..`, or does not point to an
/// existing directory. Falling back to `/tmp` would be world-writable and
/// would expose every derived path (suppress file, minify state) to
/// symlink attacks; we refuse to do so.
pub fn runtime_dir() -> Result<PathBuf> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static MISSING_WARNED: AtomicBool = AtomicBool::new(false);

    fn sanitize(raw: &str) -> Option<PathBuf> {
        let p = PathBuf::from(raw);
        if !p.is_absolute() {
            return None;
        }
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return None;
        }
        // Existence check defends against typo'd or hostile values.
        if !p.is_dir() {
            return None;
        }
        Some(p)
    }

    if let Some(dir) = env::var("XDG_RUNTIME_DIR").ok().and_then(|v| sanitize(&v)) {
        return Ok(dir);
    }
    if !MISSING_WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "XDG_RUNTIME_DIR is required (must be absolute, free of `..`, and an existing directory); refusing to fall back to /tmp"
        );
    }
    Err(crate::error::MediaControlError::invalid_argument(
        "XDG_RUNTIME_DIR is required: must be set to an absolute, existing directory path with no `..` components",
    ))
}

/// Test-only mutex serializing access to process-wide state used by the
/// suppress file and runtime-dir resolution: `$XDG_RUNTIME_DIR`,
/// `$HYPRLAND_INSTANCE_SIGNATURE`, and the on-disk suppress file path.
/// Single process-wide async mutex serialising ALL tests that touch shared
/// global state: `XDG_RUNTIME_DIR`, `HYPRLAND_INSTANCE_SIGNATURE`,
/// `MPV_IPC_SOCKET`, or the on-disk suppress file.
///
/// Using ONE lock domain eliminates the inter-domain race that previously
/// existed between sync env-mutation tests and async suppress-file tests.
/// All callers hold this with `let _g = async_env_test_mutex().lock().await`
/// for the full test body.
#[cfg(test)]
pub(crate) fn async_env_test_mutex() -> &'static tokio::sync::Mutex<()> {
    use std::sync::OnceLock;
    static M: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    M.get_or_init(|| tokio::sync::Mutex::new(()))
}
