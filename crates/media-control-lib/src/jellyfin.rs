//! Jellyfin API client for session control.
//!
//! Provides async HTTP client for interacting with Jellyfin server,
//! supporting session management, playback control, and watched status.

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::redirect;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Maximum size accepted for the Jellyfin credentials file.
///
/// Real cred.json files are < 4 KiB; the cap exists to prevent OOM if a
/// hostile or accidental large file is placed at the credential path.
const MAX_CRED_FILE_BYTES: u64 = 64 * 1024;

/// Errors that can occur when interacting with Jellyfin.
#[derive(Debug, Error)]
pub enum JellyfinError {
    #[error("credentials file not found at {0}")]
    CredentialsNotFound(PathBuf),

    #[error("credentials file too large: {size} bytes (max {max})")]
    CredentialsTooLarge { size: u64, max: u64 },

    #[error("failed to parse credentials: {0}")]
    CredentialsParsing(#[from] serde_json::Error),

    /// Like `CredentialsParsing` but carries the file path that failed to
    /// parse. Produced by `load_credentials`; the bare `CredentialsParsing`
    /// variant is reserved for the `#[from] serde_json::Error` bridge where
    /// the path isn't known at the conversion site.
    ///
    /// Without this, a parse failure surfaces as a bare TOML/JSON error and
    /// the user has to guess which credential file was offending — this
    /// matters when `XDG_CONFIG_HOME` is set non-standard or the operator
    /// has multiple jellyfin-mpv-shim profiles.
    #[error("failed to parse credentials at {}: {source}", path.display())]
    CredentialsParseAt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid credentials: missing {0}")]
    InvalidCredentials(&'static str),

    #[error("invalid HTTP header value for {0}")]
    InvalidHeader(&'static str),

    /// The `server` field in `cred.json` is not a usable HTTP(S) base URL.
    ///
    /// Without this guard, a hostile cred file with `"server":
    /// "http://attacker.example/"` would exfiltrate the bearer token to an
    /// attacker-controlled host on every API call. We reject non-`http(s)`
    /// schemes, missing/empty hosts, and `user:pass@` userinfo at construction
    /// time.
    #[error("invalid server URL {value:?}: {reason}")]
    InvalidServerUrl { value: String, reason: &'static str },

    #[error("failed to build request URL for {path:?}: {source}")]
    BuildUrl {
        path: String,
        #[source]
        source: url::ParseError,
    },

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("no active MPV session found")]
    NoMpvSession,

    #[error("session has no currently playing item")]
    NoPlayingItem,

    /// An identifier supplied to a path-building API contains characters that
    /// could escape the URL path (e.g. `/`, `..`, control chars).
    ///
    /// Jellyfin item, session, series, and library IDs are server-supplied and
    /// must be treated as untrusted input — a hostile server could return an ID
    /// like `../../admin` to traverse to unintended endpoints.
    #[error("invalid {kind} id: {value:?} (must contain only [A-Za-z0-9._-])")]
    InvalidId { kind: &'static str, value: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Validate that an ID is safe to interpolate into a URL path segment.
///
/// Note: this validator is for Jellyfin *path segments* — it rejects
/// characters that would escape a URL path segment (`/`, `?`, `#`, `\`,
/// control chars, etc.). It does **not** validate that the ID matches any
/// particular Jellyfin format (GUID, UUID, slug, …); a well-formed but
/// nonexistent ID will pass this check and fail later at the API call.
///
/// Jellyfin uses GUID-style hex IDs (32 chars) for items/sessions/users, but
/// older endpoints occasionally return IDs with `-` or `_`. We accept the
/// conservative `[A-Za-z0-9._-]` and reject anything else — particularly
/// `/`, `?`, `#`, `\`, and control characters that would let a hostile
/// server response escape the intended URL path.
///
/// Empty IDs are rejected because they would produce a `//` in the URL,
/// which most servers normalise to a different endpoint.
fn validate_id(kind: &'static str, id: &str) -> Result<()> {
    // Reject empty, `.`, and `..` outright. The character allowlist (which
    // includes `.`) would otherwise accept these path-traversal forms;
    // they're meaningful only as filesystem components, never as Jellyfin IDs.
    let bad_charset = !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if id.is_empty() || id == "." || id == ".." || bad_charset {
        return Err(JellyfinError::InvalidId {
            kind,
            value: id.to_string(),
        });
    }
    Ok(())
}

/// Parse and validate the `server` field of a credential file as an HTTP(S)
/// base URL.
///
/// Rejects anything that could redirect a bearer token to an unintended host:
///
/// - non-`http`/`https` schemes (e.g. `file://`, `gopher://`, `javascript:`)
/// - missing or empty host
/// - URLs containing user-info (`https://user:pass@host`), which would
///   otherwise be silently merged into outgoing requests
///
/// On success, returns the parsed `Url` with any trailing slash present —
/// `Url::join` handles relative-path resolution correctly without manual
/// trimming.
fn validate_server_url(raw: &str) -> Result<Url> {
    let parsed = Url::parse(raw).map_err(|_| JellyfinError::InvalidServerUrl {
        value: raw.to_string(),
        reason: "not a valid URL",
    })?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(JellyfinError::InvalidServerUrl {
                value: raw.to_string(),
                reason: "scheme must be http or https",
            });
        }
    }
    match parsed.host_str() {
        Some(h) if !h.is_empty() => {}
        _ => {
            return Err(JellyfinError::InvalidServerUrl {
                value: raw.to_string(),
                reason: "missing or empty host",
            });
        }
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(JellyfinError::InvalidServerUrl {
            value: raw.to_string(),
            reason: "must not contain user:pass@ credentials",
        });
    }
    Ok(parsed)
}

pub type Result<T> = std::result::Result<T, JellyfinError>;

/// Credentials loaded from `~/.config/jellyfin-mpv-shim/cred.json`.
///
/// The credential file format is an array of server credentials.
/// We use the first entry.
///
/// `Debug` is implemented manually to redact secrets — the auto-derived form
/// would print `token` and `device_id` verbatim, leaking them through any
/// `tracing::error!(?creds, ...)` call or panic backtrace.
#[derive(Clone, Deserialize)]
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

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("server", &self.server)
            .field("user_id", &self.user_id)
            .field("token", &"[REDACTED]")
            .field("device_id", &"[REDACTED]")
            .finish()
    }
}

/// Stable fallback device ID used when `cred.json` lacks a `uuid`.
///
/// Real cred files written by jellyfin-mpv-shim always include a `uuid`. This
/// default exists only so an entirely hand-rolled credentials file (lacking
/// `uuid`) still parses. The constant is shared across installs by design: it
/// identifies the *application* (media-control), not a specific host. Per-host
/// identification uses the sanitised `gethostname()` value in the
/// `X-Emby-Device-Name` header.
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

    /// Full item details for the queue (used as fallback when NowPlayingItem is absent)
    #[serde(default)]
    pub now_playing_queue_full_items: Vec<NowPlayingItem>,
}

impl Session {
    /// Get the currently playing item, falling back to the first queue full item.
    ///
    /// Some Jellyfin clients (like jellyfin-mpv-shim) don't always populate
    /// `NowPlayingItem` but do populate `NowPlayingQueueFullItems`.
    pub fn current_item(&self) -> Option<&NowPlayingItem> {
        self.now_playing_item
            .as_ref()
            .or_else(|| self.now_playing_queue_full_items.first())
    }
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

    /// When the item was added to the library
    #[serde(default)]
    pub date_created: Option<String>,
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

    /// Per-occurrence queue identifier for the currently playing item.
    ///
    /// Distinct from `NowPlayingItem.Id`: when the same media item appears
    /// multiple times in a queue, every occurrence gets a unique
    /// `PlaylistItemId`. Matching the queue position by `Id` alone breaks on
    /// repeat items (it always finds the *first* occurrence).
    #[serde(default, rename = "PlaylistItemId")]
    pub playlist_item_id: Option<String>,
}

/// Item in the playback queue.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueueItem {
    /// Item ID
    pub id: String,

    /// Per-occurrence queue identifier — see `PlayState::playlist_item_id`.
    #[serde(default, rename = "PlaylistItemId")]
    pub playlist_item_id: Option<String>,
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

/// An ancestor item from the `/Items/{id}/Ancestors` API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AncestorItem {
    id: String,
    name: String,
    #[serde(rename = "Type")]
    item_type: String,
    #[serde(default)]
    collection_type: Option<String>,
}

/// Information about a Jellyfin library.
#[derive(Debug, Clone)]
pub struct LibraryInfo {
    /// Library ID.
    pub id: String,
    /// Library display name (e.g., "Shows", "Pinchtube", "Movies").
    pub name: String,
    /// Collection type (e.g., "tvshows", "movies", "musicvideos").
    pub collection_type: Option<String>,
}

/// Detailed item response (for resume position etc.).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemDetail {
    #[allow(dead_code)]
    id: String,
    user_data: Option<ItemUserData>,
}

/// User-specific data for an item (playback position, played status, etc.).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemUserData {
    playback_position_ticks: i64,
}

/// Response from Items endpoints.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemsResponse {
    items: Vec<ItemSummary>,
}

/// Sort field for Jellyfin item queries.
///
/// Mirrors the subset of Jellyfin's `SortBy` enum we actually use. Constraining
/// the type at the function boundary (vs. `&str`) means a typo like
/// `"datecreated"` (lowercase, server-rejected) can't compile — and any future
/// caller is forced to confront the closed set rather than guessing a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    /// Server-side library add date (`DateCreated`).
    DateCreated,
    /// Alphabetical sort name (`SortName`).
    SortName,
    /// Original release/premiere date (`PremiereDate`).
    PremiereDate,
    /// Production year (`ProductionYear`).
    ProductionYear,
    /// Pseudo-random ordering (`Random`). Useful for shuffle-style picks.
    Random,
    /// Index number within a series/season (`IndexNumber`).
    IndexNumber,
}

impl fmt::Display for SortBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DateCreated => "DateCreated",
            Self::SortName => "SortName",
            Self::PremiereDate => "PremiereDate",
            Self::ProductionYear => "ProductionYear",
            Self::Random => "Random",
            Self::IndexNumber => "IndexNumber",
        })
    }
}

impl SortBy {
    /// Wire-format spelling expected by Jellyfin's `SortBy` query parameter.
    fn as_str(self) -> &'static str {
        match self {
            Self::DateCreated => "DateCreated",
            Self::SortName => "SortName",
            Self::PremiereDate => "PremiereDate",
            Self::ProductionYear => "ProductionYear",
            Self::Random => "Random",
            Self::IndexNumber => "IndexNumber",
        }
    }
}

/// Sort direction for Jellyfin item queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// `Ascending` — A→Z, oldest→newest.
    Ascending,
    /// `Descending` — Z→A, newest→oldest.
    Descending,
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl SortOrder {
    /// Wire-format spelling expected by Jellyfin's `SortOrder` query parameter.
    fn as_str(self) -> &'static str {
        match self {
            Self::Ascending => "Ascending",
            Self::Descending => "Descending",
        }
    }
}

/// Summary of a Jellyfin item (used in filtered queries).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemSummary {
    /// Item ID.
    pub id: String,
    /// Item name.
    pub name: String,
    /// When the item was added to the library.
    pub date_created: Option<String>,
    /// Index within a season/collection.
    pub index_number: Option<i32>,
    /// Production year.
    pub production_year: Option<i32>,
    /// Item type (Episode, Movie, etc.).
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
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
///
/// `Debug` is implemented manually to redact `device_id` and `user_id` —
/// either is enough to deanonymise a user, and the auto-derived form would
/// leak them through any `tracing::error!(?client, ...)` or panic backtrace.
#[derive(Clone)]
pub struct JellyfinClient {
    /// Validated server base URL. Stored as a `Url` (not `String`) so every
    /// outgoing request resolves against a host we vetted at construction
    /// time — preventing token exfiltration via a hostile cred.json.
    server_url: Url,
    user_id: String,
    device_id: String,
    client: reqwest::Client,
}

impl fmt::Debug for JellyfinClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JellyfinClient")
            .field("server_url", &self.server_url.as_str())
            .field("user_id", &"[REDACTED]")
            .field("device_id", &"[REDACTED]")
            .finish()
    }
}

impl JellyfinClient {
    /// Resolve the credentials file path, honoring `XDG_CONFIG_HOME`.
    fn default_cred_path() -> Option<PathBuf> {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config_dir.join("jellyfin-mpv-shim").join("cred.json"))
    }

    /// Load credentials from the default credential file.
    ///
    /// Reads from `$XDG_CONFIG_HOME/jellyfin-mpv-shim/cred.json` (falling back
    /// to `~/.config/...`). The file is size-capped at
    /// [`MAX_CRED_FILE_BYTES`] to prevent OOM on a malformed input.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, exceeds the size cap, or
    /// can't be parsed.
    pub async fn load_credentials() -> Result<Credentials> {
        use tokio::io::AsyncReadExt;

        let cred_path = Self::default_cred_path().ok_or_else(|| {
            JellyfinError::CredentialsNotFound(PathBuf::from(
                "~/.config/jellyfin-mpv-shim/cred.json",
            ))
        })?;

        // Open once, then enforce the size cap on the live reader. Doing the
        // size check via `metadata()` followed by `read_to_string()` is a
        // TOCTOU bug — the file can grow between the two calls. Wrapping the
        // reader in `.take(MAX + 1)` lets us detect oversize atomically: if
        // we read more than MAX bytes, the file is too large.
        let file = match tokio::fs::File::open(&cred_path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(JellyfinError::CredentialsNotFound(cred_path));
            }
            Err(e) => return Err(JellyfinError::Io(e)),
        };

        let mut content = String::new();
        let mut limited = file.take(MAX_CRED_FILE_BYTES + 1);
        let read = limited.read_to_string(&mut content).await?;

        if read as u64 > MAX_CRED_FILE_BYTES {
            return Err(JellyfinError::CredentialsTooLarge {
                size: read as u64,
                max: MAX_CRED_FILE_BYTES,
            });
        }

        // The credential file is an array; we use the first entry.
        // Wrap the parse error with the path so the user sees *which* file
        // was malformed (`XDG_CONFIG_HOME` may point somewhere unexpected).
        let creds: Vec<Credentials> = serde_json::from_str(&content).map_err(|source| {
            JellyfinError::CredentialsParseAt {
                path: cred_path.clone(),
                source,
            }
        })?;

        creds
            .into_iter()
            .next()
            .ok_or(JellyfinError::InvalidCredentials("no credentials in file"))
    }

    /// Sanitise a hostname (or arbitrary identifier) for safe inclusion in
    /// HTTP header values.
    ///
    /// Strips anything outside `[A-Za-z0-9._-]`, collapses to a non-empty
    /// fallback. This prevents a malformed `gethostname()` result (containing
    /// `"`, `\`, control chars, or anything HTTP forbids) from corrupting the
    /// `Authorization` header — which would either be rejected by
    /// `HeaderValue::from_str` or, worse, be silently misinterpreted by the
    /// upstream parser.
    fn sanitize_header_value(input: &str) -> String {
        let cleaned: String = input
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect();
        if cleaned.is_empty() {
            "media-control".to_string()
        } else {
            cleaned
        }
    }

    /// Build the default header bundle for Jellyfin API requests.
    ///
    /// Centralises the `Authorization` and `X-Emby-*` headers so secret
    /// material lives in headers (not URLs/logs) and header construction
    /// failures map to a single, well-typed error. The `hostname` is
    /// sanitised before interpolation so a hostile `gethostname()` cannot
    /// inject quoting characters into the auth header.
    ///
    /// `token` and `device_id` are checked for `"`, `\`, `,`, `=`, CR, LF,
    /// and NUL before interpolation into the quoted MediaBrowser auth header
    /// format. `HeaderValue::from_str` would reject CR/LF/NUL anyway, but we
    /// want a typed error (`InvalidHeader`) — and to specifically rule out:
    ///
    /// - `"` / `\` — could close the quoted string and inject extra fields
    /// - `,` — the MediaBrowser scheme uses `, ` to separate `Key="value"`
    ///   pairs, so a literal `,` in a value smuggles new fields
    /// - `=` — combined with `,` lets a payload like `, AdminToken="x"`
    ///   forge an auth field; rejected even alone for defence in depth
    fn build_default_headers(credentials: &Credentials, hostname: &str) -> Result<HeaderMap> {
        fn header_quote_safe(s: &str) -> bool {
            s.bytes().all(|b| {
                b != b'"'
                    && b != b'\\'
                    && b != b','
                    && b != b'='
                    && b != b'\r'
                    && b != b'\n'
                    && b != 0
            })
        }
        if !header_quote_safe(&credentials.device_id) {
            return Err(JellyfinError::InvalidHeader("authorization (device_id)"));
        }
        if !header_quote_safe(&credentials.token) {
            return Err(JellyfinError::InvalidHeader("authorization (token)"));
        }

        let safe_host = Self::sanitize_header_value(hostname);
        // Build authorization header in MediaBrowser format
        let auth_header = format!(
            "MediaBrowser Client=\"media-control\", Device=\"{safe_host}\", DeviceId=\"{}\", Version=\"1.0.0\", Token=\"{}\"",
            credentials.device_id, credentials.token
        );

        // Single helper to convert a borrowed string into a sensitive header value.
        let mk = |name: &'static str,
                  value: &str|
         -> Result<(reqwest::header::HeaderName, HeaderValue)> {
            let mut hv =
                HeaderValue::from_str(value).map_err(|_| JellyfinError::InvalidHeader(name))?;
            hv.set_sensitive(true);
            Ok((reqwest::header::HeaderName::from_static(name), hv))
        };

        let entries: [(reqwest::header::HeaderName, HeaderValue); 5] = [
            mk("authorization", &auth_header)?,
            mk("x-emby-token", &credentials.token)?,
            (
                reqwest::header::HeaderName::from_static("x-emby-client"),
                HeaderValue::from_static("media-control"),
            ),
            mk("x-emby-device-name", &safe_host)?,
            mk("x-emby-device-id", &credentials.device_id)?,
        ];

        let mut headers = HeaderMap::with_capacity(entries.len());
        for (k, v) in entries {
            headers.insert(k, v);
        }
        Ok(headers)
    }

    /// Create a new client from credentials.
    ///
    /// # Errors
    ///
    /// Returns an error if `credentials.server` is not a valid `http`/`https`
    /// URL with a host (and no embedded user-info), the HTTP client can't be
    /// built, a header value is invalid, or `credentials.user_id` /
    /// `credentials.device_id` contain characters unsafe for a URL path
    /// segment or HTTP header value.
    pub fn new(credentials: Credentials) -> Result<Self> {
        // user_id is interpolated into URL paths in many endpoints; device_id
        // rides in headers. Validate both up front so a broken cred.json is
        // caught at startup, not on first request.
        validate_id("user", &credentials.user_id)?;
        validate_id("device", &credentials.device_id)?;

        // Validate the server URL **before** building any headers — a
        // hostile cred.json with `"server": "http://attacker.example/"` would
        // otherwise leak the bearer token to an attacker-controlled host on
        // every request.
        let server_url = validate_server_url(&credentials.server)?;

        // Lossy is fine: hostname is identification, not a security boundary.
        // `to_string_lossy()` returns a `Cow` that borrows the OsString when
        // it's already UTF-8 — only an invalid sequence triggers an alloc.
        let hostname = gethostname::gethostname();
        let headers = Self::build_default_headers(&credentials, &hostname.to_string_lossy())?;

        // Strip cross-host redirects so the auth headers (which reqwest's
        // default redirect policy preserves on same-origin hops) can never be
        // forwarded to a different host. Without this, a misconfigured server
        // returning `Location: http://attacker.example/` would exfiltrate the
        // bearer token.
        let redirect_policy = redirect::Policy::custom(|attempt| {
            let previous = attempt.previous();
            // `previous` is non-empty inside this callback (the current attempt
            // is always preceded by at least one prior URL), but defensively
            // fall back to "stop" if that ever changes.
            let prev_host = previous.first().and_then(|u| u.host());
            if attempt.url().host() != prev_host {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .redirect(redirect_policy)
            .build()?;

        Ok(Self {
            server_url,
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

    /// Build a fully-qualified endpoint URL from a path fragment.
    ///
    /// `path` should be the API path without a leading `/` (e.g. `Sessions`,
    /// `Users/abc/Items`). Uses `Url::join` so the parsed base URL governs
    /// host/scheme — `format!("{server}/{path}")` would let a future change
    /// to the path silently change the host (e.g. an absolute `//evil/x`).
    ///
    /// `Url::join` requires the base to have a trailing slash to treat
    /// itself as a directory; we handle bases without one transparently here
    /// so callers don't have to think about it.
    fn endpoint(&self, path: &str) -> Result<Url> {
        // Ensure the base ends with `/` so `join` resolves `path` *under* the
        // base rather than replacing the base's last segment.
        let base = if self.server_url.path().ends_with('/') {
            self.server_url.clone()
        } else {
            let mut b = self.server_url.clone();
            b.set_path(&format!("{}/", b.path()));
            b
        };
        base.join(path).map_err(|e| JellyfinError::BuildUrl {
            path: path.to_string(),
            source: e,
        })
    }

    /// Core HTTP transport: build, dispatch, error-check, and deserialise a
    /// JSON response.
    ///
    /// All public Jellyfin endpoints route through this so the
    /// build/query/body/`error_for_status`/`json` chain lives in exactly one
    /// place. Pair with `request_empty` when the endpoint has no response
    /// body (or when ignoring it is intentional).
    async fn request<R, B>(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
    ) -> Result<R>
    where
        R: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let mut req = self
            .client
            .request(method, self.endpoint(path)?)
            .query(query);
        if let Some(b) = body {
            req = req.json(b);
        }
        Ok(req.send().await?.error_for_status()?.json().await?)
    }

    /// Variant of `request` that doesn't deserialise a response body.
    ///
    /// We still call `.send().await?.error_for_status()?` so HTTP errors
    /// surface as `JellyfinError::Http` rather than silently succeeding.
    ///
    /// The response body is **drained to EOF** before being dropped. Without
    /// this, the underlying TCP/TLS connection cannot be returned to
    /// `reqwest`'s pool — every `request_empty` call would leak a connection
    /// slot, starving subsequent requests under load. `bytes()` consumes the
    /// body in a single contiguous read; the result is intentionally ignored
    /// (we don't need the bytes, only the side effect of consuming them).
    async fn request_empty<B>(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
    ) -> Result<()>
    where
        B: Serialize + ?Sized,
    {
        let mut req = self
            .client
            .request(method, self.endpoint(path)?)
            .query(query);
        if let Some(b) = body {
            req = req.json(b);
        }
        let response = req.send().await?.error_for_status()?;
        // Drain the body so the connection returns to the pool. Errors here
        // (truncated body, etc.) are non-fatal: the request already succeeded
        // semantically per `error_for_status`, and surfacing a body-drain
        // failure would be more confusing than helpful for an "empty" call.
        let _ = response.bytes().await;
        Ok(())
    }

    /// GET an endpoint and deserialise the JSON body.
    async fn get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        // `()` is a placeholder body type — `None` means no body is sent.
        self.request::<T, ()>(reqwest::Method::GET, path, query, None)
            .await
    }

    /// POST to an endpoint with no body, ignoring the response.
    async fn post_empty(&self, path: &str, query: &[(&str, &str)]) -> Result<()> {
        self.request_empty::<()>(reqwest::Method::POST, path, query, None)
            .await
    }

    /// POST a JSON body to an endpoint, deserialising the response.
    async fn post_json<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: &B,
    ) -> Result<R> {
        self.request(reqwest::Method::POST, path, query, Some(body))
            .await
    }

    /// POST a JSON body to an endpoint, ignoring the response.
    async fn post_json_empty<B: Serialize>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: &B,
    ) -> Result<()> {
        self.request_empty(reqwest::Method::POST, path, query, Some(body))
            .await
    }

    /// Fetch all active sessions from the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails.
    pub async fn fetch_sessions(&self) -> Result<Vec<Session>> {
        self.get_json("Sessions", &[]).await
    }

    /// Find the active mpv-shim session.
    ///
    /// Prefers same-device sessions to avoid controlling MPV on a foreign host.
    /// Within the same device, prefers the Rust mpv-shim (client="mpv-shim")
    /// over the legacy Python shim. Falls back to any session with "mpv" in
    /// the client name only when no same-device session exists, and emits a
    /// warning when doing so.
    pub async fn find_mpv_session(&self) -> Result<Option<Session>> {
        let sessions = self.fetch_sessions().await?;

        // Drop self-sessions (media-control's own connection) up front.
        let candidates: Vec<Session> = sessions
            .into_iter()
            .filter(|s| s.client != "media-control")
            .collect();

        // Prefer same-device sessions: this prevents media-control on machine A
        // from controlling MPV on machine B.
        let mut same_device: Vec<Session> = candidates
            .iter()
            .filter(|s| s.device_id == self.device_id)
            .cloned()
            .collect();

        if !same_device.is_empty() {
            // Within same-device hits, prefer the Rust mpv-shim over the
            // legacy Python shim. swap_remove is O(1) and avoids cloning.
            if let Some(idx) = same_device.iter().position(|s| s.client == "mpv-shim") {
                return Ok(Some(same_device.swap_remove(idx)));
            }
            return Ok(same_device.into_iter().next());
        }

        // Fall back to client-name matching only if no same-device session
        // exists. This is a last resort; warn so the operator notices.
        let mut mpv_sessions: Vec<Session> = candidates
            .into_iter()
            .filter(|s| s.client.to_lowercase().contains("mpv"))
            .collect();

        if mpv_sessions.is_empty() {
            return Ok(None);
        }

        tracing::warn!("no same-device MPV session found; controlling foreign device session");

        if let Some(idx) = mpv_sessions.iter().position(|s| s.client == "mpv-shim") {
            return Ok(Some(mpv_sessions.swap_remove(idx)));
        }
        Ok(mpv_sessions.into_iter().next())
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
    /// Returns an error if `session_id` contains characters unsafe for a URL
    /// path segment, or the HTTP request fails.
    pub async fn stop(&self, session_id: &str) -> Result<()> {
        validate_id("session", session_id)?;
        let path = format!("Sessions/{session_id}/Playing/Stop");
        self.post_empty(&path, &[]).await
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
    /// Returns an error if `item_id` contains characters unsafe for a URL
    /// path segment, or the HTTP request fails.
    pub async fn mark_watched(&self, item_id: &str) -> Result<()> {
        validate_id("item", item_id)?;
        let user_id = &self.user_id;
        let path = format!("Users/{user_id}/PlayedItems/{item_id}");
        self.post_empty(&path, &[]).await
    }

    /// Fetch the active MPV session and its currently playing item.
    ///
    /// Centralises the "require session, require playing item" preamble used
    /// by every "act on what's currently playing" entry point. Returns the
    /// session by value (callers usually need its `id`) and a *clone* of the
    /// now-playing item — `current_item()` borrows from `session`, and
    /// returning a borrow alongside the owner forces awkward lifetime gymnastics
    /// at every call site.
    async fn current_session_and_item(&self) -> Result<(Session, NowPlayingItem)> {
        let session = self.require_mpv_session().await?;
        let item = session
            .current_item()
            .ok_or(JellyfinError::NoPlayingItem)?
            .clone();
        Ok((session, item))
    }

    /// Mark the currently playing item as watched.
    pub async fn mark_current_watched(&self) -> Result<()> {
        let (_session, item) = self.current_session_and_item().await?;
        self.mark_watched(&item.id).await
    }

    /// Mark the current item as watched and stop playback.
    pub async fn mark_watched_and_stop(&self) -> Result<()> {
        let (session, item) = self.current_session_and_item().await?;
        self.mark_watched(&item.id).await?;
        self.stop(&session.id).await
    }

    /// Get the next-up item.
    ///
    /// `series_id = Some(id)` returns the next episode for that series; `None`
    /// returns the first global NextUp item across all shows. The two cases
    /// share the same response shape, so we keep one method instead of two.
    ///
    /// # Errors
    ///
    /// Returns an error if `series_id` is `Some` and contains characters
    /// unsafe for a URL path segment, or the HTTP request fails.
    pub async fn get_next_up(&self, series_id: Option<&str>) -> Result<Option<String>> {
        let response: NextUpResponse = match series_id {
            Some(sid) => {
                validate_id("series", sid)?;
                let path = format!("Shows/{sid}/NextUp");
                self.get_json(&path, &[("UserId", &self.user_id)]).await?
            }
            None => {
                self.get_json("Shows/NextUp", &[("UserId", &self.user_id), ("Limit", "1")])
                    .await?
            }
        };
        Ok(response.items.into_iter().next().map(|item| item.id))
    }

    /// Get the resume position (in ticks) for an item.
    ///
    /// Returns 0 if the item has never been played or has no resume position.
    ///
    /// # Errors
    ///
    /// Returns an error if `item_id` contains characters unsafe for a URL
    /// path segment, or the HTTP request fails.
    pub async fn get_item_resume_ticks(&self, item_id: &str) -> Result<i64> {
        validate_id("item", item_id)?;
        let user_id = &self.user_id;
        let path = format!("Users/{user_id}/Items/{item_id}");
        let response: ItemDetail = self.get_json(&path, &[]).await?;
        Ok(response
            .user_data
            .map_or(0, |ud| ud.playback_position_ticks))
    }

    /// Start playing an item in a session with optional resume position.
    ///
    /// Like `play_item()` but appends `StartPositionTicks` when non-zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `session_id` or `item_id` contain characters unsafe
    /// for a URL path segment, or the HTTP request fails.
    pub async fn play_item_with_resume(
        &self,
        session_id: &str,
        item_id: &str,
        start_ticks: i64,
    ) -> Result<()> {
        validate_id("session", session_id)?;
        // `item_id` rides in the query string. `reqwest`'s `.query()`
        // percent-encodes `&` and `=`, so query-string injection is *not* the
        // concern here — validation ensures the value is a well-formed
        // Jellyfin ID (path-segment safe), since the same ID is used as a path
        // entity by other endpoints (`mark_watched`, `get_item_resume_ticks`)
        // and a hostile value would otherwise sneak past those.
        validate_id("item", item_id)?;
        let path = format!("Sessions/{session_id}/Playing");
        let start_ticks_str;
        let mut query: Vec<(&str, &str)> = vec![("PlayCommand", "PlayNow"), ("ItemIds", item_id)];
        if start_ticks > 0 {
            start_ticks_str = start_ticks.to_string();
            query.push(("StartPositionTicks", &start_ticks_str));
        }
        self.post_empty(&path, &query).await
    }

    /// Start playing an item in a session (from the beginning).
    pub async fn play_item(&self, session_id: &str, item_id: &str) -> Result<()> {
        self.play_item_with_resume(session_id, item_id, 0).await
    }

    /// Start playing multiple items in a session.
    ///
    /// Takes ownership of `item_ids` to avoid an extra clone — the underlying
    /// `PlayCommand` payload owns its `Vec<String>` regardless.
    ///
    /// # Errors
    ///
    /// Returns an error if `session_id` or any element of `item_ids` contains
    /// characters unsafe for a URL path segment, or the HTTP request fails.
    pub async fn play_items(&self, session_id: &str, item_ids: Vec<String>) -> Result<()> {
        // Validate the session ID **before** the empty-list short-circuit so a
        // malformed `session_id` always surfaces as `InvalidId` — even when the
        // caller happens to pass an empty queue. Otherwise a bug calling
        // `play_items("../admin", vec![])` would silently succeed.
        validate_id("session", session_id)?;
        if item_ids.is_empty() {
            return Ok(());
        }
        for id in &item_ids {
            validate_id("item", id)?;
        }

        let playback_info = self.fetch_playback_info(&item_ids[0]).await?;
        let command = self.build_play_command(item_ids, playback_info);
        let path = format!("Sessions/{session_id}/Command/Play");
        self.post_json_empty(&path, &[], &command).await
    }

    /// Build a [`PlayCommand`] payload from an owned id list and the
    /// playback-info response for the first item.
    ///
    /// Extracted so `play_items` stays focused on transport concerns.
    fn build_play_command(
        &self,
        item_ids: Vec<String>,
        playback_info: PlaybackInfoResponse,
    ) -> PlayCommand {
        // Bind the first media source once — three downstream lookups would
        // otherwise re-walk the slice and clone redundantly.
        let first_source = playback_info.media_sources.first();
        PlayCommand {
            item_ids,
            play_command: "PlayNow".to_string(),
            play_session_id: Some(playback_info.play_session_id),
            media_source_id: first_source.map(|s| s.id.clone()),
            start_index: 0,
            start_position_ticks: 0,
            controlling_user_id: Some(self.user_id.clone()),
            audio_stream_index: first_source.and_then(|s| s.default_audio_stream_index),
            subtitle_stream_index: first_source.and_then(|s| s.default_subtitle_stream_index),
        }
    }

    /// Fetch playback info for an item.
    async fn fetch_playback_info(&self, item_id: &str) -> Result<PlaybackInfoResponse> {
        validate_id("item", item_id)?;
        let path = format!("Items/{item_id}/PlaybackInfo");
        self.post_json(
            &path,
            &[("UserId", self.user_id.as_str())],
            &serde_json::json!({}),
        )
        .await
    }

    /// Get remaining queue item IDs after the current item.
    ///
    /// Prefers matching the current position by `PlaylistItemId` (which is
    /// unique per queue occurrence) over the raw media `Id`. `Id`-based
    /// matching is only correct when the current item appears at most once in
    /// the queue; a queue with repeated items (e.g. a season where the same
    /// trailer is inserted between episodes) would otherwise always resume
    /// from the *first* occurrence.
    ///
    /// Falls back to `Id`-matching only when either side lacks a
    /// `PlaylistItemId` — older Jellyfin servers, and some code paths that
    /// synthesise a `Session`, don't populate the field.
    pub fn get_remaining_queue_ids(session: &Session, current_item_id: &str) -> Vec<String> {
        let queue = &session.now_playing_queue;
        let current_playlist_item_id = session
            .play_state
            .as_ref()
            .and_then(|ps| ps.playlist_item_id.as_deref());

        let current_idx = queue.iter().position(|item| {
            // Both sides present → match by playlist position (unique).
            if let (Some(a), Some(b)) = (item.playlist_item_id.as_deref(), current_playlist_item_id)
            {
                return a == b;
            }
            // Either side missing → fall back to raw item id (legacy path).
            item.id == current_item_id
        });

        match current_idx {
            Some(idx) if idx + 1 < queue.len() => queue[idx + 1..]
                .iter()
                .map(|item| item.id.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Advance to the next item in the queue.
    pub async fn next(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let current_id = session
            .current_item()
            .ok_or(JellyfinError::NoPlayingItem)?
            .id
            .clone();

        let remaining = Self::get_remaining_queue_ids(&session, &current_id);
        if remaining.is_empty() {
            return Ok(());
        }

        self.play_items(&session.id, remaining).await
    }

    /// Mark the current item as watched and advance to the next in queue.
    pub async fn mark_watched_and_next(&self) -> Result<()> {
        let session = self.require_mpv_session().await?;
        let item = session.current_item().ok_or(JellyfinError::NoPlayingItem)?;

        let item_id = item.id.clone();
        let series_id = item.series_id.clone();
        let session_id = session.id.clone();

        // Capture remaining queue BEFORE marking watched.
        // Marking watched can cause jellyfin-mpv-shim to clear the queue/session.
        let remaining = Self::get_remaining_queue_ids(&session, &item_id);

        self.mark_watched(&item_id).await?;

        // Try queue first, fall back to NextUp for the series
        if !remaining.is_empty() {
            return self.play_items(&session_id, remaining).await;
        }

        // Queue empty — use NextUp API to find the next episode in the series.
        // We log-and-continue on `get_next_up` failure rather than propagating:
        // marking-watched already succeeded, and a NextUp lookup miss (network
        // blip, transient 5xx) shouldn't surface as a hard error to the caller.
        // Silently swallowing the `Err` would make a misconfigured server look
        // like "no next episode" forever, so we warn.
        if let Some(sid) = series_id {
            match self.get_next_up(Some(&sid)).await {
                Ok(Some(next_id)) => {
                    return self.play_item(&session_id, &next_id).await;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        series_id = %sid,
                        error = %e,
                        "get_next_up failed after mark_watched; no next episode played"
                    );
                }
            }
        }

        Ok(())
    }

    /// Get the library that an item belongs to via the Ancestors API.
    ///
    /// # Errors
    ///
    /// Returns an error if `item_id` contains characters unsafe for a URL
    /// path segment, or the HTTP request fails.
    pub async fn get_item_library(&self, item_id: &str) -> Result<Option<LibraryInfo>> {
        validate_id("item", item_id)?;
        let path = format!("Items/{item_id}/Ancestors");
        let ancestors: Vec<AncestorItem> = self.get_json(&path, &[]).await?;

        Ok(ancestors.into_iter().find_map(|a| {
            (a.item_type == "CollectionFolder").then_some(LibraryInfo {
                id: a.id,
                name: a.name,
                collection_type: a.collection_type,
            })
        }))
    }

    /// Get unwatched items from a library with configurable sort.
    ///
    /// # Arguments
    ///
    /// * `library_id` - Parent library ID to search within
    /// * `sort_by` - Sort field — see [`SortBy`].
    /// * `sort_order` - Sort direction — see [`SortOrder`].
    /// * `exclude_id` - Optional item ID to exclude from results
    /// * `limit` - Maximum number of items to return
    ///
    /// `sort_by`/`sort_order` are typed enums (not strings) so a typo can't
    /// reach the wire and produce a mysterious server-side rejection — the
    /// type system enforces the closed set Jellyfin actually accepts.
    pub async fn get_unwatched_items(
        &self,
        library_id: &str,
        sort_by: SortBy,
        sort_order: SortOrder,
        exclude_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ItemSummary>> {
        validate_id("library", library_id)?;
        if let Some(exc) = exclude_id {
            validate_id("item", exc)?;
        }
        let path = self.user_items_path();
        let limit_str = limit.to_string();
        let mut query: Vec<(&str, &str)> = vec![
            ("ParentId", library_id),
            ("IsPlayed", "false"),
            ("Recursive", "true"),
            ("SortBy", sort_by.as_str()),
            ("SortOrder", sort_order.as_str()),
            ("Limit", &limit_str),
            ("Fields", "DateCreated,ProductionYear"),
            ("IncludeItemTypes", "Episode,Movie,MusicVideo,Video"),
        ];
        if let Some(exc) = exclude_id {
            query.push(("ExcludeItemIds", exc));
        }

        let response: ItemsResponse = self.get_json(&path, &query).await?;
        Ok(response.items)
    }

    /// Get items in a collection (box set).
    ///
    /// Returns items sorted by their index/production year within the collection.
    ///
    /// # Errors
    ///
    /// Returns an error if `collection_id` contains characters unsafe for a
    /// URL query parameter, or the HTTP request fails.
    pub async fn get_collection_items(&self, collection_id: &str) -> Result<Vec<ItemSummary>> {
        validate_id("collection", collection_id)?;
        let path = self.user_items_path();
        let query = [
            ("ParentId", collection_id),
            ("SortBy", "SortName,ProductionYear"),
            ("SortOrder", "Ascending"),
            ("Fields", "DateCreated,ProductionYear"),
        ];
        let response: ItemsResponse = self.get_json(&path, &query).await?;
        Ok(response.items)
    }

    /// Build the `Users/{user_id}/Items` path.
    ///
    /// `user_id` was validated at credential-load time (it goes into headers
    /// too) so we don't re-validate here. Centralised so the format string
    /// lives in one place.
    fn user_items_path(&self) -> String {
        let user_id = &self.user_id;
        format!("Users/{user_id}/Items")
    }

    /// Get the server URL as a string slice.
    ///
    /// Returns the canonical form produced by `Url::parse`, which may differ
    /// slightly from the raw `cred.json` value (e.g. trailing slash, lowercase
    /// scheme/host). Internal call sites should use the typed `Url` directly.
    pub fn server_url(&self) -> &str {
        self.server_url.as_str()
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

    /// Build a `Credentials` literal with placeholder fields.
    ///
    /// Five-plus tests previously open-coded the same struct literal. Putting
    /// it here keeps the test bodies focused on the actual behaviour under
    /// test, and means a future field addition only updates one place.
    fn fake_creds() -> Credentials {
        Credentials {
            server: "http://example.com".into(),
            user_id: "u1".into(),
            token: "tok".into(),
            device_id: "did".into(),
        }
    }

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
                QueueItem {
                    id: "a".to_string(),
                    playlist_item_id: None,
                },
                QueueItem {
                    id: "b".to_string(),
                    playlist_item_id: None,
                },
                QueueItem {
                    id: "c".to_string(),
                    playlist_item_id: None,
                },
                QueueItem {
                    id: "d".to_string(),
                    playlist_item_id: None,
                },
            ],
            now_playing_queue_full_items: Vec::new(),
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
    fn build_default_headers_populates_all_fields() {
        let creds = Credentials {
            token: "secret-token".into(),
            device_id: "device-id-1".into(),
            ..fake_creds()
        };
        let headers = JellyfinClient::build_default_headers(&creds, "test-host")
            .expect("headers should build");

        let auth = headers
            .get("authorization")
            .expect("authorization header present")
            .to_str()
            .expect("authorization is ASCII");
        assert!(auth.contains("Client=\"media-control\""));
        assert!(auth.contains("Device=\"test-host\""));
        assert!(auth.contains("DeviceId=\"device-id-1\""));
        assert!(auth.contains("Token=\"secret-token\""));
        assert!(auth.contains("Version=\"1.0.0\""));

        assert_eq!(
            headers.get("x-emby-token").and_then(|v| v.to_str().ok()),
            Some("secret-token")
        );
        assert_eq!(
            headers
                .get("x-emby-device-id")
                .and_then(|v| v.to_str().ok()),
            Some("device-id-1")
        );
        assert_eq!(
            headers
                .get("x-emby-device-name")
                .and_then(|v| v.to_str().ok()),
            Some("test-host")
        );
        assert_eq!(
            headers.get("x-emby-client").and_then(|v| v.to_str().ok()),
            Some("media-control")
        );

        // Sensitive headers are flagged so tracing/logging won't leak them.
        assert!(headers.get("authorization").unwrap().is_sensitive());
        assert!(headers.get("x-emby-token").unwrap().is_sensitive());
    }

    #[test]
    fn build_default_headers_sanitises_hostile_hostname() {
        let creds = fake_creds();
        // Hostname contains quote, backslash and newline — characters that
        // would either be rejected by `HeaderValue::from_str` or smuggle into
        // the `Authorization` header as quoting-corrupting characters.
        let headers = JellyfinClient::build_default_headers(&creds, "evil\"host\\\nname")
            .expect("sanitised hostname should yield valid headers");
        let auth = headers.get("authorization").unwrap().to_str().unwrap();
        assert!(!auth.contains('\n'), "newline must be stripped");
        assert!(!auth.contains('\\'), "backslash must be stripped");
        assert!(auth.contains("Device=\"evilhostname\""));
    }

    #[test]
    fn sanitize_header_value_falls_back_for_empty_input() {
        assert_eq!(JellyfinClient::sanitize_header_value(""), "media-control");
        assert_eq!(
            JellyfinClient::sanitize_header_value("!!!@@@"),
            "media-control"
        );
        assert_eq!(
            JellyfinClient::sanitize_header_value("waterbug.local"),
            "waterbug.local"
        );
        assert_eq!(
            JellyfinClient::sanitize_header_value("box-1_2.lan"),
            "box-1_2.lan"
        );
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
        assert_eq!(
            response.media_sources[0].default_audio_stream_index,
            Some(1)
        );
        assert_eq!(
            response.media_sources[0].default_subtitle_stream_index,
            Some(2)
        );
    }

    #[test]
    fn test_ancestor_item_parsing() {
        let json = r#"[
            {"Id": "lib1", "Name": "Shows", "Type": "CollectionFolder", "CollectionType": "tvshows"},
            {"Id": "season1", "Name": "Season 1", "Type": "Season"},
            {"Id": "series1", "Name": "My Show", "Type": "Series"}
        ]"#;

        let ancestors: Vec<AncestorItem> = serde_json::from_str(json).unwrap();
        assert_eq!(ancestors.len(), 3);

        let library = ancestors.iter().find(|a| a.item_type == "CollectionFolder");
        assert!(library.is_some());
        assert_eq!(library.unwrap().name, "Shows");
        assert_eq!(library.unwrap().collection_type.as_deref(), Some("tvshows"));
    }

    #[test]
    fn test_items_response_parsing() {
        let json = r#"{
            "Items": [
                {
                    "Id": "item1",
                    "Name": "Episode 1",
                    "DateCreated": "2026-03-15T10:00:00Z",
                    "IndexNumber": 1,
                    "ProductionYear": 2026,
                    "Type": "Episode"
                },
                {
                    "Id": "item2",
                    "Name": "Episode 2",
                    "DateCreated": "2026-03-16T10:00:00Z",
                    "IndexNumber": 2,
                    "ProductionYear": 2026,
                    "Type": "Episode"
                }
            ]
        }"#;

        let response: ItemsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 2);
        assert_eq!(response.items[0].id, "item1");
        assert_eq!(response.items[0].name, "Episode 1");
        assert_eq!(response.items[0].index_number, Some(1));
        assert_eq!(
            response.items[1].date_created.as_deref(),
            Some("2026-03-16T10:00:00Z")
        );
    }

    #[test]
    fn test_items_response_minimal() {
        // Items with minimal fields (no optional fields)
        let json = r#"{"Items": [{"Id": "x", "Name": "Minimal"}]}"#;
        let response: ItemsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, "x");
        assert!(response.items[0].date_created.is_none());
        assert!(response.items[0].index_number.is_none());
    }

    #[test]
    fn test_item_detail_with_resume_ticks() {
        let json = r#"{
            "Id": "abc123",
            "UserData": {
                "PlaybackPositionTicks": 54321000000
            }
        }"#;
        let detail: ItemDetail = serde_json::from_str(json).unwrap();
        assert_eq!(detail.id, "abc123");
        assert_eq!(
            detail.user_data.unwrap().playback_position_ticks,
            54321000000
        );
    }

    #[test]
    fn test_item_detail_without_user_data() {
        let json = r#"{"Id": "def456"}"#;
        let detail: ItemDetail = serde_json::from_str(json).unwrap();
        assert_eq!(detail.id, "def456");
        assert!(detail.user_data.is_none());
    }

    #[test]
    fn test_item_detail_with_zero_ticks() {
        let json = r#"{
            "Id": "ghi789",
            "UserData": {
                "PlaybackPositionTicks": 0
            }
        }"#;
        let detail: ItemDetail = serde_json::from_str(json).unwrap();
        assert_eq!(detail.user_data.unwrap().playback_position_ticks, 0);
    }

    #[test]
    fn test_session_current_item_prefers_now_playing() {
        let json = r#"{
            "Id": "sess1",
            "UserId": "user1",
            "DeviceName": "test",
            "Client": "mpv-shim",
            "NowPlayingItem": {
                "Id": "primary",
                "Name": "Primary Item",
                "Type": "Episode"
            },
            "NowPlayingQueueFullItems": [{
                "Id": "fallback",
                "Name": "Fallback Item",
                "Type": "Episode"
            }]
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.current_item().unwrap().id, "primary");
    }

    #[test]
    fn test_session_current_item_falls_back_to_queue() {
        let json = r#"{
            "Id": "sess1",
            "UserId": "user1",
            "DeviceName": "test",
            "Client": "mpv-shim",
            "NowPlayingQueueFullItems": [{
                "Id": "fallback",
                "Name": "Fallback Item",
                "Type": "Episode"
            }]
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.current_item().unwrap().id, "fallback");
    }

    #[test]
    fn test_session_current_item_none_when_empty() {
        let json = r#"{
            "Id": "sess1",
            "UserId": "user1",
            "DeviceName": "test",
            "Client": "mpv-shim"
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert!(session.current_item().is_none());
    }

    #[test]
    fn credentials_parsing_rejects_malformed_json() {
        // Anything that isn't valid JSON must surface as `CredentialsParsing`
        // (the `#[from] serde_json::Error` bridge). A user staring at the
        // log should see the parse error verbatim, not a silent fallback.
        let raw = "{ this is not json }";
        let err = serde_json::from_str::<Vec<Credentials>>(raw)
            .map_err(JellyfinError::from)
            .unwrap_err();
        assert!(matches!(err, JellyfinError::CredentialsParsing(_)));
        assert!(err.to_string().contains("failed to parse credentials"));
    }

    #[test]
    fn credentials_parsing_rejects_wrong_shape() {
        // cred.json must be an array; an object should fail with a clear
        // typed error rather than a panic on indexing later.
        let raw = r#"{"address":"x","UserId":"u","AccessToken":"t"}"#;
        let err = serde_json::from_str::<Vec<Credentials>>(raw)
            .map_err(JellyfinError::from)
            .unwrap_err();
        assert!(matches!(err, JellyfinError::CredentialsParsing(_)));
    }

    #[test]
    fn credentials_too_large_displays_size_and_cap() {
        let err = JellyfinError::CredentialsTooLarge {
            size: 999_999,
            max: 65_536,
        };
        let msg = err.to_string();
        assert!(msg.contains("999999"), "size missing: {msg}");
        assert!(msg.contains("65536"), "cap missing: {msg}");
    }

    #[test]
    fn credentials_not_found_displays_path() {
        let err = JellyfinError::CredentialsNotFound(PathBuf::from("/tmp/nope.json"));
        assert!(err.to_string().contains("/tmp/nope.json"));
    }

    #[test]
    fn invalid_credentials_names_missing_field() {
        // Static-str field name must surface verbatim; without it the user
        // can't tell *which* field was missing from `cred.json`.
        let err = JellyfinError::InvalidCredentials("no credentials in file");
        assert!(err.to_string().contains("no credentials in file"));
    }

    #[test]
    fn validate_id_accepts_real_jellyfin_ids() {
        // Standard 32-char GUIDs and dash-separated UUIDs.
        assert!(validate_id("item", "abc123def456").is_ok());
        assert!(validate_id("user", "77c2f402-7180-4d84-a2f7-8d832b89e241").is_ok());
        assert!(validate_id("device", "device_id_with_underscores").is_ok());
        assert!(validate_id("session", "session.with.dots").is_ok());
    }

    #[test]
    fn validate_id_rejects_path_traversal() {
        // The whole point of validation: path-traversal attempts must fail
        // before reaching `format!()`.
        for evil in [
            "../admin",
            "..",
            "abc/def",
            "abc\\def",
            "abc?query=1",
            "abc#frag",
            "abc%2F",
            "abc def", // space
            "abc;rm -rf /",
            "\0null",
            "abc\nnewline",
        ] {
            let err =
                validate_id("item", evil).expect_err(&format!("expected reject for {evil:?}"));
            assert!(matches!(err, JellyfinError::InvalidId { .. }));
        }
    }

    #[test]
    fn validate_id_rejects_empty() {
        // Empty IDs would produce `//` in URL paths — different endpoint.
        let err = validate_id("item", "").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidId { kind: "item", .. }));
    }

    #[test]
    fn invalid_id_displays_kind_and_value() {
        // The Display impl must surface both the kind and offending value
        // so the user can diagnose without `Debug`.
        let err = JellyfinError::InvalidId {
            kind: "item",
            value: "../admin".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("item"), "kind missing: {msg}");
        assert!(msg.contains("../admin"), "value missing: {msg}");
    }

    #[test]
    fn jellyfin_client_new_rejects_invalid_user_id() {
        // A hostile cred.json with a path-traversal user_id must be rejected
        // at construction time, not on first API call.
        let creds = Credentials {
            user_id: "../admin".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::new(creds).unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidId { kind: "user", .. }));
    }

    #[test]
    fn build_default_headers_rejects_quote_in_token() {
        // A token with a literal `"` could close the auth header's quoted
        // section and smuggle additional `Key="value"` pairs.
        let creds = Credentials {
            token: "tok\"injected".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::build_default_headers(&creds, "host").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidHeader(s) if s.contains("token")));
    }

    #[test]
    fn build_default_headers_rejects_backslash_in_device_id() {
        let creds = Credentials {
            device_id: "did\\\"smuggle".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::build_default_headers(&creds, "host").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidHeader(s) if s.contains("device_id")));
    }

    #[test]
    fn build_default_headers_rejects_crlf_in_token() {
        // CRLF in a header value could smuggle a whole new HTTP header.
        let creds = Credentials {
            token: "tok\r\nX-Admin: yes".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::build_default_headers(&creds, "host").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidHeader(_)));
    }

    #[test]
    fn build_default_headers_rejects_comma_in_token() {
        // A `,` in the token would inject a new MediaBrowser auth field
        // (e.g. `, AdminToken="x"`).
        let creds = Credentials {
            token: "tok, AdminToken=\"x\"".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::build_default_headers(&creds, "host").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidHeader(s) if s.contains("token")));
    }

    #[test]
    fn build_default_headers_rejects_equals_in_device_id() {
        // Defence in depth: even on its own, `=` shouldn't appear in a value
        // since the auth scheme uses `Key=value` syntax.
        let creds = Credentials {
            device_id: "did=injected".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::build_default_headers(&creds, "host").unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidHeader(s) if s.contains("device_id")));
    }

    #[test]
    fn jellyfin_client_new_rejects_invalid_device_id() {
        let creds = Credentials {
            device_id: "evil/id".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::new(creds).unwrap_err();
        assert!(matches!(
            err,
            JellyfinError::InvalidId { kind: "device", .. }
        ));
    }

    #[test]
    fn jellyfin_client_new_rejects_non_http_scheme() {
        // file://, gopher://, javascript: — anything but http(s) must fail.
        for evil in [
            "file:///etc/passwd",
            "gopher://example.com",
            "javascript:alert(1)",
            "ftp://example.com",
        ] {
            let creds = Credentials {
                server: evil.into(),
                ..fake_creds()
            };
            let err = JellyfinClient::new(creds).unwrap_err();
            assert!(
                matches!(err, JellyfinError::InvalidServerUrl { .. }),
                "expected InvalidServerUrl for {evil:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn jellyfin_client_new_rejects_userinfo_in_url() {
        // user:pass@ embedded in the URL would be silently merged into every
        // request — block it so a hostile cred.json can't smuggle credentials.
        let creds = Credentials {
            server: "http://attacker:pw@example.com".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::new(creds).unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidServerUrl { .. }));
    }

    #[test]
    fn jellyfin_client_new_rejects_garbage_url() {
        let creds = Credentials {
            server: "not a url at all".into(),
            ..fake_creds()
        };
        let err = JellyfinClient::new(creds).unwrap_err();
        assert!(matches!(err, JellyfinError::InvalidServerUrl { .. }));
    }

    #[test]
    fn jellyfin_client_new_accepts_valid_https_url() {
        // Sanity check: the validator must not over-reject normal inputs.
        let creds = Credentials {
            server: "https://jellyfin.example.com:8920/".into(),
            ..fake_creds()
        };
        JellyfinClient::new(creds).expect("https with trailing slash should parse");
    }

    #[test]
    fn endpoint_join_resolves_under_base_path() {
        let creds = Credentials {
            server: "http://example.com/jellyfin".into(),
            ..fake_creds()
        };
        let client = JellyfinClient::new(creds).expect("client builds");
        let url = client.endpoint("Sessions").expect("endpoint joins");
        // The base lacks a trailing `/`, but `endpoint` adds one so the API
        // path lands *under* the base, not replacing the last segment.
        assert_eq!(url.as_str(), "http://example.com/jellyfin/Sessions");
    }

    #[test]
    fn credentials_debug_redacts_token_and_device_id() {
        // The whole point of the manual `Debug` impl: a `tracing::error!(?creds)`
        // or panic backtrace must not leak the bearer token.
        let dbg = format!("{:?}", fake_creds());
        assert!(
            dbg.contains("[REDACTED]"),
            "redaction marker missing: {dbg}"
        );
        assert!(!dbg.contains("\"tok\""), "token leaked in debug: {dbg}");
        assert!(!dbg.contains("\"did\""), "device_id leaked in debug: {dbg}");
        // server and user_id are *not* secret — they should still appear so
        // logs remain useful for diagnosis.
        assert!(dbg.contains("example.com"), "server elided: {dbg}");
        assert!(dbg.contains("u1"), "user_id elided: {dbg}");
    }

    #[test]
    fn jellyfin_client_debug_redacts_user_and_device_ids() {
        let client = JellyfinClient::new(fake_creds()).expect("client builds");
        let dbg = format!("{client:?}");
        assert!(
            dbg.contains("[REDACTED]"),
            "redaction marker missing: {dbg}"
        );
        assert!(!dbg.contains("\"u1\""), "user_id leaked in debug: {dbg}");
        assert!(!dbg.contains("\"did\""), "device_id leaked in debug: {dbg}");
    }

    #[test]
    fn get_remaining_queue_ids_disambiguates_repeats_via_playlist_item_id() {
        // The same media item appears twice; without `PlaylistItemId` the
        // helper would always resume from the first occurrence.
        let session = Session {
            id: "s1".into(),
            user_id: "u1".into(),
            device_name: "test".into(),
            client: "test".into(),
            device_id: "d1".into(),
            now_playing_item: None,
            play_state: Some(PlayState {
                position_ticks: None,
                is_paused: false,
                can_seek: false,
                playlist_item_id: Some("p2".into()),
            }),
            now_playing_queue: vec![
                QueueItem {
                    id: "a".into(),
                    playlist_item_id: Some("p1".into()),
                },
                QueueItem {
                    id: "b".into(),
                    playlist_item_id: Some("p2".into()),
                },
                QueueItem {
                    id: "a".into(), // repeat of the first item
                    playlist_item_id: Some("p3".into()),
                },
                QueueItem {
                    id: "c".into(),
                    playlist_item_id: Some("p4".into()),
                },
            ],
            now_playing_queue_full_items: Vec::new(),
        };

        // `current_item_id` is `"a"` (which appears twice); without
        // playlist_item_id matching we'd return `["b", "a", "c"]` (resuming
        // from the first occurrence). With it, the position is unambiguously
        // queue index 1 (`p2`), and the remainder is `["a", "c"]`.
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "a");
        assert_eq!(remaining, vec!["a", "c"]);
    }

    #[test]
    fn get_remaining_queue_ids_falls_back_to_id_when_playlist_id_absent() {
        // Older servers don't populate PlaylistItemId — the helper must still
        // work via raw id matching.
        let session = Session {
            id: "s1".into(),
            user_id: "u1".into(),
            device_name: "test".into(),
            client: "test".into(),
            device_id: "d1".into(),
            now_playing_item: None,
            play_state: None,
            now_playing_queue: vec![
                QueueItem {
                    id: "a".into(),
                    playlist_item_id: None,
                },
                QueueItem {
                    id: "b".into(),
                    playlist_item_id: None,
                },
            ],
            now_playing_queue_full_items: Vec::new(),
        };
        let remaining = JellyfinClient::get_remaining_queue_ids(&session, "a");
        assert_eq!(remaining, vec!["b"]);
    }

    #[test]
    fn play_state_parses_playlist_item_id() {
        let json = r#"{
            "PositionTicks": 0,
            "IsPaused": false,
            "CanSeek": true,
            "PlaylistItemId": "playlistItem9"
        }"#;
        let state: PlayState = serde_json::from_str(json).unwrap();
        assert_eq!(state.playlist_item_id.as_deref(), Some("playlistItem9"));
    }

    #[test]
    fn queue_item_parses_playlist_item_id() {
        let json = r#"{"Id": "abc", "PlaylistItemId": "qi-1"}"#;
        let item: QueueItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, "abc");
        assert_eq!(item.playlist_item_id.as_deref(), Some("qi-1"));
    }

    #[test]
    fn sort_by_display_matches_wire_format() {
        // The Display output is what flows into the URL query string —
        // mismatched casing or hyphenation would silently break server-side
        // sorting. Pin every variant to the exact spelling Jellyfin expects.
        assert_eq!(SortBy::DateCreated.to_string(), "DateCreated");
        assert_eq!(SortBy::SortName.to_string(), "SortName");
        assert_eq!(SortBy::PremiereDate.to_string(), "PremiereDate");
        assert_eq!(SortBy::ProductionYear.to_string(), "ProductionYear");
        assert_eq!(SortBy::Random.to_string(), "Random");
        assert_eq!(SortBy::IndexNumber.to_string(), "IndexNumber");
    }

    #[test]
    fn sort_by_as_str_matches_display() {
        // Display and as_str must agree — the query-builder uses as_str() but
        // user-facing logs use Display, and a divergence would be a debugging
        // nightmare ("the log says X but the server got Y").
        for sb in [
            SortBy::DateCreated,
            SortBy::SortName,
            SortBy::PremiereDate,
            SortBy::ProductionYear,
            SortBy::Random,
            SortBy::IndexNumber,
        ] {
            assert_eq!(sb.as_str(), sb.to_string());
        }
    }

    #[test]
    fn sort_order_display_matches_wire_format() {
        assert_eq!(SortOrder::Ascending.to_string(), "Ascending");
        assert_eq!(SortOrder::Descending.to_string(), "Descending");
    }

    #[test]
    fn sort_order_as_str_matches_display() {
        for so in [SortOrder::Ascending, SortOrder::Descending] {
            assert_eq!(so.as_str(), so.to_string());
        }
    }

    #[test]
    fn credentials_parse_at_displays_path_and_source() {
        // The whole point of the variant: the offending file path must
        // appear in the Display output so the user doesn't have to guess
        // which credential file is malformed.
        let source = serde_json::from_str::<Vec<Credentials>>("{not json}").unwrap_err();
        let err = JellyfinError::CredentialsParseAt {
            path: PathBuf::from("/etc/jellyfin/cred.json"),
            source,
        };
        let msg = err.to_string();
        assert!(msg.contains("/etc/jellyfin/cred.json"), "path missing: {msg}");
        assert!(msg.contains("failed to parse credentials"), "lead-in missing: {msg}");
    }

    #[test]
    fn credentials_parse_at_preserves_source_chain() {
        // `#[source]` on `source` means the Error trait's `source()` chain
        // walks back to the underlying serde_json::Error — required so
        // callers can still introspect the parse-position info.
        use std::error::Error;
        let source = serde_json::from_str::<Vec<Credentials>>("{").unwrap_err();
        let err = JellyfinError::CredentialsParseAt {
            path: PathBuf::from("/x.json"),
            source,
        };
        assert!(err.source().is_some(), "source chain broken");
    }
}
