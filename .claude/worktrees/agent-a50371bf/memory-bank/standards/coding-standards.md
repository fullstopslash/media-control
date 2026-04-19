# Coding Standards

## Overview
Idiomatic Rust with standard tooling. Strict error handling via `thiserror`, comprehensive co-located tests, and minimal external dependencies.

## Code Formatting

**Tool**: `rustfmt` (standard Rust formatter)
**Key Settings**: Default configuration (no `rustfmt.toml` overrides)

**Enforcement**: On save (editor integration). No pre-commit hook currently.

## Linting

**Tool**: `clippy` (Rust's built-in linter)
**Strictness**: Default warnings, with targeted `#[allow(...)]` only where justified.

**Key Rules**:
- Unused imports/variables: warn (default)
- Clippy pedantic: not enabled globally
- Suppress only with justification (e.g., `#[allow(clippy::too_many_arguments)]` when refactoring is not worth the complexity)

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Variables / fields | snake_case | `focus_history_id`, `media_window` |
| Functions / methods | snake_case | `find_media_window`, `get_clients` |
| Structs / enums | PascalCase | `MediaWindow`, `HyprlandError` |
| Enum variants | PascalCase | `Direction::Left`, `ConfigErrorKind::NotFound` |
| Constants | UPPER_SNAKE | `MAX_FULLSCREEN_EXIT_ATTEMPTS`, `MPV_IPC_SOCKET_DEFAULT` |
| Modules | snake_case | `move_window`, `mark_watched` |
| Crate names | kebab-case | `media-control-lib` |
| Type aliases | PascalCase | `Result<T>` |

**File Naming**: One module per file, `snake_case.rs`. Command submodules in `commands/` directory with `mod.rs` for shared context.

## File Organization

**Pattern**: Cargo workspace with domain-based modules.

**Structure**:
```text
crates/
  media-control/              # CLI binary
    src/main.rs               # Argument parsing, command routing
  media-control-daemon/       # Daemon binary
    src/main.rs               # Event loop, PID management, FIFO listener
  media-control-lib/          # Shared library
    src/
      lib.rs                  # Module declarations
      config.rs               # TOML config parsing, defaults, overrides
      error.rs                # Structured error types (thiserror)
      hyprland.rs             # Hyprland IPC client (Unix socket)
      jellyfin.rs             # Jellyfin HTTP API client
      window.rs               # Window pattern matching, priority logic
      commands/
        mod.rs                # CommandContext, shared helpers (suppress, restore_focus)
        avoid.rs              # Smart window repositioning
        chapter.rs            # mpv chapter navigation (IPC)
        close.rs              # Graceful window closing
        focus.rs              # Focus or launch media window
        fullscreen.rs         # Fullscreen toggle with state preservation
        mark_watched.rs       # Jellyfin mark-watched integration
        move_window.rs        # Vim-style directional movement
        pin.rs                # Pin-and-float toggle
    tests/
      config_integration.rs   # Config file loading integration tests
```

**Conventions**:
- Tests: co-located `mod tests` blocks in each source file
- Integration tests: `tests/` directory at crate root
- Types: defined in the module that owns them (no separate `types.rs`)
- Re-exports: `lib.rs` declares `pub mod` for all modules

## Testing Strategy

**Framework**: Built-in `#[test]` and `#[tokio::test]`
**Coverage Target**: Critical paths tested. No numeric target enforced.

**Test Types**:

| Type | Tool | When to Use |
|------|------|-------------|
| Unit | `#[test]` | Pure functions, deserialization, pattern matching |
| Async unit | `#[tokio::test]` | Async functions (file I/O, timestamps) |
| Integration | `tests/` directory | Config loading from real files |
| Doc-tests | `/// ``` ... ``` ` | Public API examples (compile-checked) |

**Conventions**:
- Test naming: `fn descriptive_name()` (e.g., `pinned_beats_focused`, `rectangles_overlap_detects_no_overlap`)
- Test structure: Arrange-Act-Assert, inline test data
- Mock strategy: No mocking framework. Test pure logic directly. Helper functions (`make_client`, `make_client_full`) construct test data.
- Hyprland IPC: Not tested directly (requires live compositor). Tested via deserialization and logic tests.

## Error Handling

**Pattern**: `thiserror` derive macros with structured error enums.

**Structure**:
- Each domain has its own error enum (`HyprlandError`, `JellyfinError`, `ConfigError`)
- `MediaControlError` is the top-level error with `From` impls for cross-domain conversion
- `Result<T>` type aliases defined per module
- Commands propagate errors with `?` operator

**Key Patterns**:
- `From` impls for automatic error conversion between domains
- `.ok()` to intentionally ignore non-critical errors (e.g., suppress file writes)
- `let _ =` for fire-and-forget operations where failure is acceptable
- No `unwrap()` in production code paths

## Logging

**Tool**: `tracing` + `tracing-subscriber` with `EnvFilter`
**Format**: Human-readable text (not structured JSON)

**Levels**:

| Level | Usage |
|-------|-------|
| error | Daemon failures that stop the event loop |
| warn | Recoverable failures (context creation, FIFO open) |
| info | Daemon lifecycle (start, stop, socket connection) |
| debug | Event processing, avoid logic decisions, suppression state |

**Rules**:
- CLI: logging off by default, enabled with `-v` flag (debug level)
- Daemon: info level by default, configurable via `RUST_LOG` env var
- Never log: credentials, tokens, full Jellyfin API responses
- Always log: daemon start/stop, Hyprland socket connection status
