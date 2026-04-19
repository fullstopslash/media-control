---
intent: play-subcommand
phase: inception
status: draft
created: 2026-03-19
---

# Intent: `play` Subcommand

Replace `shim-play.sh` (6 curl calls + python parsing, ~1-2s) with native Rust subcommands in media-control (~50-100ms).

## Motivation

`shim-play.sh` is the primary way to start playback from Hyprland keybindings. It is slow and fragile:
- 6 serial HTTP requests with shell-level orchestration
- Python subprocess for JSON parsing on every call
- No error handling beyond `set -euo pipefail`
- Hardcoded library IDs in bash
- Resume position logic duplicated outside the shim

media-control already has `JellyfinClient` with credentials loading, session discovery, `play_item()`, and mpv IPC. The `play` subcommand wires these together.

## Feature Spec

### CLI Interface

```
media-control play next-up              # First NextUp item across all shows
media-control play recent-pinchflat     # Most recent unwatched Pinchflat video
media-control play <item-id>            # Play specific item by ID (hex/GUID)
```

### What each invocation does (3 steps, all in-process)

1. **Resolve item ID** -- query Jellyfin for the target item
2. **Send IPC hint** -- tell the shim which play-source context to use for skip/advance
3. **Send PlayNow** -- tell Jellyfin to play the item in the shim's session, with resume position

All three steps reuse existing `JellyfinClient` and `send_mpv_script_message` infrastructure.

## Functional Requirements

### FR-1: Resolve item -- `next-up`
- `GET /Shows/NextUp?UserId={user_id}&Limit=1`
- Return first item's ID, or error "No next-up item found"
- `JellyfinClient` already has `get_next_up()` but it takes a `series_id`; we need a global variant without `series_id` in the path: `GET /Shows/NextUp?UserId={user_id}&Limit=1`
- Priority: Must

### FR-2: Resolve item -- `recent-pinchflat`
- `GET /Users/{user_id}/Items?ParentId={pinchflat_lib}&IsPlayed=false&Recursive=true&SortBy=DateCreated&SortOrder=Descending&Limit=1&IncludeItemTypes=Episode,Video`
- `JellyfinClient` already has `get_unwatched_items()` which does exactly this
- The Pinchflat library ID must come from config (not hardcoded)
- Priority: Must

### FR-3: Resolve item -- direct `<item-id>`
- No API call needed, use the ID directly
- Validate it looks like a Jellyfin ID (hex string, 32 chars) -- warn but don't reject if format is unexpected
- Priority: Must

### FR-4: Send IPC play-source hint
- Before sending PlayNow, send `set-play-source` via mpv IPC to tell the shim what context this playback belongs to
- `next-up` -> `{"command":["script-message","set-play-source","nextup"]}`
- `recent-pinchflat` -> `{"command":["script-message","set-play-source","strategy"]}`
- Direct item-id -> `{"command":["script-message","set-play-source","strategy"]}` (default)
- Use existing `send_mpv_script_message` but it currently only sends single-arg messages; need to extend it or use `send_mpv_ipc_command` directly with the full JSON
- Failure is non-fatal (shim may not be running yet); log warning and continue
- Priority: Must

### FR-5: Get resume position
- `GET /Users/{user_id}/Items/{item_id}` -> `UserData.PlaybackPositionTicks`
- If non-zero, append `&StartPositionTicks={ticks}` to the PlayNow URL
- Priority: Must

### FR-6: Find shim session and send PlayNow
- Use existing `find_mpv_session()` to get the session ID
- `POST /Sessions/{session_id}/Playing?PlayCommand=PlayNow&ItemIds={item_id}[&StartPositionTicks={ticks}]`
- This is exactly `play_item()` but with optional `StartPositionTicks`; extend `play_item()` or add `play_item_with_resume()`
- If no session found, exit with error + notify-send "Shim not connected"
- Priority: Must

### FR-7: Library ID configuration
- Add `[play]` section to `config.toml`:
  ```toml
  [play]
  pinchflat_library_id = "a5c0a87b1d058d1b7e70f5406ee274e2"
  ```
- Required only when `recent-pinchflat` is used; other subcommands work without it
- Priority: Must

### FR-8: Error reporting
- On failure: stderr message + `notify-send` (same pattern as existing `main()`)
- Specific error messages: "No next-up item found", "Shim not connected", "No Pinchflat library configured"
- Priority: Must

## API Endpoints Needed

| Endpoint | Method | Used For | Existing? |
|----------|--------|----------|-----------|
| `GET /Shows/NextUp?UserId={}&Limit=1` | GET | FR-1: global next-up | **New** (existing `get_next_up` is per-series) |
| `GET /Users/{}/Items?ParentId={}&...` | GET | FR-2: recent pinchflat | **Exists** (`get_unwatched_items`) |
| `GET /Users/{}/Items/{}` | GET | FR-5: resume position | **New** |
| `GET /Sessions` | GET | FR-6: find session | **Exists** (`find_mpv_session`) |
| `POST /Sessions/{}/Playing?...` | POST | FR-6: play item | **Exists** (`play_item`) but needs resume ticks param |

## Rust Implementation Approach

### Files to modify

1. **`crates/media-control-lib/src/jellyfin.rs`** -- add 3 methods:
   - `get_global_next_up() -> Result<Option<String>>` -- NextUp without series_id
   - `get_item_resume_ticks(item_id) -> Result<i64>` -- fetch UserData.PlaybackPositionTicks
   - `play_item_with_resume(session_id, item_id, start_ticks) -> Result<()>` -- PlayNow with optional StartPositionTicks

2. **`crates/media-control-lib/src/config.rs`** -- add `PlayConfig` struct:
   ```rust
   #[derive(Debug, Clone, Deserialize, Default)]
   pub struct PlayConfig {
       pub pinchflat_library_id: Option<String>,
   }
   ```
   Add `play: PlayConfig` field to `Config`.

3. **`crates/media-control-lib/src/commands/play.rs`** -- new command module:
   - `play(ctx, target)` async function
   - `PlayTarget` enum: `NextUp`, `RecentPinchflat`, `ItemId(String)`
   - Orchestrates: resolve item -> IPC hint -> resume ticks -> PlayNow

4. **`crates/media-control-lib/src/commands/mod.rs`** -- add `pub mod play;`

5. **`crates/media-control/src/main.rs`** -- add `Play` variant to `Commands` enum:
   ```rust
   Play {
       /// What to play: next-up, recent-pinchflat, or an item ID
       target: String,
   },
   ```

### IPC hint implementation detail

`send_mpv_script_message` currently formats as:
```rust
format!(r#"{{"command":["script-message","{message}"]}}"#)
```

For `set-play-source`, we need two args: `set-play-source` and the source name. Options:
- **Option A**: Add `send_mpv_script_message_with_args(msg, &[args])` that builds `["script-message","set-play-source","nextup"]`
- **Option B**: Use `send_mpv_ipc_command` directly with hand-built JSON
- **Recommended**: Option A, cleaner and reusable

### New struct for item details response

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemDetail {
    id: String,
    user_data: Option<UserData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserData {
    playback_position_ticks: i64,
}
```

## Execution Flow

```
media-control play next-up
  |
  +-- JellyfinClient::from_default_credentials()     [reuse]
  +-- JellyfinClient::get_global_next_up()            [new]  -> item_id
  +-- send_mpv_script_message_with_args(                      [new helper]
  |       "set-play-source", &["nextup"])
  +-- JellyfinClient::get_item_resume_ticks(item_id)  [new]  -> ticks
  +-- JellyfinClient::find_mpv_session()              [reuse] -> session_id
  +-- JellyfinClient::play_item_with_resume(           [new]
          session_id, item_id, ticks)
```

Total: 3 HTTP requests (NextUp + item detail + PlayNow) + 1 IPC write. Down from 6 curl calls.

## Migration Plan

### Phase 1: Implement and test
- Add the `play` subcommand behind the existing CLI
- Test manually: `media-control play next-up`, `media-control play recent-pinchflat`
- Verify resume position works, IPC hint arrives, playback starts

### Phase 2: Switch Hyprland bindings
Change in `~/.config/hypr/hyprland.conf`:
```
# Before
bind = $mainMod, XF86AudioPlay, exec, ~/.config/hypr/scripts/shim-play.sh recent-pinchflat
bind = $mainMod CTRL, XF86AudioPlay, exec, ~/.config/hypr/scripts/shim-play.sh next-up

# After
bind = $mainMod, XF86AudioPlay, exec, media-control play recent-pinchflat
bind = $mainMod CTRL, XF86AudioPlay, exec, media-control play next-up
```

### Phase 3: Remove shim-play.sh
- Delete `~/.config/hypr/scripts/shim-play.sh`
- Remove any references to the old script

## Non-Goals

- No daemon mode -- this is a one-shot CLI invocation per keypress
- No queue building -- this plays a single item; the shim handles queue/advance after that
- No UI/picker -- just plays the first result; a future `play pick` could add fzf-style selection
- No `play_items()` (multi-item) -- `play_item_with_resume()` is sufficient for single-item playback

## Testing

- Unit test: `PlayTarget` parsing from string
- Unit test: resume ticks deserialization from JSON
- Integration: mock Jellyfin responses for next-up, items query, item detail
- Manual: end-to-end with real Jellyfin server, verify <200ms total latency

## Estimated Effort

- ~150 lines new Rust code (play.rs command module)
- ~50 lines additions to jellyfin.rs (3 new methods)
- ~10 lines config.rs (PlayConfig struct)
- ~15 lines main.rs (CLI wiring)
- Total: ~225 lines, small scope, high leverage
