# Intent: `media-control status`

## Feature

A `media-control status` command that queries the shim's mpv IPC socket and outputs current playback state. Default output is human-readable; `--json` flag emits machine-parseable JSON.

### Output fields

| Field | mpv source | Notes |
|-------|-----------|-------|
| Item name | `media-title` property | |
| Series/season/episode | `media-title` (parse) or `user-data` | Shim sets `media-title` to e.g. "Show S01E03 - Title". May need a new shim IPC query if structured data is wanted. |
| Playback position | `playback-time` property | Seconds as float |
| Duration | `duration` property | Seconds as float |
| Paused | `pause` property | Boolean |
| Play source | Needs shim support | nextup/strategy/external — shim knows this but doesn't expose it via mpv property yet |

### Example output

```
Playing: Silo S02E05 - Descent
Position: 12:34 / 45:01
Source: strategy
Paused: no
```

```json
{"title":"Silo S02E05 - Descent","position":754.2,"duration":2701.0,"paused":false,"source":"strategy"}
```

## Use cases

- **Waybar/polybar module**: Show current episode in status bar via `media-control status --json`
- **Scripting**: Conditional logic based on playback state (e.g., auto-lock only when nothing is playing)
- **Debugging**: Quick check of what the shim thinks is playing

## Implementation notes

### Rust side (media-control)

- Add `Status` variant to `Commands` enum with `--json` flag
- Send `get_property` commands to mpv IPC socket at `/tmp/mpvctl-jshim`:
  ```json
  {"command":["get_property","media-title"]}
  {"command":["get_property","playback-time"]}
  {"command":["get_property","duration"]}
  {"command":["get_property","pause"]}
  ```
- mpv IPC supports multiple commands on one connection (one JSON per line, one response per line) — send all four and read four responses
- Refactor `send_mpv_ipc_command` to return the response string instead of discarding it, or add a parallel `query_mpv_property` function that returns `serde_json::Value`
- No Hyprland dependency needed — skip the `get_media_window` check. If the socket isn't there, exit with a clear "not playing" status (exit code 1 or empty JSON)

### Shim side (jellyfin-mpv-shim-fork)

- **Optional but nice**: Add a `get-status` script-message handler that returns structured JSON with series/season/episode/source in one shot, avoiding title parsing
- Alternatively, set `user-data/jellyfin-status` on the mpv player with structured metadata so the Rust side can read it with a single `get_property` call

### Socket interaction

Reuse the existing socket discovery logic (`$MPV_IPC_SOCKET` -> `/tmp/mpvctl-jshim` -> `/tmp/mpvctl0`). The `status` command should be fast — single attempt, no retry, short timeout (200ms total).

### Exit behavior

- Playing: exit 0, print status
- Not playing (no socket / no file loaded): exit 1, print nothing (or `{"playing":false}` with `--json`)
- This makes waybar integration trivial: `exec: media-control status --json`
