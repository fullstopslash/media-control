# Shim IPC Integration Plan

The jellyfin-mpv-shim fork at ~/projects/jellyfin-mpv-shim-fork now handles all
Jellyfin interaction natively. media-control should delegate to it via mpv IPC
instead of making its own Jellyfin API calls.

## Available IPC Commands

All commands sent via mpv Unix socket at `/tmp/mpvctl-jshim`:

```bash
echo '{"command":["script-message","COMMAND"]}' | socat - /tmp/mpvctl-jshim
```

| Command | What it does |
|---------|-------------|
| `mark-watched-next` | Mark watched + advance via per-library strategy |
| `mark-watched` | Mark current item watched (no advance) |
| `stop-and-clear` | Stop playback, clear session (mpv stays alive) |
| `skip-next` | Skip to next item via strategy (no mark watched) |
| `skip-prev` | Skip to previous item via strategy (no mark watched) |
| `play-next-strategy` | Advance via strategy without marking |

## Changes Needed in media-control

### 1. `mark_watched` command (mark_watched.rs:38-57)

**Current:** Creates JellyfinClient, calls `mark_current_watched()` via Jellyfin API.
**Change to:** `send_mpv_script_message("mark-watched").await`

This is faster (IPC vs HTTP round-trip) and ensures the shim's cache is invalidated.

### 2. `mark_watched_and_stop` command (mark_watched.rs:72-97)

**Current:** Creates JellyfinClient, calls `mark_watched_and_stop()`, then `playerctl stop`.
**Change to:**
```rust
send_mpv_script_message("mark-watched").await?;
send_mpv_script_message("stop-and-clear").await
```

### 3. New `skip_next` / `skip_prev` commands

Add two new commands that delegate to the shim:

```rust
pub async fn skip_next(_ctx: &CommandContext) -> Result<()> {
    send_mpv_script_message("skip-next").await
}

pub async fn skip_prev(_ctx: &CommandContext) -> Result<()> {
    send_mpv_script_message("skip-prev").await
}
```

Register in the CLI:
```
media-control skip-next
media-control skip-prev
```

### 4. Update Hyprland bindings (local.conf)

Replace raw socat calls with media-control commands:
```
bind = $mainMod CTRL, bracketright, exec, $media skip-next
bind = $mainMod CTRL, bracketleft, exec, $media skip-prev
```

### 5. Deprecate Jellyfin API code

After switching to IPC, these are no longer needed for mpv:
- `JellyfinClient::mark_current_watched()`
- `JellyfinClient::mark_watched_and_stop()`
- `JellyfinClient::stop_mpv()`
- `JellyfinClient::from_default_credentials()` (only used by mark_watched)
- All the `NextEpisodeConfig` / strategy code (already unused since shim handles it)

Keep `JellyfinClient` for non-mpv use cases if any exist.

### 6. Fallback behavior

If the mpv IPC socket doesn't exist (shim not running), fall back to the
current Jellyfin API approach. The `send_mpv_script_message` already returns
`Err` when no socket is found — handle it gracefully.

## Summary

| Command | Before | After |
|---------|--------|-------|
| mark-watched | Jellyfin HTTP API (~200ms) | IPC mark-watched (~2ms) |
| mark-watched-and-stop | Jellyfin API + playerctl (~400ms) | IPC mark-watched + stop-and-clear (~4ms) |
| mark-watched-and-next | IPC (already done) | No change |
| close (mpv) | IPC (already done) | No change |
| skip-next | Raw socat in Hyprland | media-control command → IPC |
| skip-prev | Raw socat in Hyprland | media-control command → IPC |
