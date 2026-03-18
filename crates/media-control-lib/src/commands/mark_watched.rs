//! Jellyfin integration for marking items as watched.
//!
//! Provides commands to mark the current item as watched, optionally
//! stopping playback or advancing to the next item in the queue.

use super::{get_media_window, CommandContext};
use crate::error::{MediaControlError, Result};
use crate::jellyfin::{JellyfinClient, JellyfinError};

/// Convert a Jellyfin error to a MediaControlError.
fn convert_jellyfin_error(e: JellyfinError) -> MediaControlError {
    match e {
        JellyfinError::CredentialsNotFound(_) | JellyfinError::InvalidCredentials(_) => {
            MediaControlError::jellyfin_credentials()
        }
        JellyfinError::NoMpvSession => MediaControlError::jellyfin_session_not_found(),
        JellyfinError::NoPlayingItem => MediaControlError::jellyfin_session_not_found(),
        JellyfinError::Http(e) => MediaControlError::jellyfin_api(e),
        JellyfinError::CredentialsParsing(e) => MediaControlError::jellyfin_api(e),
        JellyfinError::HostnameError => MediaControlError::jellyfin_api("hostname lookup failed"),
        JellyfinError::Io(e) => MediaControlError::Io(e),
    }
}

/// Mark the current Jellyfin session item as watched.
///
/// This command finds the active mpv media window, loads Jellyfin credentials,
/// and marks the currently playing item as watched on the Jellyfin server.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;
    jellyfin
        .mark_current_watched()
        .await
        .map_err(convert_jellyfin_error)?;

    Ok(())
}

/// Mark current item as watched and stop playback.
///
/// Marks the current Jellyfin item as watched and stops both the Jellyfin
/// session and local mpv playback via playerctl.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched_and_stop(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched_and_stop(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;
    jellyfin
        .mark_watched_and_stop()
        .await
        .map_err(convert_jellyfin_error)?;

    // Also try playerctl stop (best effort, ignore errors)
    let _ = tokio::process::Command::new("playerctl")
        .args(["--player=mpv", "stop"])
        .output()
        .await;

    Ok(())
}

/// Mark current item as watched and advance to next in queue.
///
/// Marks the current Jellyfin item as watched and advances playback to
/// the next item in the queue.
///
/// # Returns
///
/// - `Ok(())` if successful, no media window found, or window is not mpv
/// - `Err(...)` if Jellyfin API call fails
///
/// # Example
///
/// ```no_run
/// use media_control_lib::commands::{CommandContext, mark_watched};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let ctx = CommandContext::new()?;
/// mark_watched::mark_watched_and_next(&ctx).await?;
/// # Ok(())
/// # }
/// ```
pub async fn mark_watched_and_next(ctx: &CommandContext) -> Result<()> {
    let media = match get_media_window(ctx).await? {
        Some(m) => m,
        None => return Ok(()),
    };

    if media.class != "mpv" {
        return Ok(());
    }

    let jellyfin = JellyfinClient::from_default_credentials()
        .await
        .map_err(convert_jellyfin_error)?;

    let session = jellyfin
        .find_mpv_session()
        .await
        .map_err(convert_jellyfin_error)?;
    let Some(session) = session else { return Ok(()); };
    let Some(item) = session.current_item() else { return Ok(()); };

    let item_id = item.id.clone();
    let series_id = item.series_id.clone();
    let session_id = session.id.clone();

    // Run mark_watched and next-episode strategy resolution in parallel.
    // They're independent — marking doesn't affect which item we pick next.
    let (mark_result, next_item_id) = tokio::join!(
        jellyfin.mark_watched(&item_id),
        execute_next_strategy(&jellyfin, &ctx.config, &item_id, series_id.as_deref()),
    );

    if let Err(e) = mark_result {
        tracing::debug!("mark_watched failed: {e}");
    }

    if let Some(ref next_id) = next_item_id {
        let _ = jellyfin.play_item(&session_id, next_id).await;
    }

    Ok(())
}

/// Execute the configured next-episode strategy.
///
/// Resolves the library for the current item, looks up the matching strategy
/// rule from config, and executes it. Returns the item ID to play next,
/// or None if no suitable item was found.
///
/// Strategy errors are silently ignored (best-effort).
async fn execute_next_strategy(
    jellyfin: &JellyfinClient,
    config: &crate::config::Config,
    current_item_id: &str,
    series_id: Option<&str>,
) -> Option<String> {
    use crate::config::NextEpisodeStrategy;

    // Resolve library name from Jellyfin to match config rules.
    // This is the slow part (~3s on first call). If the matched rule
    // already has library_id set, we use that directly for the strategy.
    let library_info = match jellyfin.get_item_library(current_item_id).await {
        Ok(info) => info,
        Err(_) => None,
    };

    let library_name = library_info.as_ref().map(|l| l.name.as_str()).unwrap_or("");
    let resolved = config.next_episode.resolve_strategy(library_name);

    // Prefer library_id from config (no extra API call) over the one from detection
    let library_id = resolved
        .library_id
        .as_deref()
        .or_else(|| library_info.as_ref().map(|l| l.id.as_str()));

    match resolved.strategy {
        NextEpisodeStrategy::NextUp => {
            strategy_next_up(jellyfin, series_id).await
        }
        NextEpisodeStrategy::RecentUnwatched => {
            if let Some(lid) = library_id {
                strategy_recent_unwatched(jellyfin, lid, current_item_id).await
            } else {
                strategy_next_up(jellyfin, series_id).await
            }
        }
        NextEpisodeStrategy::SeriesOrRandom => {
            if let Some(lid) = library_id {
                strategy_series_or_random(jellyfin, current_item_id, lid).await
            } else {
                strategy_next_up(jellyfin, series_id).await
            }
        }
        NextEpisodeStrategy::RandomUnwatched => {
            if let Some(lid) = library_id {
                strategy_random_unwatched(jellyfin, lid, current_item_id).await
            } else {
                strategy_next_up(jellyfin, series_id).await
            }
        }
    }
}

/// Strategy: NextUp - next unwatched episode in the series.
async fn strategy_next_up(jellyfin: &JellyfinClient, series_id: Option<&str>) -> Option<String> {
    let sid = series_id?;
    jellyfin.get_next_up(sid).await.ok().flatten()
}

/// Strategy: RecentUnwatched - most recently acquired unwatched item.
/// Prefers items newer than current; falls back to most recent older item.
async fn strategy_recent_unwatched(
    jellyfin: &JellyfinClient,
    library_id: &str,
    current_item_id: &str,
) -> Option<String> {
    // Get unwatched items sorted by DateCreated descending (newest first)
    let items = jellyfin
        .get_unwatched_items(library_id, "DateCreated", "Descending", Some(current_item_id), 50)
        .await
        .ok()?;

    if items.is_empty() {
        return None;
    }

    // Get current item's DateCreated for comparison
    let current_date = {
        let all_items = jellyfin
            .get_unwatched_items(library_id, "DateCreated", "Descending", None, 200)
            .await
            .ok()?;
        all_items
            .iter()
            .find(|i| i.id == current_item_id)
            .and_then(|i| i.date_created.clone())
    };

    if let Some(ref current_dc) = current_date {
        // Prefer items more recent than current
        let newer: Vec<_> = items
            .iter()
            .filter(|i| i.date_created.as_deref() > Some(current_dc.as_str()))
            .collect();

        if let Some(item) = newer.last() {
            // newest-first list, so .last() is the one closest to (but after) current
            return Some(item.id.clone());
        }
    }

    // No newer items (or no date to compare) — just pick the first (most recent) unwatched
    Some(items[0].id.clone())
}

/// Strategy: RandomUnwatched - random unwatched item from the library.
async fn strategy_random_unwatched(
    jellyfin: &JellyfinClient,
    library_id: &str,
    current_item_id: &str,
) -> Option<String> {
    let items = jellyfin
        .get_unwatched_items(library_id, "Random", "Descending", Some(current_item_id), 1)
        .await
        .ok()?;

    items.first().map(|i| i.id.clone())
}

/// Strategy: SeriesOrRandom - next in box set if applicable, otherwise random.
async fn strategy_series_or_random(
    jellyfin: &JellyfinClient,
    current_item_id: &str,
    library_id: &str,
) -> Option<String> {
    // Check if the item is in a box set via ancestors
    if let Ok(Some(collection_id)) = find_parent_collection(jellyfin, current_item_id).await {
        // Get items in the collection
        if let Ok(items) = jellyfin.get_collection_items(&collection_id).await {
            // Find current item's position and return the next one
            let current_pos = items.iter().position(|i| i.id == current_item_id);
            if let Some(pos) = current_pos {
                if pos + 1 < items.len() {
                    return Some(items[pos + 1].id.clone());
                }
            }
        }
    }

    // Not in a collection, or last in collection — random unwatched
    strategy_random_unwatched(jellyfin, library_id, current_item_id).await
}

/// Find a parent BoxSet collection for an item via the Ancestors API.
async fn find_parent_collection(
    jellyfin: &JellyfinClient,
    item_id: &str,
) -> std::result::Result<Option<String>, crate::jellyfin::JellyfinError> {
    let response: Vec<serde_json::Value> = jellyfin
        .fetch_ancestors_raw(item_id)
        .await?;

    for ancestor in response {
        if ancestor.get("Type").and_then(|t| t.as_str()) == Some("BoxSet") {
            if let Some(id) = ancestor.get("Id").and_then(|i| i.as_str()) {
                return Ok(Some(id.to_string()));
            }
        }
    }

    Ok(None)
}
