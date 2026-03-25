//! Tag the currently-playing item as "keep" to prevent auto-deletion.
//!
//! Broadcasts `script-message keep` to ALL known mpv sockets. Each mpv
//! instance has its own context handler (shim for Jellyfin, lua for Stash)
//! that acts only when relevant content is playing.

use super::{get_media_window, CommandContext};
use crate::error::Result;

/// All mpv sockets that might have keepable content.
const KEEP_SOCKETS: &[&str] = &["/tmp/mpvctl-jshim", "/tmp/mpvctl-stash", "/tmp/mpvctl0"];

/// Tag the current item as "keep" across all mpv instances.
pub async fn keep(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    use std::os::unix::fs::FileTypeExt;
    use std::path::Path;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    use tokio::time::timeout;

    let payload = r#"{"command":["script-message","keep"]}"#;
    let mut sent = false;

    for socket_path in KEEP_SOCKETS {
        let path = Path::new(socket_path);
        match std::fs::metadata(path) {
            Ok(meta) if meta.file_type().is_socket() => {}
            _ => continue,
        }

        let result = timeout(std::time::Duration::from_millis(500), async {
            let mut stream = UnixStream::connect(path).await?;
            stream.write_all(payload.as_bytes()).await?;
            stream.write_all(b"\n").await?;
            Ok::<_, std::io::Error>(())
        })
        .await;

        if matches!(result, Ok(Ok(()))) {
            sent = true;
        }
    }

    if !sent {
        eprintln!("media-control: keep: no mpv socket responded");
    }

    Ok(())
}
