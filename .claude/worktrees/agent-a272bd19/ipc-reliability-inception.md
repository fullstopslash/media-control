<original_task>
Fix unreliable and slow IPC command delivery from media-control to jellyfin-mpv-shim.

Commands like mark-watched-and-next, skip-next, skip-prev take 3+ seconds or don't
go through at all. The root cause is that media-control sends commands via Unix socket
(/tmp/mpvctl-jshim) to mpv's IPC, but:

1. mpv frequently dies and respawns, leaving a stale/broken socket
2. Other tools (socat, debugging) can overwrite the socket with a regular file
3. send_mpv_script_message() has no timeout, no socket validation, no error feedback
4. The user gets zero indication when a command fails — it just silently does nothing
</original_task>

<work_completed>
media-control already has the correct architecture:
- Keypress → Hyprland binding → media-control CLI → mpv IPC socket → jellyfin-mpv-shim
- send_mpv_script_message() in crates/media-control-lib/src/commands/mod.rs:241-285
- Socket discovery: $MPV_IPC_SOCKET → /tmp/mpvctl-jshim → /tmp/mpvctl0
- Commands sent as JSON: {"command":["script-message","mark-watched-next"]}

jellyfin-mpv-shim handles these script-messages in player.py IPC_COMMANDS dict:
- mark-watched-next → mark_watched_and_advance()
- skip-next → _skip_item(forward=True)
- skip-prev → _skip_item(forward=False)
- stop-and-clear → stop()
</work_completed>

<work_remaining>
## Problem 1: No socket validation
send_mpv_script_message() does UnixStream::connect() without checking if the path
is actually a Unix socket. If it's a regular file (left by socat or other tools),
tokio may hang or error slowly.

FIX: stat() the path before connecting. If it's not a socket (S_ISSOCK), skip it
and try the next path. Log a warning.

## Problem 2: No connection timeout
If the socket exists but mpv is dead/unresponsive, connect() can hang indefinitely.

FIX: Add tokio::time::timeout (500ms) around the connect + write. If it times out,
try the next socket path. Return an error if all paths fail.

## Problem 3: No error feedback to user
send_mpv_script_message() returns Result<()> but the callers in mark_watched.rs
silently swallow errors. The user has no idea the command failed.

FIX: Propagate errors to main(). Print a brief error to stderr. Exit with non-zero
code. Consider a desktop notification (notify-send) for failed commands.

## Problem 4: No response verification
The function sends the JSON command but never reads a response. mpv IPC returns a
JSON response for every command. Reading the response would confirm:
- The command was received
- The command was valid
- The command completed (or errored)

FIX: Read the response with a short timeout (200ms). Log warnings on error responses.
This also catches cases where the socket connects but mpv's IPC handler is broken.

## Problem 5: Stale socket after mpv respawn
When mpv dies and respawns, the old socket may be gone and a new one created. The
shim's mpv is configured with --input-ipc-server=/tmp/mpvctl-jshim, so the socket
should be recreated on respawn. But there's a race window where the socket doesn't
exist yet.

FIX: If connect fails, wait briefly (100ms) and retry once. mpv respawn takes
~100-500ms based on logs.
</work_remaining>

<attempted_approaches>
- Lock optimization in jellyfin-mpv-shim (bolt 020) — reduced lock scope in _play_media
  so mark_watched_and_advance can acquire the lock faster. This helps but doesn't fix
  the IPC delivery problem.
- Timing instrumentation added to mark_watched_and_advance — will show where time is
  spent once commands actually arrive.
- mpv death diagnostics (bolt 022) — structured logging, quit-vs-crash classification,
  stderr buffer. Shows mpv dying frequently ("Exiting... (Quit)").
</attempted_approaches>

<critical_context>
## Key files in media-control
- crates/media-control-lib/src/commands/mod.rs:241-285 — send_mpv_script_message()
- crates/media-control-lib/src/commands/mark_watched.rs — command handlers
- crates/media-control/src/main.rs — CLI entry, error handling

## Socket paths
- /tmp/mpvctl-jshim — jellyfin-mpv-shim's mpv IPC socket
- /tmp/mpvctl0 — fallback for regular mpv instances
- $MPV_IPC_SOCKET — env var override

## jellyfin-mpv-shim IPC handler (player.py)
IPC_COMMANDS = {
    "mark-watched-next": mark_watched_and_advance,
    "mark-watched": lambda: video.set_played(),
    "stop-and-clear": stop,
    "skip-next": _skip_item(forward=True),
    "skip-prev": _skip_item(forward=False),
    "play-next-strategy": _advance_without_marking,
}

## Timing data needed
The timing instrumentation in jellyfin-mpv-shim will show us:
- Lock acquisition time (should be <50ms after bolt 020)
- Strategy resolution time
- API call time for next item
- mpv load time
But we need the IPC to actually work first.

## User preferences
- Uses Hyprland on Arch Linux
- media-control is a personal Rust project at ~/projects/media-control/
- Uses jj for version control, fj for Forgejo
- Desktop notifications via notify-send are acceptable
- Wants sub-second response from keypress to action
</critical_context>

<current_state>
## What works
- media-control correctly identifies the mpv window via Hyprland IPC
- media-control correctly constructs the JSON command
- Socket path discovery order is correct

## What's broken
- Commands take 3+ seconds or silently fail
- mpv dies frequently, leaving broken sockets
- No timeout on socket operations
- No validation that socket path is actually a socket
- No error feedback to user
- No response reading from mpv IPC

## Environment
- Rust workspace with tokio async runtime
- mpv IPC is a Unix domain socket
- mpv respawns automatically via jellyfin-mpv-shim's _ensure_mpv()
</current_state>
