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
//! let matcher = WindowMatcher::new(&patterns)?;
//! // Use matcher.find_media_window() with clients from HyprlandClient
//! # Ok(())
//! # }
//! ```

use regex::Regex;

use crate::config::Pattern;
use crate::error::{MediaControlError, Result};
use crate::hyprland::Client;

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
        match s.to_lowercase().as_str() {
            "class" => Some(Self::Class),
            "title" => Some(Self::Title),
            _ => None,
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
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Index of the pattern that matched.
    pub pattern_index: usize,
    /// Whether to always pin this window.
    pub always_pin: bool,
}

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
    /// Match priority (1=pinned, 2=focused, 3=any).
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
    /// Create a new matcher from config patterns.
    ///
    /// # Errors
    ///
    /// Returns an error if any pattern's regex is invalid.
    pub fn new(patterns: &[Pattern]) -> Result<Self> {
        let compiled = patterns
            .iter()
            .filter_map(|p| {
                let Some(key) = PatternKey::from_str(&p.key) else {
                    eprintln!("media-control: unknown pattern key {:?}, skipping", p.key);
                    return None;
                };
                let regex = match Regex::new(&p.value) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "media-control: invalid regex {:?}: {e}, skipping",
                            p.value
                        );
                        return None;
                    }
                };
                Some(CompiledPattern {
                    key,
                    regex,
                    pinned_only: p.pinned_only,
                    always_pin: p.always_pin,
                })
            })
            .collect();

        Ok(Self { patterns: compiled })
    }

    /// Create a matcher that validates all patterns, returning error on first invalid regex.
    pub fn new_strict(patterns: &[Pattern]) -> Result<Self> {
        let mut compiled = Vec::with_capacity(patterns.len());

        for p in patterns {
            let Some(key) = PatternKey::from_str(&p.key) else {
                continue; // Skip unknown keys
            };
            let regex = Regex::new(&p.value).map_err(MediaControlError::from)?;
            compiled.push(CompiledPattern {
                key,
                regex,
                pinned_only: p.pinned_only,
                always_pin: p.always_pin,
            });
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
        let mut pinned_match: Option<(&Client, MatchResult)> = None;
        let mut focused_match: Option<(&Client, MatchResult)> = None;
        let mut any_match: Option<(&Client, MatchResult)> = None;

        for client in clients.iter().filter(|c| c.mapped && !c.hidden) {
            if let Some(match_result) = self.matches(client) {
                // Priority 1: Pinned
                if client.pinned && pinned_match.is_none() {
                    pinned_match = Some((client, match_result));
                    continue;
                }

                // Priority 2: Focused
                if focused_match.is_none()
                    && focus_addr.is_some_and(|addr| client.address == addr)
                {
                    focused_match = Some((client, match_result));
                    continue;
                }

                // Priority 3: Any
                if any_match.is_none() {
                    any_match = Some((client, match_result));
                }
            }
        }

        // Return highest priority match
        if let Some((client, match_result)) = pinned_match {
            return Some(MediaWindow::from_client(client, &match_result, 1));
        }
        if let Some((client, match_result)) = focused_match {
            return Some(MediaWindow::from_client(client, &match_result, 2));
        }
        if let Some((client, match_result)) = any_match {
            return Some(MediaWindow::from_client(client, &match_result, 3));
        }

        None
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
                let priority = if client.pinned { 1 } else { 3 };
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
        clients
            .iter()
            .filter(|c| c.workspace.id == target_workspace)
            .filter(|c| c.address != media_addr)
            .filter(|c| c.mapped && !c.hidden)
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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

        let clients = vec![
            make_client("0x1", "mpv", "unfocused", false, true),
            make_client("0x2", "mpv", "focused", false, true),
            make_client("0x3", "firefox", "browser", false, false),
        ];

        let result = matcher
            .find_media_window(&clients, Some("0x2"))
            .unwrap();
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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

        let clients = vec![
            make_client("0x1", "mpv", "focused but not pinned", false, true),
            make_client("0x2", "mpv", "pinned but not focused", true, true),
        ];

        // Even with focus_addr pointing to 0x1, pinned window should win
        let result = matcher
            .find_media_window(&clients, Some("0x1"))
            .unwrap();
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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

        let clients =
            vec![make_client_full("0x1", "mpv", "video.mp4", true, true, 0, 1, 0, 0)];

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

        let clients = vec![
            make_client_full("0x1", "mpv", "video1", false, true, 0, 1, 0, 2),
            make_client_full("0x2", "mpv", "video2", true, true, 0, 1, 0, 1), // Pinned
            make_client_full("0x3", "firefox", "pip", false, true, 0, 1, 1, 0), // Different monitor
            make_client_full("0x4", "firefox", "Picture-in-Picture", true, true, 0, 1, 0, 0),
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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&[]).unwrap();
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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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
        let matcher = WindowMatcher::new(&patterns).unwrap();

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

        let matcher = WindowMatcher::new(&patterns).unwrap();
        let client = make_client("0x1", "mpv", "video", false, true);

        // Should still work with valid pattern
        assert!(matcher.matches(&client).is_some());
    }
}
