# Global Story Index

## Overview
- **Total stories**: 33
- **Generated**: 30
- **Completed**: 3
- **Last updated**: 2026-03-19

---

## Stories by Intent

### 001-test-and-refactor

**Unit: 001-mock-infrastructure** (3 stories)
- [x] **001-mock-server** (mock-infrastructure): Mock Hyprland socket server - Must - ✅ GENERATED <!-- tw:4e8f55a4-42da-4453-ac4c-93304bebf7fb -->
- [x] **002-command-capture** (mock-infrastructure): Command capture and assertion - Must - ✅ GENERATED <!-- tw:4ea8c137-b120-45d2-87dd-c6908d109e3e -->
- [x] **003-test-context** (mock-infrastructure): CommandContext test constructor - Must - ✅ GENERATED <!-- tw:d29c33f7-e393-4f08-820d-0dc5f979eb0b -->

**Unit: 002-test-coverage** (5 stories)
- [x] **001-avoid-tests** (test-coverage): Avoid command E2E + edge cases - Must - ✅ GENERATED <!-- tw:d7bc6433-1c65-40c5-89e1-d237f00189ee -->
- [x] **002-fullscreen-tests** (test-coverage): Fullscreen command E2E + edge cases - Must - ✅ GENERATED <!-- tw:bc30b290-4e1f-4f56-9f24-9f3e188a1ce2 -->
- [x] **003-simple-command-tests** (test-coverage): Move, pin, close, focus E2E - Must - ✅ GENERATED <!-- tw:a89324c4-cb55-4725-ad11-5f857bf9266b -->
- [x] **004-edge-cases** (test-coverage): Cross-cutting edge cases - Should - ✅ GENERATED <!-- tw:c621b20c-db0a-4847-affa-6bc39e9735c9 -->
- [x] **005-daemon-tests** (test-coverage): Daemon debounce and lifecycle - Should - ✅ GENERATED <!-- tw:beab0572-2532-4d7d-9ebf-b09e172da5db -->

**Unit: 003-logic-cleanup** (3 stories)
- [x] **001-simplify-avoid** (logic-cleanup): Simplify avoid command logic - Must - ✅ GENERATED <!-- tw:5655d467-7c67-499e-91dc-eaf31e8beb14 -->
- [x] **002-simplify-fullscreen-close** (logic-cleanup): Simplify fullscreen and close - Must - ✅ GENERATED <!-- tw:86df3029-fb6e-4d6a-bc76-a22e5598ce01 -->
- [x] **003-error-consistency** (logic-cleanup): Error handling consistency pass - Should - ✅ GENERATED <!-- tw:5e2b75ad-54d1-43ee-b493-bb7797254eb1 -->

### 004-ipc-reliability

**Unit: 001-ipc-hardening** (5 stories)
- [x] **001-socket-validation** (ipc-hardening): Socket path validation before connect - Must - ✅ GENERATED <!-- tw:d193a65e-16cd-4ea4-a4c3-bf07f29ae883 -->
- [x] **002-connection-timeout** (ipc-hardening): 500ms timeout on connect+write - Must - ✅ GENERATED <!-- tw:d49e061b-74e0-4917-8411-47d8024b53a7 -->
- [x] **003-response-verification** (ipc-hardening): Read and verify mpv IPC response - Should - ✅ GENERATED <!-- tw:48404705-a733-4103-b346-fd5e5c288e17 -->
- [x] **004-stale-socket-retry** (ipc-hardening): Retry once after 100ms on total failure - Should - ✅ GENERATED <!-- tw:bc4bebd6-4be7-4011-a39e-f3926f086490 -->
- [x] **005-error-feedback** (ipc-hardening): Error to stderr + notify-send + exit code - Must - ✅ GENERATED <!-- tw:79f4afe7-019e-41c1-9ae7-79fe3793449f -->

### 005-play-subcommand

**Unit: 001-play-command** (5 stories)
- [x] **001-jellyfin-methods** (play-command): 3 new Jellyfin API methods - Must - ✅ GENERATED <!-- tw:14a62246-41c1-4c8a-9810-f1506a5d4fd4 -->
- [x] **002-multi-arg-ipc** (play-command): Multi-arg script-message helper - Must - ✅ GENERATED <!-- tw:567b3553-e4bc-4bae-88ef-ad8efcdce84c -->
- [x] **003-play-config** (play-command): PlayConfig struct for config.toml - Must - ✅ GENERATED <!-- tw:b5ae0d94-3ce1-46e5-ac86-26b194bd7bc0 -->
- [x] **004-play-command** (play-command): play.rs command module with PlayTarget - Must - ✅ GENERATED <!-- tw:34fdb369-7e50-4ef8-aa5d-0c7efc2254a9 -->
- [x] **005-cli-wiring** (play-command): Wire Play into main.rs CLI - Must - ✅ GENERATED <!-- tw:5deb3294-2b45-49f4-bef0-21b93f479275 -->

### 006-status-subcommand

**Unit: 001-status-command** (3 stories)
- [x] **001-query-mpv-property** (status-command): Query mpv property with response - Must - ✅ GENERATED <!-- tw:ffcafaf3-c075-45f6-bfe6-d44e4b030686 -->
- [x] **002-status-command** (status-command): Status command module - Must - ✅ GENERATED <!-- tw:e0f435c4-3248-473d-9b1d-7ffcf4c7f6a8 -->
- [x] **003-cli-wiring** (status-command): Wire Status into main.rs with --json - Must - ✅ GENERATED <!-- tw:7cdd199b-34dd-4c23-a49e-87073741417d -->

### 007-jellyfin-error-handling

**Unit: 001-jellyfin-error-handling** (2 stories)
- [x] **001-get-error-status** (jellyfin-error-handling): Add error_for_status to GET requests - Must - ✅ GENERATED <!-- tw:3608e2a0-0899-46ad-919e-d5b08369ab2e -->
- [x] **002-resume-error-logging** (jellyfin-error-handling): Log resume ticks errors in play.rs - Must - ✅ GENERATED <!-- tw:48ee7788-725d-443a-bb1d-2baa8f488f4b -->

### 008-daemon-reliability

**Unit: 001-daemon-signals** (1 story)
- [x] **001-sigterm-handling** (daemon-signals): Handle SIGTERM for clean daemon shutdown - Must - GENERATED <!-- tw:e184cda4-baeb-4ff2-9f21-b22f80f97181 -->

### 010-config-window-hardening

**Unit: 001-config-window-fixes** (3 stories)
- [x] **Given** no config file exists and no --config flag, **When** media-control runs, **Then** it uses successfully <!-- tw:144fe534-0618-468e-ab47-7fe024eaf310 -->
- [x] **Given** a hidden mpv window (hidden=true), **When** find_media_window runs, **Then** it is not returned <!-- tw:df24d077-1d03-428d-939d-c10f2c3a92e1 -->
- [x] **Given** windows with focus_history_id [0, 2, -1], **When** find_media_windows sorts, **Then** order is [0, 2, -1] <!-- tw:ddcf42e7-d8a4-4bdb-a196-169e575101bc -->

### 009-error-propagation

**Unit: 001-error-propagation** (3 stories)
- [x] **Given** `move_media_window` calls `ctx.hyprland.batch()`, **When** the batch fails, **Then** the error propagates to the caller via `?` <!-- tw:ade2bb5f-7073-4e15-9bc7-4e6ca9ec52b4 -->
- [x] **Given** `send_mpv_script_message("stop-and-clear")` fails, **When** close is called for an mpv window, **Then** the error is propagated or logged as a warning <!-- tw:7049e5a1-6e2e-41c8-bd43-7513461c4cfd -->
- [x] **Given** the batch call for repositioning after fullscreen exit, **When** it fails, **Then** the error propagates via `?` <!-- tw:4442c19b-a780-4b03-8521-507713f7fa17 -->

---

## Stories by Status

- **Planned**: 0
- **Generated**: 30
- **In Progress**: 0
- **Completed**: 3
