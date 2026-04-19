//! Integration tests for configuration loading.

use std::io::Write;

use media_control_lib::config::Config;

#[test]
fn load_config_from_temp_file() {
    let config_content = r#"
[[patterns]]
key = "class"
value = "vlc"
always_pin = true

[positions]
x_left = 10
x_right = 1900
y_top = 10
y_bottom = 1070
width = 854
height = 480

[positioning]
wide_window_threshold = 80
default_x = "x_left"
default_y = "y_top"

[[positioning.overrides]]
focused_class = "kitty"
pref_x = "x_right"
pref_y = "y_bottom"
pref_width = 640
pref_height = 360
"#;

    // Create a temporary file
    let mut temp_file = tempfile::NamedTempFile::new().expect("failed to create temp file");
    temp_file
        .write_all(config_content.as_bytes())
        .expect("failed to write config");

    // Load the config
    let config = Config::load_from_path(temp_file.path()).expect("failed to load config");

    // Verify patterns
    assert_eq!(config.patterns.len(), 1);
    assert_eq!(config.patterns[0].key, "class");
    assert_eq!(config.patterns[0].value, "vlc");
    assert!(config.patterns[0].always_pin);

    // Verify positions
    assert_eq!(config.positions.x_left, 10);
    assert_eq!(config.positions.x_right, 1900);
    assert_eq!(config.positions.y_top, 10);
    assert_eq!(config.positions.y_bottom, 1070);
    assert_eq!(config.positions.width, 854);
    assert_eq!(config.positions.height, 480);

    // Verify positioning
    assert_eq!(config.positioning.wide_window_threshold, 80);
    assert_eq!(config.positioning.default_x, "x_left");
    assert_eq!(config.positioning.default_y, "y_top");

    // Verify overrides
    assert_eq!(config.positioning.overrides.len(), 1);
    let kitty = &config.positioning.overrides[0];
    assert_eq!(kitty.focused_class, "kitty");
    assert_eq!(kitty.pref_x.as_deref(), Some("x_right"));
    assert_eq!(kitty.pref_y.as_deref(), Some("y_bottom"));
    assert_eq!(kitty.pref_width, Some(640));
    assert_eq!(kitty.pref_height, Some(360));
}

#[test]
fn load_nonexistent_file_returns_error() {
    let result = Config::load_from_path(std::path::Path::new("/nonexistent/path/config.toml"));
    assert!(result.is_err());
}

#[test]
fn load_invalid_toml_returns_parse_error() {
    let mut temp_file = tempfile::NamedTempFile::new().expect("failed to create temp file");
    temp_file
        .write_all(b"this is not valid toml [[[")
        .expect("failed to write");

    let result = Config::load_from_path(temp_file.path());
    assert!(result.is_err());
}
