//! TOML configuration parsing for media window patterns and positions.
//!
//! Loads configuration from `~/.config/media-control/config.toml` with support for:
//! - Window matching patterns (class/title regex)
//! - Corner coordinates for window positioning
//! - Avoidance behavior settings with per-class overrides
//!
//! # Example Configuration
//!
//! ```toml
//! [[patterns]]
//! key = "class"
//! value = "mpv"
//!
//! [[patterns]]
//! key = "title"
//! value = "Picture-in-Picture"
//! always_pin = true
//!
//! [positions]
//! x_left = 48
//! x_right = 1272
//! y_top = 48
//! y_bottom = 712
//! width = 600
//! height = 338
//!
//! [positioning]
//! wide_window_threshold = 90
//! position_tolerance = 5
//! default_x = "x_right"
//! default_y = "y_bottom"
//!
//! [[positioning.overrides]]
//! focused_class = "firefox"
//! pref_x = "x_left"
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

/// Maximum size accepted for the user config file.
///
/// Real configs are < 16 KiB; the cap exists to prevent OOM if a hostile
/// or accidental large file is symlinked at the config path.
const MAX_CONFIG_FILE_BYTES: u64 = 1024 * 1024; // 1 MiB

/// Maximum compiled-NFA size for a user-supplied title regex (bytes).
/// Caps catastrophic-backtracking surface area for daemon-hot regexes.
const TITLE_REGEX_SIZE_LIMIT: usize = 64 * 1024;

/// Error type for configuration operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("config file too large: {size} bytes (max {max})")]
    TooLarge { size: u64, max: u64 },

    #[error("home directory not found")]
    NoHomeDir,

    /// A field's value violates its documented invariants.
    ///
    /// `field` names the offending key; `reason` describes the constraint.
    #[error("invalid value for {field}: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: String,
    },

    /// A user-supplied regex failed to compile.
    ///
    /// Pre-validating these at load time avoids silently broken matches in
    /// the daemon hot path (where errors would otherwise be swallowed).
    #[error("invalid regex for {field} ({pattern:?}): {source}")]
    InvalidRegex {
        field: &'static str,
        pattern: String,
        #[source]
        source: regex::Error,
    },
}

/// Result type alias for configuration operations.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Reject values exceeding a millisecond ceiling.
///
/// Centralises the `debounce_ms`/`suppress_ms` ceiling check so both fields
/// share identical wording and bounds.
fn check_ms_ceiling(field: &'static str, value: u16, ceiling: u16) -> Result<()> {
    if value > ceiling {
        return Err(ConfigError::InvalidValue {
            field,
            reason: format!("must be <= {ceiling} (got {value})"),
        });
    }
    Ok(())
}

/// Reject non-positive integer values.
///
/// Used for pixel dimensions where zero/negative makes no geometric sense.
fn check_positive(field: &'static str, value: i32) -> Result<()> {
    if value <= 0 {
        return Err(ConfigError::InvalidValue {
            field,
            reason: format!("must be > 0 (got {value})"),
        });
    }
    Ok(())
}

/// Root configuration structure.
///
/// Contains all settings for media window management including
/// patterns, positions, and avoidance behavior.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Window matching patterns.
    pub patterns: Vec<Pattern>,

    /// Corner coordinates for positioning.
    pub positions: Positions,

    /// Avoidance behavior settings.
    pub positioning: Positioning,
}

impl Config {
    /// Load configuration from the default path (`~/.config/media-control/config.toml`).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The home directory cannot be determined
    /// - The config file cannot be read
    /// - The TOML content is malformed
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from_path(&path)
    }

    /// Load configuration from a specific path.
    ///
    /// The file is size-capped at [`MAX_CONFIG_FILE_BYTES`] to prevent OOM
    /// on a malformed (or symlinked) input. The cap is enforced both via
    /// `metadata` (fast-path rejection) and via a bounded reader (defense
    /// against TOCTOU growth between metadata and read).
    ///
    /// After parsing, [`Config::validate`] is invoked to surface invalid
    /// numeric ranges or malformed regexes at load time rather than at
    /// daemon hot-path execution.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read or exceeds the size cap
    /// - The TOML content is malformed
    /// - Validation of any field fails
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let meta = std::fs::metadata(path)?;
        if meta.len() > MAX_CONFIG_FILE_BYTES {
            return Err(ConfigError::TooLarge {
                size: meta.len(),
                max: MAX_CONFIG_FILE_BYTES,
            });
        }

        // Bounded read: even if the file grows after the metadata check
        // (TOCTOU) or is a special file lying about its size, we cap memory.
        let file = std::fs::File::open(path)?;
        let mut content = String::new();
        file.take(MAX_CONFIG_FILE_BYTES + 1)
            .read_to_string(&mut content)?;
        if content.len() as u64 > MAX_CONFIG_FILE_BYTES {
            return Err(ConfigError::TooLarge {
                size: content.len() as u64,
                max: MAX_CONFIG_FILE_BYTES,
            });
        }

        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration invariants surfaced at load time.
    ///
    /// Currently checks:
    /// - `positioning.wide_window_threshold` is in `0..=100` (percentage).
    /// - `positioning.minified_scale` is finite and in `(0.0, 1.0]`.
    /// - `positioning.debounce_ms` and `suppress_ms` are within sane bounds
    ///   (a 60 s ceiling — anything larger is almost certainly a typo and
    ///   would freeze the daemon).
    /// - `positions.width` and `positions.height` are positive.
    /// - Every `patterns[].value` regex compiles.
    /// - Every `positioning.overrides[].focused_title` regex compiles
    ///   (within the same NFA size cap used at runtime).
    ///
    /// Validating up front avoids silent fallback in the avoider hot path,
    /// where a malformed regex would otherwise silently never match.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidValue`] for out-of-range numerics or
    /// [`ConfigError::InvalidRegex`] for any regex that fails to compile.
    pub fn validate(&self) -> Result<()> {
        Self::validate_positioning(&self.positioning)?;
        Self::validate_positions(&self.positions)?;
        Self::validate_pattern_regexes(&self.patterns)?;
        Self::validate_override_regexes(&self.positioning.overrides)
    }

    /// Validate numeric ranges on `[positioning]`.
    fn validate_positioning(p: &Positioning) -> Result<()> {
        if p.wide_window_threshold > 100 {
            return Err(ConfigError::InvalidValue {
                field: "positioning.wide_window_threshold",
                reason: format!("must be in 0..=100 (got {})", p.wide_window_threshold),
            });
        }
        let scale = p.minified_scale;
        if !scale.is_finite() || scale <= 0.0 || scale > 1.0 {
            return Err(ConfigError::InvalidValue {
                field: "positioning.minified_scale",
                reason: format!("must be finite in (0.0, 1.0] (got {scale})"),
            });
        }
        // Cap timeouts at 60 s. The type bound (u16) already caps at 65 535,
        // but a single 60 s+ debounce or suppression window would freeze the
        // avoider daemon — almost certainly a misconfiguration.
        const MS_CEILING: u16 = 60_000;
        check_ms_ceiling("positioning.debounce_ms", p.debounce_ms, MS_CEILING)?;
        check_ms_ceiling("positioning.suppress_ms", p.suppress_ms, MS_CEILING)?;
        Ok(())
    }

    /// Validate numeric ranges on `[positions]`.
    fn validate_positions(p: &Positions) -> Result<()> {
        check_positive("positions.width", p.width)?;
        check_positive("positions.height", p.height)?;
        Ok(())
    }

    /// Validate that every `patterns[].value` regex compiles.
    fn validate_pattern_regexes(patterns: &[Pattern]) -> Result<()> {
        for p in patterns {
            Regex::new(&p.value).map_err(|source| ConfigError::InvalidRegex {
                field: "patterns[].value",
                pattern: p.value.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// Validate that every override's `focused_title` regex compiles within
    /// the NFA size cap that the daemon enforces at runtime.
    fn validate_override_regexes(overrides: &[PositionOverride]) -> Result<()> {
        for o in overrides {
            if o.focused_title.is_empty() {
                continue;
            }
            RegexBuilder::new(&o.focused_title)
                .size_limit(TITLE_REGEX_SIZE_LIMIT)
                .build()
                .map_err(|source| ConfigError::InvalidRegex {
                    field: "positioning.overrides[].focused_title",
                    pattern: o.focused_title.clone(),
                    source,
                })?;
        }
        Ok(())
    }

    /// Get the default configuration file path.
    ///
    /// Returns `~/.config/media-control/config.toml`.
    ///
    /// Respects `XDG_CONFIG_HOME` if set, otherwise falls back to `$HOME/.config`.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
            .ok_or(ConfigError::NoHomeDir)?;

        Ok(config_dir.join("media-control").join("config.toml"))
    }

    /// Resolve a position variable name to its value.
    ///
    /// Accepts either a direct integer value (as string) or a variable name
    /// like "x_left", "x_right", "y_top", "y_bottom", "width", "height".
    pub fn resolve_position(&self, name: &str) -> Option<i32> {
        match name {
            "x_left" => Some(self.positions.x_left),
            "x_right" => Some(self.positions.x_right),
            "y_top" => Some(self.positions.y_top),
            "y_bottom" => Some(self.positions.y_bottom),
            "width" => Some(self.positions.width),
            "height" => Some(self.positions.height),
            _ => name.parse().ok(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            patterns: vec![
                Pattern {
                    key: "class".to_string(),
                    value: "mpv".to_string(),
                    pinned_only: false,
                    always_pin: false,
                },
                Pattern {
                    key: "title".to_string(),
                    value: "Picture-in-Picture".to_string(),
                    pinned_only: false,
                    always_pin: true,
                },
                Pattern {
                    key: "class".to_string(),
                    value: "com.github.iwalton3.jellyfin-media-player".to_string(),
                    pinned_only: true,
                    always_pin: false,
                },
            ],
            positions: Positions::default(),
            positioning: Positioning::default(),
        }
    }
}

/// Window matching pattern.
///
/// Patterns are evaluated in order to find media windows.
/// Each pattern specifies a property to match and a regex value.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Pattern {
    /// Property to match: "class" or "title".
    pub key: String,

    /// Regex pattern to match against the property value.
    pub value: String,

    /// Only match if the window is pinned or fullscreen.
    #[serde(default)]
    pub pinned_only: bool,

    /// Automatically pin windows matching this pattern.
    #[serde(default)]
    pub always_pin: bool,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            key: "class".to_string(),
            value: String::new(),
            pinned_only: false,
            always_pin: false,
        }
    }
}

/// Corner coordinates for window positioning.
///
/// Defines the four corners of the positioning area plus default dimensions.
/// Coordinates are in pixels from the top-left of the monitor.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Positions {
    /// X coordinate of the left position.
    pub x_left: i32,

    /// X coordinate of the right position.
    pub x_right: i32,

    /// Y coordinate of the top position.
    pub y_top: i32,

    /// Y coordinate of the bottom position.
    pub y_bottom: i32,

    /// Default window width.
    pub width: i32,

    /// Default window height.
    pub height: i32,
}

impl Default for Positions {
    fn default() -> Self {
        Self {
            x_left: 48,
            x_right: 1272,
            y_top: 48,
            y_bottom: 712,
            width: 600,
            height: 338,
        }
    }
}

/// Avoidance behavior settings.
///
/// Controls how media windows are repositioned to avoid
/// overlapping with focused windows.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Positioning {
    /// Percentage of screen width above which a window is considered "wide".
    /// Wide windows trigger vertical repositioning instead of horizontal.
    pub wide_window_threshold: u8,

    /// Pixels of tolerance for position comparison.
    /// Windows within this distance of the target are considered correctly positioned.
    pub position_tolerance: u8,

    /// Debounce timeout in milliseconds for the daemon.
    /// Prevents rapid-fire avoid calls from multiple events.
    pub debounce_ms: u16,

    /// Suppression timeout in milliseconds.
    /// After repositioning, avoid is suppressed for this duration to prevent loops.
    pub suppress_ms: u16,

    /// Default X position variable name ("x_left" or "x_right").
    pub default_x: String,

    /// Default Y position variable name ("y_top" or "y_bottom").
    pub default_y: String,

    /// Default secondary X position variable for mouseover toggle.
    /// When moused over, the window toggles between default_x and secondary_x.
    pub secondary_x: String,

    /// Default secondary Y position variable for mouseover toggle.
    /// When moused over, the window toggles between default_y and secondary_y.
    pub secondary_y: String,

    /// Scale factor for minified mode (0.0–1.0, default 0.5).
    /// When minified, window dimensions are multiplied by this factor.
    #[serde(default = "default_minified_scale")]
    pub minified_scale: f32,

    /// Per-class position overrides.
    #[serde(default)]
    pub overrides: Vec<PositionOverride>,
}

fn default_minified_scale() -> f32 {
    0.5
}

impl Default for Positioning {
    fn default() -> Self {
        Self {
            wide_window_threshold: 90,
            position_tolerance: 5,
            debounce_ms: 15,
            suppress_ms: 150,
            default_x: "x_right".to_string(),
            default_y: "y_bottom".to_string(),
            secondary_x: "x_left".to_string(),
            secondary_y: "y_bottom".to_string(),
            minified_scale: default_minified_scale(),
            overrides: Vec::new(),
        }
    }
}

impl Positioning {
    /// Get preferred position for a focused window.
    ///
    /// Matches overrides by class name (case-insensitive) and/or title (regex).
    /// Returns the first matching override, or None if no match.
    ///
    /// Regexes are compiled lazily on first use and cached for subsequent calls.
    pub fn get_override(
        &self,
        focused_class: &str,
        focused_title: &str,
    ) -> Option<&PositionOverride> {
        self.overrides.iter().find(|o| {
            // Check class match (if specified, case-insensitive)
            let class_matches =
                o.focused_class.is_empty() || o.focused_class.eq_ignore_ascii_case(focused_class);

            // Check title match (if specified, using cached regex)
            let title_matches = o.matches_title(focused_title);

            // Must have at least one non-empty matcher, and all specified must match
            let has_matcher = !o.focused_class.is_empty() || !o.focused_title.is_empty();
            has_matcher && class_matches && title_matches
        })
    }
}

/// Per-window position override.
///
/// Allows specifying different positioning preferences when
/// specific windows are focused. Matches by class name (case-insensitive) and/or title regex.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PositionOverride {
    /// Window class name to match (case-insensitive).
    #[serde(default)]
    pub focused_class: String,

    /// Window title regex pattern to match.
    #[serde(default)]
    pub focused_title: String,

    /// Compiled regex for title matching (populated lazily).
    #[serde(skip)]
    compiled_title_regex: OnceLock<Option<Regex>>,

    /// Primary X position variable (e.g., "x_left", "x_right").
    pub pref_x: Option<String>,

    /// Primary Y position variable (e.g., "y_top", "y_bottom").
    pub pref_y: Option<String>,

    /// Secondary X position variable for mouseover toggle.
    pub secondary_x: Option<String>,

    /// Secondary Y position variable for mouseover toggle.
    pub secondary_y: Option<String>,

    /// Override width for this class.
    pub pref_width: Option<i32>,

    /// Override height for this class.
    pub pref_height: Option<i32>,
}

impl Clone for PositionOverride {
    fn clone(&self) -> Self {
        Self {
            focused_class: self.focused_class.clone(),
            focused_title: self.focused_title.clone(),
            compiled_title_regex: OnceLock::new(), // Don't clone cached regex, recompile if needed
            pref_x: self.pref_x.clone(),
            pref_y: self.pref_y.clone(),
            secondary_x: self.secondary_x.clone(),
            secondary_y: self.secondary_y.clone(),
            pref_width: self.pref_width,
            pref_height: self.pref_height,
        }
    }
}

impl PositionOverride {
    /// Get the compiled title regex, compiling it on first access.
    ///
    /// Compilation is bounded by [`TITLE_REGEX_SIZE_LIMIT`] to cap the worst
    /// case for catastrophic-backtracking patterns supplied by user config —
    /// this matcher runs in the daemon hot path on every focus event.
    fn title_regex(&self) -> Option<&Regex> {
        self.compiled_title_regex
            .get_or_init(|| {
                if self.focused_title.is_empty() {
                    None
                } else {
                    RegexBuilder::new(&self.focused_title)
                        .size_limit(TITLE_REGEX_SIZE_LIMIT)
                        .build()
                        .ok()
                }
            })
            .as_ref()
    }

    /// Check if the given title matches this override's pattern.
    fn matches_title(&self, title: &str) -> bool {
        if self.focused_title.is_empty() {
            true
        } else {
            self.title_regex()
                .map(|re| re.is_match(title))
                .unwrap_or(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONFIG: &str = r#"
[[patterns]]
key = "class"
value = "mpv"

[[patterns]]
key = "title"
value = "Picture-in-Picture"
always_pin = true

[[patterns]]
key = "class"
value = "com.github.iwalton3.jellyfin-media-player"
pinned_only = true

[positions]
x_left = 100
x_right = 1200
y_top = 50
y_bottom = 700
width = 640
height = 360

[positioning]
wide_window_threshold = 85
position_tolerance = 10
default_x = "x_left"
default_y = "y_top"

[[positioning.overrides]]
focused_class = "firefox"
pref_x = "x_right"
pref_y = "y_bottom"

[[positioning.overrides]]
focused_class = "code"
pref_width = 800
pref_height = 450
"#;

    #[test]
    fn parse_sample_config() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        assert_eq!(config.patterns.len(), 3);
        assert_eq!(config.patterns[0].key, "class");
        assert_eq!(config.patterns[0].value, "mpv");
        assert!(!config.patterns[0].always_pin);

        assert_eq!(config.patterns[1].key, "title");
        assert_eq!(config.patterns[1].value, "Picture-in-Picture");
        assert!(config.patterns[1].always_pin);

        assert!(config.patterns[2].pinned_only);
    }

    #[test]
    fn parse_positions() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        assert_eq!(config.positions.x_left, 100);
        assert_eq!(config.positions.x_right, 1200);
        assert_eq!(config.positions.y_top, 50);
        assert_eq!(config.positions.y_bottom, 700);
        assert_eq!(config.positions.width, 640);
        assert_eq!(config.positions.height, 360);
    }

    #[test]
    fn parse_positioning() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        assert_eq!(config.positioning.wide_window_threshold, 85);
        assert_eq!(config.positioning.position_tolerance, 10);
        assert_eq!(config.positioning.default_x, "x_left");
        assert_eq!(config.positioning.default_y, "y_top");
    }

    #[test]
    fn parse_overrides() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        assert_eq!(config.positioning.overrides.len(), 2);

        let firefox = &config.positioning.overrides[0];
        assert_eq!(firefox.focused_class, "firefox");
        assert_eq!(firefox.pref_x.as_deref(), Some("x_right"));
        assert_eq!(firefox.pref_y.as_deref(), Some("y_bottom"));
        assert!(firefox.pref_width.is_none());

        let code = &config.positioning.overrides[1];
        assert_eq!(code.focused_class, "code");
        assert_eq!(code.pref_width, Some(800));
        assert_eq!(code.pref_height, Some(450));
    }

    #[test]
    fn default_values_applied() {
        let minimal_config = r#"
[[patterns]]
key = "class"
value = "mpv"
"#;
        let config: Config = toml::from_str(minimal_config).expect("failed to parse config");

        // Pattern defaults
        assert!(!config.patterns[0].pinned_only);
        assert!(!config.patterns[0].always_pin);

        // Position defaults
        assert_eq!(config.positions.x_left, 48);
        assert_eq!(config.positions.x_right, 1272);
        assert_eq!(config.positions.y_top, 48);
        assert_eq!(config.positions.y_bottom, 712);
        assert_eq!(config.positions.width, 600);
        assert_eq!(config.positions.height, 338);

        // Positioning defaults
        assert_eq!(config.positioning.wide_window_threshold, 90);
        assert_eq!(config.positioning.position_tolerance, 5);
        assert_eq!(config.positioning.default_x, "x_right");
        assert_eq!(config.positioning.default_y, "y_bottom");
        assert!(config.positioning.overrides.is_empty());
    }

    #[test]
    fn empty_config_uses_all_defaults() {
        let config: Config = toml::from_str("").expect("failed to parse empty config");

        // Empty config uses Config::default() which includes standard patterns
        assert_eq!(config.patterns.len(), 3);
        assert_eq!(config.patterns[0].value, "mpv");

        // Position defaults still apply
        assert_eq!(config.positions.x_right, 1272);
        assert_eq!(config.positions.y_bottom, 712);
    }

    #[test]
    fn config_default_has_standard_patterns() {
        let config = Config::default();

        assert_eq!(config.patterns.len(), 3);
        assert_eq!(config.patterns[0].value, "mpv");
        assert!(config.patterns[1].always_pin); // Picture-in-Picture
        assert!(config.patterns[2].pinned_only); // Jellyfin
    }

    #[test]
    fn resolve_position_variable() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        assert_eq!(config.resolve_position("x_left"), Some(100));
        assert_eq!(config.resolve_position("x_right"), Some(1200));
        assert_eq!(config.resolve_position("y_top"), Some(50));
        assert_eq!(config.resolve_position("y_bottom"), Some(700));
        assert_eq!(config.resolve_position("width"), Some(640));
        assert_eq!(config.resolve_position("height"), Some(360));

        // Direct numeric values
        assert_eq!(config.resolve_position("500"), Some(500));
        assert_eq!(config.resolve_position("-100"), Some(-100));

        // Unknown variable
        assert_eq!(config.resolve_position("unknown"), None);
    }

    #[test]
    fn get_override_for_class() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("failed to parse config");

        // Match by class
        let firefox = config.positioning.get_override("firefox", "");
        assert!(firefox.is_some());
        assert_eq!(firefox.unwrap().pref_x.as_deref(), Some("x_right"));

        let code = config.positioning.get_override("code", "any title");
        assert!(code.is_some());
        assert_eq!(code.unwrap().pref_width, Some(800));

        // No match
        let unknown = config.positioning.get_override("unknown_class", "");
        assert!(unknown.is_none());
    }

    #[test]
    fn get_override_for_title_regex() {
        let config_with_title = r#"
[[positioning.overrides]]
focused_title = "(?i)✳"
pref_x = "x_left"
"#;
        let config: Config = toml::from_str(config_with_title).expect("failed to parse config");

        // Match by title regex (Unicode)
        let claude = config
            .positioning
            .get_override("kitty", "✳ Claude Config Issue");
        assert!(claude.is_some(), "Should match title with ✳");
        assert_eq!(claude.unwrap().pref_x.as_deref(), Some("x_left"));

        // No match - different title
        let other = config.positioning.get_override("kitty", "Some other title");
        assert!(other.is_none(), "Should not match without ✳");
    }

    #[test]
    fn partial_positions_use_defaults() {
        let partial_config = r#"
[positions]
x_left = 200
width = 500
"#;
        let config: Config = toml::from_str(partial_config).expect("failed to parse config");

        assert_eq!(config.positions.x_left, 200);
        assert_eq!(config.positions.width, 500);
        // Defaults for unspecified fields
        assert_eq!(config.positions.x_right, 1272);
        assert_eq!(config.positions.y_top, 48);
    }

    #[test]
    fn partial_positioning_uses_defaults() {
        let partial_config = r#"
[positioning]
wide_window_threshold = 75
"#;
        let config: Config = toml::from_str(partial_config).expect("failed to parse config");

        assert_eq!(config.positioning.wide_window_threshold, 75);
        // Defaults for unspecified fields
        assert_eq!(config.positioning.position_tolerance, 5);
        assert_eq!(config.positioning.default_x, "x_right");
    }

    // --- Edge case tests ---

    #[test]
    fn resolve_position_zero() {
        let config = Config::default();
        assert_eq!(config.resolve_position("0"), Some(0));
    }

    #[test]
    fn resolve_position_negative() {
        let config = Config::default();
        assert_eq!(config.resolve_position("-100"), Some(-100));
    }

    #[test]
    fn resolve_position_large_value() {
        let config = Config::default();
        assert_eq!(config.resolve_position("99999"), Some(99999));
    }

    #[test]
    fn resolve_position_unknown_name() {
        let config = Config::default();
        assert_eq!(config.resolve_position("nonexistent"), None);
    }

    #[test]
    fn resolve_position_empty_string() {
        let config = Config::default();
        assert_eq!(config.resolve_position(""), None);
    }

    #[test]
    fn get_override_class_and_title_both_must_match() {
        let config_str = r#"
[[positioning.overrides]]
focused_class = "kitty"
focused_title = "(?i)special"
pref_x = "x_left"
"#;
        let config: Config = toml::from_str(config_str).expect("parse");

        // Both match
        let result = config.positioning.get_override("kitty", "Special Terminal");
        assert!(
            result.is_some(),
            "should match when both class and title match"
        );

        // Class matches, title doesn't
        let result = config.positioning.get_override("kitty", "Regular Terminal");
        assert!(
            result.is_none(),
            "should not match when title doesn't match"
        );

        // Title matches, class doesn't
        let result = config.positioning.get_override("firefox", "Special Page");
        assert!(
            result.is_none(),
            "should not match when class doesn't match"
        );
    }

    #[test]
    fn get_override_class_only_matches_any_title() {
        let config_str = r#"
[[positioning.overrides]]
focused_class = "firefox"
pref_x = "x_left"
"#;
        let config: Config = toml::from_str(config_str).expect("parse");

        let result = config.positioning.get_override("firefox", "any title here");
        assert!(
            result.is_some(),
            "class-only override should match any title"
        );

        let result = config.positioning.get_override("firefox", "");
        assert!(
            result.is_some(),
            "class-only override should match empty title"
        );
    }

    #[test]
    fn get_override_title_only_matches_any_class() {
        let config_str = r#"
[[positioning.overrides]]
focused_title = "(?i)special"
pref_x = "x_left"
"#;
        let config: Config = toml::from_str(config_str).expect("parse");

        let result = config
            .positioning
            .get_override("anything", "Special Window");
        assert!(
            result.is_some(),
            "title-only override should match any class"
        );
    }

    // --- Validation tests ---

    #[test]
    fn validate_accepts_default_config() {
        let config = Config::default();
        assert!(
            config.validate().is_ok(),
            "Default config must satisfy its own invariants"
        );
    }

    #[test]
    fn validate_rejects_threshold_above_100() {
        let mut config = Config::default();
        config.positioning.wide_window_threshold = 101;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::InvalidValue {
                    field: "positioning.wide_window_threshold",
                    ..
                }
            ),
            "got: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_minified_scale_zero() {
        let mut config = Config::default();
        config.positioning.minified_scale = 0.0;
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidValue {
                field: "positioning.minified_scale",
                ..
            }
        ));
    }

    #[test]
    fn validate_rejects_minified_scale_negative_or_nan() {
        for bad in [-0.5_f32, f32::NAN, f32::INFINITY, 2.0] {
            let mut config = Config::default();
            config.positioning.minified_scale = bad;
            assert!(
                config.validate().is_err(),
                "scale={bad} should be rejected"
            );
        }
    }

    #[test]
    fn validate_rejects_oversize_debounce_ms() {
        let mut config = Config::default();
        config.positioning.debounce_ms = 60_001;
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidValue {
                field: "positioning.debounce_ms",
                ..
            }
        ));
    }

    #[test]
    fn validate_rejects_oversize_suppress_ms() {
        let mut config = Config::default();
        config.positioning.suppress_ms = 60_001;
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidValue {
                field: "positioning.suppress_ms",
                ..
            }
        ));
    }

    #[test]
    fn validate_accepts_zero_debounce_and_suppress() {
        // Zero is meaningful (disable debounce / suppression), so the
        // validator must not reject it.
        let mut config = Config::default();
        config.positioning.debounce_ms = 0;
        config.positioning.suppress_ms = 0;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_or_negative_dimensions() {
        let mut config = Config::default();
        config.positions.width = 0;
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidValue { field: "positions.width", .. }
        ));

        let mut config = Config::default();
        config.positions.height = -1;
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidValue { field: "positions.height", .. }
        ));
    }

    #[test]
    fn validate_rejects_invalid_pattern_regex() {
        let mut config = Config::default();
        config.patterns.push(Pattern {
            key: "title".into(),
            value: "[unclosed".into(),
            pinned_only: false,
            always_pin: false,
        });
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidRegex { field: "patterns[].value", .. }
        ));
    }

    #[test]
    fn validate_rejects_invalid_override_title_regex() {
        let mut config = Config::default();
        config.positioning.overrides.push(PositionOverride {
            focused_class: String::new(),
            focused_title: "[unclosed".into(),
            ..Default::default()
        });
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidRegex {
                field: "positioning.overrides[].focused_title",
                ..
            }
        ));
    }

    #[test]
    fn load_from_path_runs_validation() {
        // Sanity: load_from_path must reject configs whose post-parse state
        // violates `validate()`.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[positioning]
wide_window_threshold = 250
"#,
        )
        .expect("write");
        let err = Config::load_from_path(&path).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidValue {
                field: "positioning.wide_window_threshold",
                ..
            }
        ));
    }

    #[test]
    fn load_from_path_caps_size() {
        // A multi-megabyte file should be rejected by the metadata pre-check.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.toml");
        // 2 MiB of comment lines — fast to write, oversized for the cap.
        let blob = "# ".to_string() + &"x".repeat(2 * 1024 * 1024);
        std::fs::write(&path, blob).expect("write");
        let err = Config::load_from_path(&path).unwrap_err();
        assert!(matches!(err, ConfigError::TooLarge { .. }));
    }
}
