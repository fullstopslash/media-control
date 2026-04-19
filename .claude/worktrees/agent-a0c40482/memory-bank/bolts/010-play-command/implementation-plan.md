---
stage: plan
bolt: 010-play-command
created: 2026-03-19T18:00:00Z
---

## Implementation Plan: play-command

### Objective

Implement `media-control play <target>` subcommand that replaces shim-play.sh with native Rust — resolving items via Jellyfin API, sending IPC hints, and initiating playback with resume.

### Deliverables

- 3 new methods on JellyfinClient in `jellyfin.rs`
- `send_mpv_script_message_with_args()` helper in `commands/mod.rs`
- `PlayConfig` struct in `config.rs`
- `commands/play.rs` module with `PlayTarget` enum + orchestration
- `Play` variant in `main.rs` Commands enum

### Dependencies

- Existing: JellyfinClient, send_mpv_ipc_command, Config, CommandContext
- No new crate dependencies

### Technical Approach

#### 1. jellyfin.rs — 3 new methods (Story 001)

**`get_global_next_up()`**: Like existing `get_next_up()` but without series_id path segment:
```
GET {server}/Shows/NextUp?UserId={user_id}&Limit=1
```
Returns `Option<String>` (item ID or None). Reuse existing `NextUpResponse` struct.

**`get_item_resume_ticks(item_id)`**: New endpoint:
```
GET {server}/Users/{user_id}/Items/{item_id}
```
New structs: `ItemDetail { id, user_data: Option<UserData> }`, `UserData { playback_position_ticks }`.
Returns `i64` (0 if no UserData).

**`play_item_with_resume(session_id, item_id, start_ticks)`**: Extend existing `play_item()` pattern:
```
POST {server}/Sessions/{session_id}/Playing?PlayCommand=PlayNow&ItemIds={item_id}[&StartPositionTicks={ticks}]
```
Only append StartPositionTicks when non-zero.

#### 2. commands/mod.rs — multi-arg IPC helper (Story 002)

```rust
pub async fn send_mpv_script_message_with_args(message: &str, args: &[&str]) -> Result<()> {
    let mut parts: Vec<&str> = vec!["script-message", message];
    parts.extend_from_slice(args);
    let payload = serde_json::json!({"command": parts}).to_string();
    send_mpv_ipc_command(&payload).await
}
```

#### 3. config.rs — PlayConfig (Story 003)

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlayConfig {
    pub pinchflat_library_id: Option<String>,
}
```
Add `pub play: PlayConfig` to Config struct with `#[serde(default)]`.

#### 4. commands/play.rs — command module (Story 004)

- `PlayTarget` enum: `NextUp`, `RecentPinchflat`, `ItemId(String)`
- `PlayTarget::parse(s: &str) -> Self`
- `pub async fn play(ctx: &CommandContext, target_str: &str) -> Result<()>`
- Flow: resolve item ID → send IPC hint (non-fatal) → get resume ticks → find session → PlayNow
- Add `pub mod play;` to commands/mod.rs

#### 5. main.rs — CLI wiring (Story 005)

Add to Commands enum:
```rust
Play {
    /// What to play: next-up, recent-pinchflat, or an item ID
    target: String,
},
```
Add match arm: `Commands::Play { target } => commands::play::play(&ctx, &target).await?`

### Acceptance Criteria

- [-] `media-control play next-up` resolves and plays first NextUp item <!-- tw:4f24f9d3-6927-44db-882e-868177984ad6 -->
- [-] `media-control play recent-pinchflat` resolves and plays most recent unwatched <!-- tw:d046547d-0297-4177-97fc-2cd499513d0e -->
- [-] `media-control play <item-id>` plays specific item directly <!-- tw:75da3f55-22b3-48d0-bd9e-3e40820848f6 -->
- [-] Playback resumes from last position (StartPositionTicks) <!-- tw:f57eb161-0987-4029-8f93-9691ecb8281f -->
- [-] IPC hint sent before PlayNow (non-fatal on failure) <!-- tw:2a6412b3-1533-4ae0-a5ba-0972e309d80d -->
- [-] Errors produce stderr + notify-send <!-- tw:b2f3a06a-f0e1-4688-8fc8-2d6521339475 -->
- [-] `cargo clippy` and `cargo test` pass <!-- tw:8e28d697-fff2-473d-ac85-2e4badbebba2 -->
