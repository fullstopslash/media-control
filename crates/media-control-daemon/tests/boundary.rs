//! Daemon ↔ workflow boundary verification (companion to ADR-001).
//!
//! Scans every `.rs` file under `crates/media-control-daemon/src/` for
//! forbidden import patterns and asserts none are present. This catches the
//! cargo-workspace-feature-unification escape path that the structural
//! `cli` cargo feature alone cannot close: under
//! `cargo build --workspace`, the CLI's `features = ["cli"]` activates the
//! lib's `cli` feature for the daemon's lib build too, so the daemon
//! source could compile a forbidden import without the feature flag
//! catching it. This grep test runs as part of `cargo test --workspace`
//! and fails on any matching import.
//!
//! # What this catches
//!
//! - `use media_control_lib::commands::workflow::...`
//! - `use media_control_lib::jellyfin...`
//! - The same paths reached through any `as` alias on the same line
//! - The shim leak path: `use media_control_lib::commands::send_mpv_*`
//!   and `use media_control_lib::commands::query_mpv_property`
//!   (top-level re-exports of workflow items via `commands/mod.rs` shims)
//!
//! # What this does NOT catch
//!
//! - Imports via deeply renamed paths (e.g. importing the lib root and then
//!   accessing `media_control_lib::commands::workflow::X` through a non-`use`
//!   path expression). This is a known coverage gap; the realistic failure
//!   mode is a contributor typing a `use` line, which this catches.
//! - Imports added to source files outside `crates/media-control-daemon/src/`.
//!   That's by design — only the daemon's own source is the bounded contract.
//!
//! # Adding a new forbidden pattern
//!
//! Append to `FORBIDDEN_PATTERNS`. Each pattern is a substring that the test
//! grep-matches against a file's lines.

use std::fs;
use std::path::{Path, PathBuf};

/// Substrings that must not appear in any line of any daemon `.rs` source file.
///
/// All entries express paths into modules that the daemon's contract forbids.
/// Match is substring-based (not regex), case-sensitive, anchored only by
/// the appearance in a line. False positives are unlikely because these are
/// fully-qualified Rust paths that have no other plausible reading.
const FORBIDDEN_PATTERNS: &[&str] = &[
    // Direct workflow / jellyfin paths
    "media_control_lib::commands::workflow",
    "media_control_lib::jellyfin",
    // Shim leak path: workflow items re-exported under top-level `commands::*`
    // by `commands/mod.rs`. Even though those re-exports are `cli`-gated, the
    // grep test catches an attempted use that would survive workspace-wide
    // feature unification.
    "media_control_lib::commands::send_mpv_script_message",
    "media_control_lib::commands::send_mpv_script_message_with_args",
    "media_control_lib::commands::send_mpv_ipc_command",
    "media_control_lib::commands::query_mpv_property",
    "media_control_lib::commands::mark_watched",
    "media_control_lib::commands::chapter",
    "media_control_lib::commands::play",
    "media_control_lib::commands::random",
    "media_control_lib::commands::seek",
    "media_control_lib::commands::status",
    "media_control_lib::commands::keep",
];

/// Walk the daemon's `src/` directory and collect every `.rs` file.
///
/// Recursive but flat in practice — the daemon currently has only `main.rs`.
/// The recursion future-proofs against subdirectory growth.
fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    walk(root, &mut out);
    out
}

#[test]
fn daemon_source_contains_no_forbidden_imports() {
    // CARGO_MANIFEST_DIR points at the daemon crate root when this test is
    // executed by `cargo test -p media-control-daemon` or `cargo test --workspace`.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src_dir = PathBuf::from(manifest_dir).join("src");

    let files = collect_rs_files(&src_dir);
    assert!(
        !files.is_empty(),
        "boundary test found no .rs files under {}; check CARGO_MANIFEST_DIR resolution",
        src_dir.display()
    );

    let mut violations = Vec::new();

    for file in &files {
        let contents = fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));

        for (lineno, line) in contents.lines().enumerate() {
            // Skip comment lines so doc-comments referencing the patterns
            // (e.g. an explanatory paragraph in a module doc) do not trip the
            // test. Treats any line whose first non-whitespace char is `/` as
            // a comment — covers `//`, `///`, `//!`, `/*`. Source code that
            // legitimately starts with `/` (rare; a path literal would be in
            // a string or after a binding) would need a more sophisticated
            // tokenizer; we accept the simplification.
            let trimmed = line.trim_start();
            if trimmed.starts_with('/') {
                continue;
            }

            for pattern in FORBIDDEN_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{}:{}: forbidden pattern {pattern:?} in line: {}",
                        file.display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "daemon source must not import workflow / jellyfin items \
         (see ADR-001 for the contract). Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn boundary_test_self_check_patterns_are_nonempty() {
    // Sanity test: if FORBIDDEN_PATTERNS were ever emptied, the main test
    // would silently pass against any source. Catch that drift here.
    //
    // Clippy flags this as `const_is_empty` because the const's emptiness is
    // compile-time decidable — that's exactly the property we want: a
    // contributor who emptied the list would get a compile-time hint that
    // this test will always fail, which is the right signal.
    #[allow(clippy::const_is_empty)]
    let is_empty = FORBIDDEN_PATTERNS.is_empty();
    assert!(
        !is_empty,
        "FORBIDDEN_PATTERNS must list at least one pattern"
    );
}
