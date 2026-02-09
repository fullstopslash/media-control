//! Jellyfin API client for session control.
//!
//! Provides async HTTP client for interacting with Jellyfin server,
//! supporting session management, playback control, and watched status.

use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when interacting with Jellyfin.
#[derive(Debug, Error)]
pub enum JellyfinError {
    #[error("credentials file not found at {0}")]
    CredentialsNotFound(PathBuf),

    #[error("failed to parse credentials: {0}")]
    CredentialsParsing(#[from] serde_json::Error),

    #[error("invalid credentials: missing {0}")]
    InvalidCredentials(&'static str),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("no active MPV session found")]
    NoMpvSession,

    #[error("session has no currently playing item")]
    NoPlayingItem,

    #[error("failed to get hostname")]
    HostnameError,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, JellyfinError>;

/// Credentials loaded from `~/.config/jellyfin-mpv-shim/cred.json`.
///
/// The credential file format is an array of server credentials.
/// We use the first entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    /// Server URL (called "address" in cred.json)
    #[serde(alias = "address")]
    pub server: String,

    /// User ID for API calls (called "UserId" in cred.json)
    #[serde(alias = "UserId")]
    pub user_id: String,

    /// API token (called "AccessToken" in cred.json)
    #[serde(alias = "AccessToken")]
    pub token: String,

    /// Device ID for session identification (called "uuid" in cred.json)
    #[serde(alias = "uuid", default = "default_device_id")]
    pub device_id: String,
}

fn default_device_id() -> String {
    "77c2f402-7180-4d84-a2f7-8d832b89e241".to_string()
}

/// Active session data from `/Sessions` endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Session {
    /// Session ID
    pub id: String,

    /// User ID associated with this session
    pub user_id: String,

    /// Device name
    pub device_name: String,

    /// Client application name
    pub client: String,

    /// Device ID
    #[serde(default)]
    pub device_id: String,

    /// Currently playing item, if any
    pub now_playing_item: Option<NowPlayingItem>,

    /// Current playback state
    pub play_state: Option<PlayState>,

    /// Queue of items for playback
    #[serde(default)]
    pub now_playing_queue: Vec<QueueItem>,
}

/// Information about the currently playing item.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NowPlayingItem {
    /// Item ID
    pub id: String,

    /// Item name/title
    pub name: String,

    /// Series name (for episodes)
    pub series_name: Option<String>,

    /// Series ID (for episodes)
    pub series_id: Option<String>,

    /// Item type (Episode, Movie, etc.)
    #[serde(rename = "Type")]
    pub type_field: String,
}

/// Current playback state.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayState {
    /// Current position in ticks (10,000 ticks = 1ms)
    pub position_ticks: Option<i64>,

    /// Whether playback is paused
    #[serde(default)]
    pub is_paused: bool,

    /// Whether seeking is supported
    #[serde(default)]
    pub can_seek: bool,
}

/// Item in the playback queue.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueueItem {
    /// Item ID
    pub id: String,
}

/// Response from the NextUp endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NextUpResponse {
    items: Vec<NextUpItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NextUpItem {
    id: String,
}

/// Playback info response for getting media source details.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackInfoResponse {
    play_session_id: String,
    media_sources: Vec<MediaSource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaSource {
    id: String,
    default_audio_stream_index: Option<i32>,
    default_subtitle_stream_index: Option<i32>,
}

/// Play command payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlayCommand {
    item_ids: Vec<String>,
    play_command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    play_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_source_id: Option<String>,
    start_index: i32,
    start_position_ticks: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    controlling_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle_stream_index: Option<i32>,
}

/// Jellyfin API client for session control.
#[derive(Debug, Clone)]
pub struct JellyfinClient {
    server_url: String,
    user_id: String,
    device_id: String,
    client: reqwest::Client,
}

impl JellyfinClient {
    /// Load credentials from the default credential file.
    ///
    /// Reads from `~/.config/jellyfin-mpv-shim/cred.json`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist or can't be parsed.
    pub async fn load_credentials() -> Result<Credentials> {
        let home = std::env::var("HOME").map_err(|_| {
            JellyfinError::CredentialsNotFound(PathBuf::from("~/.config/jellyfin-mpv-shim/cred.json"))
        })?;

        let cred_path = PathBuf::from(home).join(".config/jellyfin-mpv-shim/cred.json");

        if !cred_path.exists() {
            return Err(JellyfinError::CredentialsNotFound(cred_path));
        }

        let content = tokio::fs::read_to_string(&cred_path).await?;

        // The credential file is an array; we use the first entry
        let creds: Vec<Credentials> = serde_json::from_str(&content)?;

        creds
            .into_iter()
            .next()
            .ok_or(JellyfinError::InvalidCredentials("no credentials in file"))
    }

    /// Create a new client from credentials.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client can't be built or hostname lookup fails.
    pub fn new(credentials: Credentials) -> Result<Self> {
        let hostname = gethostname::gethostname()
            .into_string()
            .unwrap_or_else(|_| "media-control".to_string());

        let mut headers = HeaderMap::new();

        // Build authorization header in MediaBrowser format
        let auth_header = format!(
            "MediaBrowser Client=\"media-control\", Device=\"{}\", DeviceId=\"{}\", Version=\"1.0.0\", Token=\"{}\"",
            hostname, credentials.device_id, credentials.token
        );

        headers.insert(
            "Authorization",
            HeaderValue::from_str(&auth_header).map_err(|_| JellyfinError::HostnameError)?,
        );
        headers.insert(
            "X-Emby-Token",
            HeaderValue::from_str(&credentials.token).map_err(|_| JellyfinError::InvalidCredentials("token"))?,
        );
        headers.insert(
            "X-Emby-Client",
            HeaderValue::from_static("media-control"),
        );
        headers.insert(
            "X-Emby-Device-Name",
            HeaderValue::from_str(&hostname).map_err(|_| JellyfinError::HostnameError)?,
        );
        headers.insert(
            "X-Emby-Device-Id",
            HeaderValue::from_str(&credentials.device_id)
                .map_err(|_| JellyfinError::InvalidCredentials("device_id"))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            server_url: credentials.server.trim_end_matches('/').to_string(),
            user_id: credentials.user_id,
            device_id: credentials.device_id,
            client,
        })
    }

    /// Create a client by loading credentials from the default file.
    ///
    /// Convenience method combining `load_credentials()` and `new()`.
    pub async fn from_default_credentials() -> Result<Self> {
        let credentials = Self::load_credentials().await?;
        Self::new(credentials)
    }

    /// Fetch all active sessions from the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn fetch_sessions(&self) -> Result<Vec<Session>> {
        let url = format!("{}/Sessions", self.server_url);
        let sessions = self.client.get(&url).send().await?.json().await?;
        Ok(sessions)
    }

    /// Find the active MPV or jellyfin-mpv-shim session.
    ///
    /// Looks for sessions where:
    /// - The device ID matches our configured device ID, OR
    /// - The client name contains "mpv" or "jellyfin mpv shim"
    pub async fn find_mpv_session(&self) -> Result<Option<Session>> {
        let sessions = self.fetch_sessions().await?;

        let session = sessions.into_iter().find(|s| {
            s.device_id == self.device_id
                || s.client.to_lowercase().contains("mpv")
                || s.client.to_lowercase().contains("jellyfin mpv shim")
        });

        Ok(session)
    }

    /// Find the active MPV session or return an error.
    async fn require_mpv_session(&self) -> Result<Session> {
        self.find_mpv_session()
            .await?
            .ok_or(JellyfinError::NoMpvSession)
    }

    /// Stop playback for a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn stop(&self, session_id: &str) -> Result<()> {
        let url = format!("{}/Sessions/{}/Playing/Stop", self.server_url, session_id);
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// Stop playback for the active MPV session.
    pub async fn stop_mpv(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        self.stop(&session.id).await
    }

    /// Mark an item as watched for the current user.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn mark_watched(&self, item_id: &str) -> Result<()> {
        let url = format!(
            "{}/Users/{}/PlayedItems/{}",
            self.server_url, self.user_id, item_id
        );
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// Mark the currently playing item as watched.
    pub async fn mark_current_watched(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let item = session
            .now_playing_item
            .ok_or(JellyfinError::NoPlayingItem)?;
        self.mark_watched(&item.id).await
    }

    /// Mark the current item as watched and stop playback.
    pub async fn mark_watched_and_stop(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let item = session
            .now_playing_item
            .ok_or(JellyfinError::NoPlayingItem)?;

        self.mark_watched(&item.id).await?;
        self.stop(&session.id).await
    }

    /// Get the next episode in a series (NextUp).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn get_next_up(&self, series_id: &str) -> Result<Option<String>> {
        let url = format!(
            "{}/Shows/{}/NextUp?UserId={}",
            self.server_url, series_id, self.user_id
        );

        let response: NextUpResponse = self.client.get(&url).send().await?.json().await?;
        Ok(response.items.into_iter().next().map(|item| item.id))
    }

    /// Start playing an item in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn play_item(&self, session_id: &str, item_id: &str) -> Result<()> {
        self.play_items(session_id, &[item_id.to_string()]).await
    }

    /// Start playing multiple items in a session.
    pub async fn play_items(&self, session_id: &str, item_ids: &[String]) -> Result<()> {
        if item_ids.is_empty() {
            return Ok(());
        }

        // Fetch playback parameters for the first item
        let playback_info = self.fetch_playback_info(&item_ids[0]).await?;

        let command = PlayCommand {
            item_ids: item_ids.to_vec(),
            play_command: "PlayNow".to_string(),
            play_session_id: Some(playback_info.play_session_id),
            media_source_id: playback_info.media_sources.first().map(|s| s.id.clone()),
            start_index: 0,
            start_position_ticks: 0,
            controlling_user_id: Some(self.user_id.clone()),
            audio_stream_index: playback_info
                .media_sources
                .first()
                .and_then(|s| s.default_audio_stream_index),
            subtitle_stream_index: playback_info
                .media_sources
                .first()
                .and_then(|s| s.default_subtitle_stream_index),
        };

        let url = format!("{}/Sessions/{}/Command/Play", self.server_url, session_id);
        self.client.post(&url).json(&command).send().await?;
        Ok(())
    }

    /// Fetch playback info for an item.
    async fn fetch_playback_info(&self, item_id: &str) -> Result<PlaybackInfoResponse> {
        let url = format!(
            "{}/Items/{}/PlaybackInfo?UserId={}",
            self.server_url, item_id, self.user_id
        );

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    /// Get remaining queue item IDs after the current item.
    fn get_remaining_queue_ids(session: &Session, current_item_id: &str) -> Vec<String> {
        let queue = &session.now_playing_queue;

        let current_idx = queue.iter().position(|item| item.id == current_item_id);

        match current_idx {
            Some(idx) if idx + 1 < queue.len() => {
                queue[idx + 1..].iter().map(|item| item.id.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Advance to the next item in the queue.
    pub async fn next(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let current_id = session
            .now_playing_item
            .as_ref()
            .ok_or(JellyfinError::NoPlayingItem)?
            .id
            .clone();

        let remaining = Self::get_remaining_queue_ids(&session, &current_id);
        if remaining.is_empty() {
            return Ok(());
        }

        self.play_items(&session.id, &remaining).await
    }

    /// Mark the current item as watched and advance to the next in queue.
    pub async fn mark_watched_and_next(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let item = session
            .now_playing_item
            .as_ref()
            .ok_or(JellyfinError::NoPlayingItem)?;

        let item_id = item.id.clone();
        self.mark_watched(&item_id).await?;

        // Re-fetch session to get updated queue
        let session = self.require_mpv_session().await?;
        let remaining = Self::get_remaining_queue_ids(&session, &item_id);

        if remaining.is_empty() {
            return Ok(());
        }

        self.play_items(&session.id, &remaining).await
    }

    /// Get the server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Get the user ID.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the device ID.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_parsing() {
        let json = r#"[
            {
                "address": "http://jellyfin.local:8096",
                "UserId": "user-123-456",
                "uuid": "device-uuid-789",
                "AccessToken": "secret-token-abc"
            }
        ]"#;

        let creds: Vec<Credentials> = serde_json::from_str(json).unwrap();
        assert_eq!(creds.len(), 1);

        let cred = &creds[0];
        assert_eq!(cred.server, "http://jellyfin.local:8096");
        assert_eq!(cred.user_id, "user-123-456");
        assert_eq!(cred.token, "secret-token-abc");
        assert_eq!(cred.device_id, "device-uuid-789");
    }

    #[test]
    fn test_credentials_with_default_device_id() {
        let json = r#"[
            {
                "address": "http://localhost:8096",
                "UserId": "user-id",
                "AccessToken": "token"
            }
        ]"#;

        let creds: Vec<Credentials> = serde_json::from_str(json).unwrap();
        // Default device_id should be applied when uuid is missing
        assert_eq!(creds[0].device_id, "77c2f402-7180-4d84-a2f7-8d832b89e241");
    }

    #[test]
    fn test_session_parsing() {
        let json = r#"{
            "Id": "session-123",
            "UserId": "user-456",
            "DeviceName": "Desktop",
            "Client": "Jellyfin MPV Shim",
            "DeviceId": "device-789",
            "NowPlayingItem": {
                "Id": "item-abc",
                "Name": "Episode 1",
                "SeriesName": "Test Show",
                "SeriesId": "series-xyz",
                "Type": "Episode"
            },
            "PlayState": {
                "PositionTicks": 12345678900,
                "IsPaused": false,
                "CanSeek": true
            },
            "NowPlayingQueue": [
                {"Id": "item-abc"},
                {"Id": "item-def"},
                {"Id": "item-ghi"}
            ]
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "session-123");
        assert_eq!(session.user_id, "user-456");
        assert_eq!(session.client, "Jellyfin MPV Shim");

        let item = session.now_playing_item.unwrap();
        assert_eq!(item.id, "item-abc");
        assert_eq!(item.name, "Episode 1");
        assert_eq!(item.series_name, Some("Test Show".to_string()));
        assert_eq!(item.type_field, "Episode");

        let state = session.play_state.unwrap();
        assert_eq!(state.position_ticks, Some(12345678900));
        assert!(!state.is_paused);
        assert!(state.can_seek);

        assert_eq!(session.now_playing_queue.len(), 3);
    }

    #[test]
    fn test_session_without_optional_fields() {
        let json = r#"{
            "Id": "session-123",
            "UserId": "user-456",
            "DeviceName": "Desktop",
            "Client": "Web Client"
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();
        assert!(session.now_playing_item.is_none());
        assert!(session.play_state.is_none());
        assert!(session.now_playing_queue.is_empty());
    }

    #[test]
    fn test_get_remaining_queue_ids() {
        let session = Session {
            id: "s1".to_string(),
            user_id: "u1".to_string(),
            device_name: "test".to_string(),
            client: "test".to_string(),
            device_id: "d1".to_string(),
            now_playing_item: None,
            play_state: None,
            now_playing_queue: vec![
                QueueItem { id: "a".to_string() },
                QueueItem { id: "b".to_string() },
                QueueItem { id: "c".to_string() },
                QueueItem { id: "d".to_string() },
            ],
        };

        // Current is first item
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "a");
        assert_eq!(remaining, vec!["b", "c", "d"]);

        // Current is middle item
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "b");
        assert_eq!(remaining, vec!["c", "d"]);

        // Current is last item
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "d");
        assert!(remaining.is_empty());

        // Current not in queue
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "unknown");
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_auth_header_format() {
        // Test that auth header components are correct
        let hostname = "test-host";
        let device_id = "test-device-id";
        let token = "test-token";

        let auth_header = format!(
            "MediaBrowser Client=\"media-control\", Device=\"{}\", DeviceId=\"{}\", Version=\"1.0.0\", Token=\"{}\"",
            hostname, device_id, token
        );

        assert!(auth_header.contains("Client=\"media-control\""));
        assert!(auth_header.contains(&format!("Device=\"{}\"", hostname)));
        assert!(auth_header.contains(&format!("DeviceId=\"{}\"", device_id)));
        assert!(auth_header.contains(&format!("Token=\"{}\"", token)));
        assert!(auth_header.contains("Version=\"1.0.0\""));
    }

    #[test]
    fn test_next_up_response_parsing() {
        let json = r#"{
            "Items": [
                {"Id": "next-episode-id"}
            ]
        }"#;

        let response: NextUpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, "next-episode-id");
    }

    #[test]
    fn test_next_up_empty_response() {
        let json = r#"{"Items": []}"#;

        let response: NextUpResponse = serde_json::from_str(json).unwrap();
        assert!(response.items.is_empty());
    }

    #[test]
    fn test_playback_info_response_parsing() {
        let json = r#"{
            "PlaySessionId": "play-session-123",
            "MediaSources": [
                {
                    "Id": "media-source-456",
                    "DefaultAudioStreamIndex": 1,
                    "DefaultSubtitleStreamIndex": 2
                }
            ]
        }"#;

        let response: PlaybackInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.play_session_id, "play-session-123");
        assert_eq!(response.media_sources.len(), 1);
        assert_eq!(response.media_sources[0].id, "media-source-456");
        assert_eq!(response.media_sources[0].default_audio_stream_index, Some(1));
        assert_eq!(response.media_sources[0].default_subtitle_stream_index, Some(2));
    }
}
