---
title: media-control
project: media-control
area: code
horizon: projects
created: 2025-11-30
tags: []
---

# Todo - media-control

## Pending <!-- the-desk:filter project:media-control status:pending -->

  - **What**: The media-control Rust codebase has a per-library configurable "next episode" system in `commands/mark_watched.rs` and `config.rs` (`NextEpisodeConfig`, `NextEpisodeStrategy`, `NextEpisodeRule`). This should be ported to the jellyfin-mpv-shim Python fork so the shim handles episode advancement natively instead of relying on external session API commands.
  - **Strategies to port**:
    - `next-up`: Jellyfin's NextUp API (next unwatched in series)
    - `recent-unwatched`: Jump to newest unwatched item in library
    - `next-older`: Walk down timeline to next older unwatched item (used for Pinchflat/YouTube content)
    - `series-or-random`: Next in box set collection, or random unwatched
    - `random-unwatched`: Random unwatched from library
  - **Config format** (TOML in media-control, adapt to shim's config format):
    - Rules matched in order, first match wins
    - Each rule has: `library` (name match), `library_id`, `path_contains` (fast path matching), `strategy`
    - Default/fallback rule has no `library` field
  - **Key lessons learned during implementation**:
    - Jellyfin Ancestors API is very slow on first call (~3s cold cache). Use `path_contains` matching against the item's file path instead when possible.
    - Queue-based advancement doesn't work with the shim — marking an item watched clears the session queue. Must capture queue before marking, or use strategy-based advancement.
    - The shim's session doesn't always populate `NowPlayingItem` — check `NowPlayingQueueFullItems` as fallback.
    - `play_item` must use `POST /Sessions/{id}/Playing?PlayCommand=PlayNow&ItemIds={id}` (query params), NOT `POST /Sessions/{id}/Command/Play` (JSON body) — the shim only responds to the former.
    - Exclude media-control's own sessions when searching for the mpv session (`client != "media-control"`).
  - **Advantage of porting to shim**: The shim already knows which library it's playing from internally — no Ancestors API or path matching needed. Strategy logic and config format can carry over directly.
  - **Reference files**: `crates/media-control-lib/src/commands/mark_watched.rs`, `crates/media-control-lib/src/config.rs` (search for `NextEpisode`), `crates/media-control-lib/src/jellyfin.rs` (search for `get_unwatched_items`, `get_collection_items`, `get_item_library`), `~/.config/media-control/config.toml` (live config with all rules)

  - **Root cause**: `jellyfin-mpv-shim-fork` `player.py:578` — `_play_media` blocks on `wait_property(duration)` with a `@synchronous("_lock")` decorator. New play commands queue behind the lock instead of cancelling the current load.
  - **Symptom**: Pressing the keybinding multiple times marks the SAME item watched repeatedly because the session's `NowPlayingItem` is stale (shim hasn't finished loading the new video).
  - **Fix location**: `~/projects/jellyfin-mpv-shim-fork/jellyfin_mpv_shim/player.py` — the `_play_media` method needs to cancel in-progress loads when a new play command arrives.
  - **Blocked on**: Fix must be implemented in the shim fork project, not here.

## Completed <!-- the-desk:filter project:media-control status:completed -->
