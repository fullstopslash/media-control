<original_task>
Implement `media-control play` subcommand to replace shim-play.sh. Then update
Hyprland keybindings to use it.
</original_task>

<work_completed>
media-control already has:
- JellyfinClient with credentials from ~/.config/jellyfin-mpv-shim/cred.json
- Session discovery (find_mpv_session)
- play_item() via POST /Sessions/{id}/Playing
- send_mpv_script_message() for IPC to mpv
- send_mpv_ipc_command() for raw JSON IPC
- Config from ~/.config/media-control/config.toml

The detailed implementation spec is at: intents/play-subcommand.md
</work_completed>

<work_remaining>
## 1. Add 3 new methods to jellyfin.rs

### get_global_next_up() -> Result<Option<String>>
```
GET /Shows/NextUp?UserId={user_id}&Limit=1
```
Return first item's Id or None.

### get_item_resume_ticks(item_id: &str) -> Result<i64>
```
GET /Users/{user_id}/Items/{item_id}
```
Return UserData.PlaybackPositionTicks (0 if absent).

### play_item_with_resume(session_id, item_id, start_ticks) -> Result<()>
```
POST /Sessions/{session_id}/Playing?PlayCommand=PlayNow&ItemIds={item_id}[&StartPositionTicks={ticks}]
```
Like existing play_item() but with optional StartPositionTicks.

## 2. Add send_mpv_script_message_with_args to commands/mod.rs

Extend to support multi-arg script-messages:
```rust
pub async fn send_mpv_script_message_with_args(message: &str, args: &[&str]) -> Result<()> {
    let parts: Vec<String> = std::iter::once("script-message".to_string())
        .chain(std::iter::once(message.to_string()))
        .chain(args.iter().map(|a| a.to_string()))
        .collect();
    let payload = serde_json::json!({"command": parts}).to_string();
    send_mpv_ipc_command(&payload).await
}
```

## 3. Add PlayConfig to config.rs

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlayConfig {
    pub pinchflat_library_id: Option<String>,
}
```
Add `pub play: PlayConfig` to the Config struct with `#[serde(default)]`.

## 4. Add config.toml entry

Add to ~/.config/media-control/config.toml:
```toml
[play]
pinchflat_library_id = "a5c0a87b1d058d1b7e70f5406ee274e2"
```

## 5. Create commands/play.rs

```rust
use crate::commands::{send_mpv_script_message_with_args, CommandContext};
use crate::jellyfin::JellyfinClient;
use anyhow::{bail, Result};

pub enum PlayTarget {
    NextUp,
    RecentPinchflat,
    ItemId(String),
}

impl PlayTarget {
    pub fn parse(s: &str) -> Self {
        match s {
            "next-up" => Self::NextUp,
            "recent-pinchflat" => Self::RecentPinchflat,
            id => Self::ItemId(id.to_string()),
        }
    }
}

pub async fn play(ctx: &CommandContext, target_str: &str) -> Result<()> {
    let target = PlayTarget::parse(target_str);
    let jf = JellyfinClient::from_default_credentials()?;

    // Step 1: Resolve item ID
    let item_id = match &target {
        PlayTarget::NextUp => {
            jf.get_global_next_up().await?
                .ok_or_else(|| anyhow::anyhow!("No next-up item found"))?
        }
        PlayTarget::RecentPinchflat => {
            let lib_id = ctx.config.play.pinchflat_library_id.as_ref()
                .ok_or_else(|| anyhow::anyhow!("No pinchflat_library_id in config.toml [play] section"))?;
            jf.get_recent_unwatched(lib_id).await?
                .ok_or_else(|| anyhow::anyhow!("No unwatched Pinchflat videos found"))?
        }
        PlayTarget::ItemId(id) => id.clone(),
    };

    // Step 2: Send IPC play-source hint
    let source = match &target {
        PlayTarget::NextUp => "nextup",
        _ => "strategy",
    };
    let _ = send_mpv_script_message_with_args("set-play-source", &[source]).await;

    // Step 3: Get resume position
    let resume_ticks = jf.get_item_resume_ticks(&item_id).await.unwrap_or(0);

    // Step 4: Find session and play
    let session_id = jf.find_mpv_session().await?
        .ok_or_else(|| anyhow::anyhow!("Shim not connected"))?;
    jf.play_item_with_resume(&session_id, &item_id, resume_ticks).await?;

    Ok(())
}
```

## 6. Wire into main.rs

Add to Commands enum:
```rust
/// Play a Jellyfin item
Play {
    /// What to play: next-up, recent-pinchflat, or an item ID
    target: String,
},
```

Add to match:
```rust
Commands::Play { target } => {
    commands::play::play(&ctx, &target).await?;
}
```

## 7. Update Hyprland keybindings

Edit ~/.config/hypr/conf.d/malphas.conf, change lines 91-92:

FROM:
```
bind = $mainMod, XF86AudioPlay, exec, ~/.config/hypr/scripts/shim-play.sh recent-pinchflat
bind = $mainMod CTRL, XF86AudioPlay, exec, ~/.config/hypr/scripts/shim-play.sh next-up
```

TO:
```
bind = $mainMod, XF86AudioPlay, exec, $media play recent-pinchflat
bind = $mainMod CTRL, XF86AudioPlay, exec, $media play next-up
```

## 8. Build and test

```bash
cd ~/projects/media-control
cargo build --release
# Test manually:
./target/release/media-control play next-up
./target/release/media-control play recent-pinchflat
```

## 9. After verification, remove shim-play.sh

Delete ~/.config/hypr/scripts/shim-play.sh

</work_remaining>

<critical_context>
## Jellyfin credentials
~/.config/jellyfin-mpv-shim/cred.json — array of server objects with address, AccessToken, UserId

## Jellyfin server
https://jellyfin.chimera-micro.ts.net (Tailscale HTTPS)

## mpv IPC socket
/tmp/mpvctl-jshim

## Library IDs
- Pinchflat: a5c0a87b1d058d1b7e70f5406ee274e2
- Shows: a656b907eb3a73532e40e44b968d0225

## Shim client UUID (for session matching)
~/.config/jellyfin-mpv-shim/conf.json → client_uuid field
The shim registers with Jellyfin using this as DeviceId.

## media-control binary
~/projects/media-control/target/release/media-control
$media variable in Hyprland config points here.

## Hyprland config
~/.config/hypr/conf.d/malphas.conf — lines 91-92

## Key requirement
The IPC hint (set-play-source) MUST arrive BEFORE the PlayNow websocket event.
Since local IPC is <1ms and the Jellyfin round-trip is 50-200ms, this is guaranteed
as long as we send IPC before the PlayNow POST.
</critical_context>
