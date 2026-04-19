//! Window types and pattern matching for media windows.
//!
//! This module provides pattern matching logic to identify media windows
//! (mpv, Picture-in-Picture, Jellyfin Media Player) from Hyprland clients.
//!
//! # Example
//!
//! ```no_run
//! use media_control_lib::config::Pattern;
//! use media_control_lib::hyprland::Client;
//! use media_control_lib::window::WindowMatcher;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let patterns = vec![
//!     Pattern { key: "class".into(), value: "mpv".into(), ..Default::default() },
//!     Pattern { key: "title".into(), value: "Picture-in-Picture".into(), always_pin: true, ..Default::default() },
//! ];
//!
//! let matcher = WindowMatcher::new(&patterns);
//! // Use matcher.find_media_window() with clients from HyprlandClient
//! # Ok(())
//! # }
//! ```

use regex::{Regex, RegexBuilder};

use crate::config::Pattern;
use crate::error::{MediaControlError, Result};
use crate::hyprland::Client;

/// Maximum compiled-NFA size (bytes) for user-supplied class/title regex.
///
/// Bounds catastrophic-backtracking surface area for the daemon hot path —
/// `WindowMatcher::matches` runs on every focus event for every client.
/// Mirrors the cap used in `config.rs` for override title regexes.
///
/// Also used by `config.rs` validation so that a pattern that would exceed
/// this limit at runtime is rejected at load time rather than silently failing.
pub(crate) const PATTERN_REGEX_SIZE_LIMIT: usize = 64 * 1024;

/// Compile a user-supplied pattern regex with the size cap applied.
///
/// Note: pattern regex is intentionally **unanchored** (consistent with the
/// original bash `[[ =~ ]]` semantics) — `is_match` returns true on any
/// substring match. Callers who need exact matching must anchor with `^...$`
/// in the configured pattern value.
fn compile_pattern_regex(pattern: &str) -> std::result::Result<Regex, regex::Error> {
    RegexBuilder::new(pattern)
        .size_limit(PATTERN_REGEX_SIZE_LIMIT)
        .build()
}

/// Property key for pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKey {
    /// Match against window class.
    Class,
    /// Match against window title.
    Title,
}

impl PatternKey {
    /// Parse pattern key from string.
    fn from_str(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("class") {
            Some(Self::Class)
        } else if s.eq_ignore_ascii_case("title") {
            Some(Self::Title)
        } else {
            None
        }
    }

    /// Get the value to match from a client.
    fn get_value<'a>(&self, client: &'a Client) -> &'a str {
        match self {
            Self::Class => &client.class,
            Self::Title => &client.title,
        }
    }
}

/// A compiled pattern with regex for efficient matching.
#[derive(Debug)]
pub struct CompiledPattern {
    /// Which property to match.
    pub key: PatternKey,
    /// Compiled regex pattern.
    pub regex: Regex,
    /// Only match pinned or fullscreen windows.
    pub pinned_only: bool,
    /// Automatically pin windows matching this pattern.
    pub always_pin: bool,
}

impl CompiledPattern {
    /// Check if a client matches this pattern.
    fn matches(&self, client: &Client) -> bool {
        // pinned_only: require pinned OR fullscreen
        if self.pinned_only && !client.pinned && client.fullscreen == 0 {
            return false;
        }

        let value = self.key.get_value(client);
        self.regex.is_match(value)
    }
}

/// Result of pattern matching.
#[derive(Debug, Clone, Copy)]
pub struct MatchResult {
    /// Index of the pattern that matched.
    pub pattern_index: usize,
    /// Whether to always pin this window.
    pub always_pin: bool,
}

/// Pinned media window — highest match priority.
pub const PRIORITY_PINNED: u8 = 1;
/// Focused media window — second priority.
pub const PRIORITY_FOCUSED: u8 = 2;
/// Any matching media window — lowest priority.
pub const PRIORITY_ANY: u8 = 3;

/// A media window with all relevant metadata.
#[derive(Debug, Clone)]
pub struct MediaWindow {
    /// Hyprland window address (e.g., "0x55a1b2c3d4e5").
    pub address: String,
    /// Window class.
    pub class: String,
    /// Window title.
    pub title: String,
    /// X position in pixels.
    pub x: i32,
    /// Y position in pixels.
    pub y: i32,
    /// Window width in pixels.
    pub width: i32,
    /// Window height in pixels.
    pub height: i32,
    /// Whether the window is pinned.
    pub pinned: bool,
    /// Whether the window is floating.
    pub floating: bool,
    /// Fullscreen state (0=none, 1=maximized, 2=fullscreen).
    pub fullscreen: u8,
    /// Monitor ID.
    pub monitor: i32,
    /// Workspace ID.
    pub workspace_id: i32,
    /// From matching pattern's always_pin field.
    pub always_pin: bool,
    /// Match priority — see [`PRIORITY_PINNED`], [`PRIORITY_FOCUSED`], [`PRIORITY_ANY`].
    pub priority: u8,
    /// Focus history ID from Hyprland (lower = more recently focused).
    pub focus_history_id: i32,
    /// Process ID of the window.
    pub pid: i32,
}

impl MediaWindow {
    /// Create a MediaWindow from a Client and match result.
    fn from_client(client: &Client, match_result: &MatchResult, priority: u8) -> Self {
        Self {
            address: client.address.clone(),
            class: client.class.clone(),
            title: client.title.clone(),
            x: client.at[0],
            y: client.at[1],
            width: client.size[0],
            height: client.size[1],
            pinned: client.pinned,
            floating: client.floating,
            fullscreen: client.fullscreen,
            monitor: client.monitor,
            workspace_id: client.workspace.id,
            always_pin: match_result.always_pin,
            priority,
            pid: client.pid,
            focus_history_id: client.focus_history_id,
        }
    }
}

/// Pattern matching engine for media windows.
///
/// Compiles regex patterns from config and provides efficient matching
/// against Hyprland clients.
#[derive(Debug)]
pub struct WindowMatcher {
    /// Compiled patterns in order of definition.
    patterns: Vec<CompiledPattern>,
}

impl WindowMatcher {
    /// Build a `CompiledPattern` from a config `Pattern`, given a precompiled regex.
    #[inline]
    fn build(p: &Pattern, key: PatternKey, regex: Regex) -> CompiledPattern {
        CompiledPattern {
            key,
            regex,
            pinned_only: p.pinned_only,
            always_pin: p.always_pin,
        }
    }

    /// Create a new matcher from config patterns.
    ///
    /// Invalid patterns are logged and skipped — the matcher is best-effort.
    /// Use [`Self::new_strict`] when you want hard failure on invalid regex.
    ///
    /// Each pattern regex is compiled with a 64 KiB NFA size cap to bound
    /// catastrophic-backtracking surface area in the daemon hot path. A
    /// pathological pattern that exceeds the cap is logged and skipped.
    pub fn new(patterns: &[Pattern]) -> Self {
        let compiled = patterns
            .iter()
            .filter_map(|p| {
                let key = PatternKey::from_str(&p.key).or_else(|| {
                    eprintln!("media-control: unknown pattern key {:?}, skipping", p.key);
                    None
                })?;
                let regex = compile_pattern_regex(&p.value)
                    .map_err(|e| {
                        eprintln!("media-control: invalid regex {:?}: {e}, skipping", p.value);
                    })
                    .ok()?;
                Some(Self::build(p, key, regex))
            })
            .collect();

        Self { patterns: compiled }
    }

    /// Create a matcher that validates all patterns, returning error on first
    /// invalid regex. Same NFA size cap as [`Self::new`].
    pub fn new_strict(patterns: &[Pattern]) -> Result<Self> {
        let mut compiled = Vec::with_capacity(patterns.len());

        for p in patterns {
            let Some(key) = PatternKey::from_str(&p.key) else {
                continue; // Skip unknown keys
            };
            let regex = compile_pattern_regex(&p.value).map_err(MediaControlError::from)?;
            compiled.push(Self::build(p, key, regex));
        }

        Ok(Self { patterns: compiled })
    }

    /// Check if a client matches any pattern.
    ///
    /// Returns the first matching pattern's result, or None if no match.
    pub fn matches(&self, client: &Client) -> Option<MatchResult> {
        for (idx, pattern) in self.patterns.iter().enumerate() {
            if pattern.matches(client) {
                return Some(MatchResult {
                    pattern_index: idx,
                    always_pin: pattern.always_pin,
                });
            }
        }
        None
    }

    /// Find the best media window using priority logic.
    ///
    /// Priority order:
    /// 1. Pinned window matching any pattern
    /// 2. Focused window matching any pattern (if focus_addr provided)
    /// 3. Any window matching any pattern
    ///
    /// Returns the highest priority match, or None if no media window found.
    pub fn find_media_window(
        &self,
        clients: &[Client],
        focus_addr: Option<&str>,
    ) -> Option<MediaWindow> {
        let mut pinned: Option<(&Client, MatchResult)> = None;
        let mut focused: Option<(&Client, MatchResult)> = None;
        let mut any: Option<(&Client, MatchResult)> = None;

        for client in clients.iter().filter(|c| c.mapped && !c.hidden) {
            let Some(match_result) = self.matches(client) else { continue };

            let slot = if client.pinned {
                &mut pinned
            } else if focus_addr.is_some_and(|addr| client.address == addr) {
                &mut focused
            } else {
                &mut any
            };

            if slot.is_none() {
                *slot = Some((client, match_result));
            }
        }

        // Highest priority wins.
        pinned
            .map(|(c, m)| (c, m, PRIORITY_PINNED))
            .or_else(|| focused.map(|(c, m)| (c, m, PRIORITY_FOCUSED)))
            .or_else(|| any.map(|(c, m)| (c, m, PRIORITY_ANY)))
            .map(|(c, m, p)| MediaWindow::from_client(c, &m, p))
    }

    /// Find all media windows on a specific monitor.
    ///
    /// Returns windows sorted by priority (pinned first, then by focus history).
    pub fn find_media_windows(&self, clients: &[Client], monitor: i32) -> Vec<MediaWindow> {
        let mut windows: Vec<_> = clients
            .iter()
            .filter(|c| c.monitor == monitor)
            .filter(|c| c.mapped && !c.hidden)
            .filter_map(|client| {
                let match_result = self.matches(client)?;
                let priority = if client.pinned {
                    PRIORITY_PINNED
                } else {
                    PRIORITY_ANY
                };
                Some(MediaWindow::from_client(client, &match_result, priority))
            })
            .collect();

        // Sort by priority, then by focus history (lower ID = more recent)
        // focus_history_id -1 means never focused; sort those last
        windows.sort_by(|a, b| {
            a.priority.cmp(&b.priority).then_with(|| {
                match (a.focus_history_id < 0, b.focus_history_id < 0) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => a.focus_history_id.cmp(&b.focus_history_id),
                }
            })
        });

        windows
    }

    /// Find a window to restore focus to after media window operations.
    ///
    /// Returns the address of the most recently focused window that:
    /// - Is on the same workspace as the media window (or specified workspace)
    /// - Is not the media window itself
    /// - Has the lowest focus_history_id (most recently focused)
    pub fn find_previous_focus(
        &self,
        clients: &[Client],
        media_addr: &str,
        workspace: Option<i32>,
    ) -> Option<String> {
        // Find the media window's workspace if not specified
        let target_workspace = workspace.or_else(|| {
            clients
                .iter()
                .find(|c| c.address == media_addr)
                .map(|c| c.workspace.id)
        })?;

        // Find candidates on the same workspace, excluding the media window
        // and windows that were never focused (focus_history_id < 0)
        clients
            .iter()
            .filter(|c| c.workspace.id == target_workspace)
            .filter(|c| c.address != media_addr)
            .filter(|c| c.mapped && !c.hidden)
            .filter(|c| c.focus_history_id >= 0)
            .min_by_key(|c| c.focus_history_id)
            .map(|c| c.address.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyprland::Workspace;

    fn make_client(
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
            class: class.to_string(),
            title: title.to_string(),
            focus_history_id: 0,
            pid: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_client_full(
        address: &str,
        class: &str,
        title: &str,
        pinned: bool,
        floating: bool,
        fullscreen: u8,
        workspace_id: i32,
        monitor: i32,
        focus_history_id: i32,
    ) -> Client {
        Client {
            address: address.to_string(),
            mapped: true,
            hidden: false,
            at: [100, 100],
            size: [640, 360],
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

    // Pattern matching tests

    #[test]
    fn matches_class_pattern() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mpv = make_client("0x1", "mpv", "video.mp4", false, true);
        let firefox = make_client("0x2", "firefox", "Mozilla Firefox", false, false);

        assert!(matcher.matches(&mpv).is_some());
        assert!(matcher.matches(&firefox).is_none());
    }

    #[test]
    fn matches_title_pattern() {
        let patterns = vec![Pattern {
            key: "title".to_string(),
            value: "Picture-in-Picture".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let pip = make_client("0x1", "firefox", "Picture-in-Picture", true, true);
        let normal = make_client("0x2", "firefox", "Mozilla Firefox", false, false);

        assert!(matcher.matches(&pip).is_some());
        assert!(matcher.matches(&normal).is_none());
    }

    #[test]
    fn matches_regex_pattern() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: r"mpv|vlc".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mpv = make_client("0x1", "mpv", "video.mp4", false, true);
        let vlc = make_client("0x2", "vlc", "movie.mkv", false, true);
        let firefox = make_client("0x3", "firefox", "Mozilla Firefox", false, false);

        assert!(matcher.matches(&mpv).is_some());
        assert!(matcher.matches(&vlc).is_some());
        assert!(matcher.matches(&firefox).is_none());
    }

    #[test]
    fn matches_partial_regex() {
        let patterns = vec![Pattern {
            key: "title".to_string(),
            value: r"Picture".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let pip = make_client("0x1", "firefox", "Picture-in-Picture", true, true);
        assert!(matcher.matches(&pip).is_some());
    }

    #[test]
    fn pinned_only_requires_pinned_or_fullscreen() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "jellyfin".to_string(),
            pinned_only: true,
            always_pin: false,
        }];
        let matcher = WindowMatcher::new(&patterns);

        // Not pinned, not fullscreen - should not match
        let unpinned = make_client("0x1", "jellyfin", "Jellyfin", false, false);
        assert!(matcher.matches(&unpinned).is_none());

        // Pinned - should match
        let pinned = make_client("0x2", "jellyfin", "Jellyfin", true, true);
        assert!(matcher.matches(&pinned).is_some());

        // Fullscreen (not pinned) - should match
        let fullscreen = make_client_full("0x3", "jellyfin", "Jellyfin", false, false, 2, 1, 0, 0);
        assert!(matcher.matches(&fullscreen).is_some());
    }

    #[test]
    fn always_pin_propagates() {
        let patterns = vec![Pattern {
            key: "title".to_string(),
            value: "Picture-in-Picture".to_string(),
            pinned_only: false,
            always_pin: true,
        }];
        let matcher = WindowMatcher::new(&patterns);

        let pip = make_client("0x1", "firefox", "Picture-in-Picture", false, true);
        let result = matcher.matches(&pip).unwrap();

        assert!(result.always_pin);
    }

    #[test]
    fn pattern_index_is_correct() {
        let patterns = vec![
            Pattern {
                key: "class".to_string(),
                value: "mpv".to_string(),
                ..Default::default()
            },
            Pattern {
                key: "title".to_string(),
                value: "Picture-in-Picture".to_string(),
                ..Default::default()
            },
        ];
        let matcher = WindowMatcher::new(&patterns);

        let mpv = make_client("0x1", "mpv", "video.mp4", false, true);
        let pip = make_client("0x2", "firefox", "Picture-in-Picture", true, true);

        assert_eq!(matcher.matches(&mpv).unwrap().pattern_index, 0);
        assert_eq!(matcher.matches(&pip).unwrap().pattern_index, 1);
    }

    // Priority ordering tests

    #[test]
    fn priority_pinned_first() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client("0x1", "mpv", "unpinned", false, true),
            make_client("0x2", "mpv", "pinned", true, true),
            make_client("0x3", "firefox", "browser", false, false),
        ];

        let result = matcher.find_media_window(&clients, None).unwrap();
        assert_eq!(result.address, "0x2");
        assert_eq!(result.priority, 1);
    }

    #[test]
    fn priority_focused_second() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client("0x1", "mpv", "unfocused", false, true),
            make_client("0x2", "mpv", "focused", false, true),
            make_client("0x3", "firefox", "browser", false, false),
        ];

        let result = matcher.find_media_window(&clients, Some("0x2")).unwrap();
        assert_eq!(result.address, "0x2");
        assert_eq!(result.priority, 2);
    }

    #[test]
    fn priority_any_third() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client("0x1", "mpv", "video.mp4", false, true),
            make_client("0x2", "firefox", "browser", false, false),
        ];

        let result = matcher.find_media_window(&clients, None).unwrap();
        assert_eq!(result.address, "0x1");
        assert_eq!(result.priority, 3);
    }

    #[test]
    fn pinned_beats_focused() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client("0x1", "mpv", "focused but not pinned", false, true),
            make_client("0x2", "mpv", "pinned but not focused", true, true),
        ];

        // Even with focus_addr pointing to 0x1, pinned window should win
        let result = matcher.find_media_window(&clients, Some("0x1")).unwrap();
        assert_eq!(result.address, "0x2");
        assert_eq!(result.priority, 1);
    }

    #[test]
    fn no_match_returns_none() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![make_client("0x1", "firefox", "browser", false, false)];

        assert!(matcher.find_media_window(&clients, None).is_none());
    }

    // Previous focus tests

    #[test]
    fn find_previous_focus_same_workspace() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 2),
            make_client_full("0x2", "firefox", "browser", false, false, 0, 1, 0, 0), // Most recent
            make_client_full("0x3", "kitty", "terminal", false, false, 0, 1, 0, 1),
            make_client_full("0x4", "code", "editor", false, false, 0, 2, 0, 0), // Different workspace
        ];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert_eq!(result, Some("0x2".to_string()));
    }

    #[test]
    fn find_previous_focus_explicit_workspace() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 2),
            make_client_full("0x2", "firefox", "browser ws1", false, false, 0, 1, 0, 0),
            make_client_full("0x3", "code", "editor ws2", false, false, 0, 2, 0, 0),
        ];

        // Explicitly request workspace 2
        let result = matcher.find_previous_focus(&clients, "0x1", Some(2));
        assert_eq!(result, Some("0x3".to_string()));
    }

    #[test]
    fn find_previous_focus_excludes_media() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 0), // Most recent but excluded
            make_client_full("0x2", "firefox", "browser", false, false, 0, 1, 0, 1),
        ];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert_eq!(result, Some("0x2".to_string()));
    }

    #[test]
    fn find_previous_focus_excludes_hidden() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mut hidden = make_client_full("0x2", "firefox", "hidden", false, false, 0, 1, 0, 0);
        hidden.hidden = true;

        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 2),
            hidden,
            make_client_full("0x3", "kitty", "visible", false, false, 0, 1, 0, 1),
        ];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert_eq!(result, Some("0x3".to_string()));
    }

    #[test]
    fn find_previous_focus_no_candidates() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![make_client_full(
            "0x1",
            "mpv",
            "video.mp4",
            true,
            true,
            0,
            1,
            0,
            0,
        )];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert!(result.is_none());
    }

    // find_media_windows tests

    #[test]
    fn find_media_windows_on_monitor() {
        let patterns = vec![
            Pattern {
                key: "class".to_string(),
                value: "mpv".to_string(),
                ..Default::default()
            },
            Pattern {
                key: "title".to_string(),
                value: "Picture-in-Picture".to_string(),
                ..Default::default()
            },
        ];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "video1", false, true, 0, 1, 0, 2),
            make_client_full("0x2", "mpv", "video2", true, true, 0, 1, 0, 1), // Pinned
            make_client_full("0x3", "firefox", "pip", false, true, 0, 1, 1, 0), // Different monitor
            make_client_full(
                "0x4",
                "firefox",
                "Picture-in-Picture",
                true,
                true,
                0,
                1,
                0,
                0,
            ),
        ];

        let windows = matcher.find_media_windows(&clients, 0);

        assert_eq!(windows.len(), 3);
        // Pinned windows should come first (0x2 and 0x4 have priority 1)
        assert!(windows[0].pinned || windows[1].pinned);
    }

    #[test]
    fn find_media_windows_sorted_by_priority() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "unpinned1", false, true, 0, 1, 0, 2),
            make_client_full("0x2", "mpv", "pinned", true, true, 0, 1, 0, 1),
            make_client_full("0x3", "mpv", "unpinned2", false, true, 0, 1, 0, 0),
        ];

        let windows = matcher.find_media_windows(&clients, 0);

        assert_eq!(windows.len(), 3);
        // First should be pinned (priority 1)
        assert_eq!(windows[0].address, "0x2");
        assert_eq!(windows[0].priority, 1);
        // Remaining sorted by focus history
        assert_eq!(windows[1].address, "0x3"); // focus_history_id 0
        assert_eq!(windows[2].address, "0x1"); // focus_history_id 2
    }

    // MediaWindow field mapping tests

    #[test]
    fn media_window_fields_mapped_correctly() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            always_pin: true,
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mut client = make_client_full("0xabc", "mpv", "test.mp4", true, true, 2, 3, 1, 5);
        client.at = [200, 300];
        client.size = [800, 450];

        let clients = vec![client];
        let window = matcher.find_media_window(&clients, None).unwrap();

        assert_eq!(window.address, "0xabc");
        assert_eq!(window.class, "mpv");
        assert_eq!(window.title, "test.mp4");
        assert_eq!(window.x, 200);
        assert_eq!(window.y, 300);
        assert_eq!(window.width, 800);
        assert_eq!(window.height, 450);
        assert!(window.pinned);
        assert!(window.floating);
        assert_eq!(window.fullscreen, 2);
        assert_eq!(window.monitor, 1);
        assert_eq!(window.workspace_id, 3);
        assert!(window.always_pin);
        assert_eq!(window.priority, 1); // Pinned
    }

    // Edge cases

    #[test]
    fn empty_patterns() {
        let matcher = WindowMatcher::new(&[]);
        let clients = vec![make_client("0x1", "mpv", "video", false, true)];

        assert!(matcher.find_media_window(&clients, None).is_none());
    }

    #[test]
    fn empty_clients() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        assert!(matcher.find_media_window(&[], None).is_none());
    }

    #[test]
    fn invalid_pattern_key_skipped() {
        let patterns = vec![
            Pattern {
                key: "invalid_key".to_string(),
                value: "mpv".to_string(),
                ..Default::default()
            },
            Pattern {
                key: "class".to_string(),
                value: "mpv".to_string(),
                ..Default::default()
            },
        ];
        let matcher = WindowMatcher::new(&patterns);

        let client = make_client("0x1", "mpv", "video", false, true);
        let result = matcher.matches(&client).unwrap();

        // Should match second pattern (index 0 after filtering)
        assert_eq!(result.pattern_index, 0);
    }

    #[test]
    fn strict_mode_rejects_invalid_regex() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "[invalid".to_string(), // Invalid regex
            ..Default::default()
        }];

        assert!(WindowMatcher::new_strict(&patterns).is_err());
    }

    #[test]
    fn non_strict_mode_skips_invalid_regex() {
        let patterns = vec![
            Pattern {
                key: "class".to_string(),
                value: "[invalid".to_string(), // Invalid regex
                ..Default::default()
            },
            Pattern {
                key: "class".to_string(),
                value: "mpv".to_string(),
                ..Default::default()
            },
        ];

        let matcher = WindowMatcher::new(&patterns);
        let client = make_client("0x1", "mpv", "video", false, true);

        // Should still work with valid pattern
        assert!(matcher.matches(&client).is_some());
    }

    #[test]
    fn find_previous_focus_ignores_never_focused() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        // All candidates have focus_history_id = -1 (never focused)
        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 2),
            make_client_full("0x2", "firefox", "browser", false, false, 0, 1, 0, -1),
            make_client_full("0x3", "kitty", "terminal", false, false, 0, 1, 0, -1),
        ];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert!(
            result.is_none(),
            "should not return never-focused windows"
        );
    }

    #[test]
    fn find_previous_focus_prefers_focused_over_never_focused() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 3),
            make_client_full("0x2", "firefox", "browser", false, false, 0, 1, 0, -1), // never focused
            make_client_full("0x3", "kitty", "terminal", false, false, 0, 1, 0, 1), // was focused
        ];

        let result = matcher.find_previous_focus(&clients, "0x1", None);
        assert_eq!(result, Some("0x3".to_string()));
    }

    #[test]
    fn find_media_window_skips_unmapped() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mut client = make_client("0x1", "mpv", "video", false, true);
        client.mapped = false;

        assert!(matcher.find_media_window(&[client], None).is_none());
    }

    #[test]
    fn find_media_window_skips_hidden() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mut client = make_client("0x1", "mpv", "video", false, true);
        client.hidden = true;

        assert!(matcher.find_media_window(&[client], None).is_none());
    }

    #[test]
    fn find_media_windows_filters_hidden_and_unmapped() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let mut hidden = make_client_full("0x1", "mpv", "h", true, true, 0, 1, 0, 0);
        hidden.hidden = true;
        let mut unmapped = make_client_full("0x2", "mpv", "u", true, true, 0, 1, 0, 1);
        unmapped.mapped = false;
        let visible = make_client_full("0x3", "mpv", "v", true, true, 0, 1, 0, 2);

        let windows = matcher.find_media_windows(&[hidden, unmapped, visible], 0);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].address, "0x3");
    }

    #[test]
    fn find_media_window_first_pinned_wins_when_multiple_match() {
        // Two pinned matches: the first one in client order is selected
        // (the slot is filled once and never overwritten).
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client("0x1", "mpv", "first-pinned", true, true),
            make_client("0x2", "mpv", "second-pinned", true, true),
        ];
        let result = matcher.find_media_window(&clients, None).unwrap();
        assert_eq!(result.address, "0x1");
        assert_eq!(result.priority, PRIORITY_PINNED);
    }

    #[test]
    fn find_previous_focus_includes_id_zero_when_not_media() {
        // Edge case: focus_history_id == 0 is the most-recently-focused window.
        // When the media window's address differs from the id-0 window, the id-0
        // window IS the previous focus and must be returned (not filtered out).
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            // mpv on workspace 1, id 5 (not the focused window)
            make_client_full("0xmpv", "mpv", "video.mp4", true, true, 0, 1, 0, 5),
            // firefox is the focused window (id 0) — must be returned
            make_client_full("0xfx", "firefox", "browser", false, false, 0, 1, 0, 0),
            // a kitty terminal that was focused before firefox
            make_client_full("0xkit", "kitty", "term", false, false, 0, 1, 0, 1),
        ];

        let result = matcher.find_previous_focus(&clients, "0xmpv", None);
        assert_eq!(
            result,
            Some("0xfx".to_string()),
            "id=0 (current focus) is the previous-focus candidate when not the media window"
        );
    }

    #[test]
    fn find_previous_focus_id_zero_filtered_when_it_is_media() {
        // Inverse case: when the media window IS focus_history_id == 0, it is
        // excluded by address, so the next-lowest non-negative id wins.
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0xmpv", "mpv", "video.mp4", true, true, 0, 1, 0, 0),
            make_client_full("0xfx", "firefox", "browser", false, false, 0, 1, 0, 1),
        ];

        let result = matcher.find_previous_focus(&clients, "0xmpv", None);
        assert_eq!(result, Some("0xfx".to_string()));
    }

    #[test]
    fn pattern_regex_size_cap_rejects_huge_pattern() {
        // A regex that produces a state machine larger than the 64 KiB cap
        // must be rejected by both `new` (skipped) and `new_strict` (Err).
        // We construct one via large repetition.
        let huge = "a{50000}".to_string();
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: huge.clone(),
            ..Default::default()
        }];

        // new() skips invalid → no patterns compile → no match
        let matcher = WindowMatcher::new(&patterns);
        let client = make_client("0x1", "aaaa", "test", false, false);
        assert!(
            matcher.matches(&client).is_none(),
            "size-capped pattern must be skipped by new()"
        );

        // new_strict() must return Err
        assert!(
            WindowMatcher::new_strict(&patterns).is_err(),
            "size-capped pattern must error from new_strict()"
        );
    }

    #[test]
    fn pattern_regex_anchoring_is_unanchored() {
        // Documented behavior: patterns are unanchored (substring match),
        // matching the original bash `[[ =~ ]]` semantics. Test enforces that
        // a pattern like "mpv" matches "not-mpv-really" so future refactors
        // that silently anchor would fail this test.
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);
        let client = make_client("0x1", "not-mpv-really", "test", false, false);
        assert!(
            matcher.matches(&client).is_some(),
            "patterns are intentionally unanchored — substring match expected"
        );

        // Anchored pattern in config must NOT match the substring case.
        let anchored = vec![Pattern {
            key: "class".to_string(),
            value: "^mpv$".to_string(),
            ..Default::default()
        }];
        let matcher2 = WindowMatcher::new(&anchored);
        assert!(
            matcher2.matches(&client).is_none(),
            "anchored pattern must reject substring"
        );
    }

    #[test]
    fn find_media_windows_filters_by_monitor() {
        let patterns = vec![Pattern {
            key: "class".to_string(),
            value: "mpv".to_string(),
            ..Default::default()
        }];
        let matcher = WindowMatcher::new(&patterns);

        let clients = vec![
            make_client_full("0x1", "mpv", "mon0", false, true, 0, 1, 0, 0),
            make_client_full("0x2", "mpv", "mon1", false, true, 0, 1, 1, 1),
            make_client_full("0x3", "mpv", "mon2", false, true, 0, 1, 2, 2),
        ];
        let on_mon1 = matcher.find_media_windows(&clients, 1);
        assert_eq!(on_mon1.len(), 1);
        assert_eq!(on_mon1[0].address, "0x2");
    }
}
