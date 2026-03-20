---
stage: plan
bolt: 011-status-command
created: 2026-03-19T19:00:00Z
---

## Implementation Plan: status-command

### Objective

Implement `media-control status [--json]` that queries mpv IPC for playback properties and outputs current state.

### Deliverables

- `query_mpv_property()` function in `commands/mod.rs`
- `commands/status.rs` module with dual output format
- `Status` CLI variant in `main.rs` with `--json` flag

### Dependencies

- Existing: socket discovery constants, socket validation logic
- No new crate dependencies

### Technical Approach

#### 1. commands/mod.rs — query_mpv_property (Story 001)

New function that connects to mpv, sends a `get_property` command, and returns the response data:

```rust
pub async fn query_mpv_property(property: &str) -> Result<serde_json::Value>
```

- Reuse socket discovery (env var → default → fallback)
- Reuse socket validation (stat, is_socket)
- Single attempt, no retry — status should be fast or fail
- 200ms total timeout for connect+write+read
- Parse response JSON, extract `data` field
- Return `MpvIpc` error if no socket or property unavailable

#### 2. commands/status.rs — status command (Story 002)

```rust
pub async fn status(json_output: bool) -> Result<()>
```

- Query 4 properties: `media-title`, `playback-time`, `duration`, `pause`
- If any query fails (no socket), handle as "not playing"
- Human output: `Playing: {title}\nPosition: {mm:ss} / {mm:ss}\nPaused: {yes/no}`
- JSON output: `{"title","position","duration","paused","playing":true}`
- Not playing: exit 1, no output (or `{"playing":false}` with --json)
- No CommandContext needed — status doesn't use Hyprland or config
- Helper: `format_time(seconds: f64) -> String` for MM:SS formatting

#### 3. main.rs — CLI wiring (Story 003)

Add to Commands enum:
```rust
Status {
    #[arg(long)]
    json: bool,
},
```

Route before config loading — status doesn't need config/context:
```rust
Commands::Status { json } => {
    commands::status::status(json).await?;
}
```

### Acceptance Criteria

- [ ] `media-control status` shows human-readable playback state <!-- tw:5a1a3c5f-1516-4a7d-b2c1-de34515cc734 -->
- [ ] `media-control status --json` emits valid JSON <!-- tw:0c3b0948-c0eb-437e-bedc-1f1fd83f3e0d -->
- [ ] Exit 0 when playing, exit 1 when not playing <!-- tw:94cea613-00c1-4b9e-b233-347faaefc0c0 -->
- [ ] Response time < 50ms on local socket <!-- tw:cbd14950-c005-455d-83dd-94eb34bfb2e9 -->
- [ ] `cargo clippy` and `cargo test` pass <!-- tw:8afeca2a-f1ce-41a9-98d2-b1332b9000d2 -->
