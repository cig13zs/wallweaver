//! Messages between the Wallweaver manager UI and the daemon.
//!
//! The daemon listens on a per-user local socket (a Unix domain socket in a
//! private folder, or a named pipe on Windows). Each message is a frame: a
//! 4-byte little-endian length followed by that many bytes of UTF-8 JSON,
//! at most [`MAX_FRAME`] bytes.
//!
//! A connection starts with a handshake. The client sends
//! [`ClientMessage::Hello`] with the protocol range it speaks; the daemon
//! answers [`ServerMessage::Welcome`] with the version both will use, or
//! [`ServerMessage::Rejected`]. After that the client sends
//! [`ClientMessage::Request`]s, each with an id, and the daemon answers with
//! a [`ServerMessage::Response`] carrying the same id. If the client sent
//! [`Request::Subscribe`], the daemon also pushes [`ServerMessage::Event`]s
//! at any time.
//!
//! Compatibility rules, so the UI and daemon can be updated separately:
//!
//! * fields are only ever added, never renamed or removed, within a protocol
//!   version, and new fields have defaults
//! * unknown event and response kinds decode as `Unknown` instead of failing
//! * a request the daemon doesn't know gets [`ErrorCode::Unsupported`]
//! * breaking changes bump [`PROTOCOL_VERSION`]; the daemon keeps answering
//!   versions down to [`MIN_PROTOCOL_VERSION`]

mod frame;
mod transport;

use serde::{Deserialize, Serialize};
use ww_governor::{MonitorId, Policy, Reason, State};
use ww_permissions::{Capability, Risk};

pub use frame::{FrameError, MAX_FRAME, read_frame, write_frame};
pub use transport::{Client, Connection, Endpoint, Incoming, IpcError, Server, unsupported};

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The oldest protocol version this build still accepts.
pub const MIN_PROTOCOL_VERSION: u32 = 1;

/// Pick the protocol version for a client that speaks `min..=max`, or
/// `None` if the ranges don't overlap.
pub fn negotiate(client_min: u32, client_max: u32) -> Option<u32> {
    let v = client_max.min(PROTOCOL_VERSION);
    (v >= client_min && v >= MIN_PROTOCOL_VERSION).then_some(v)
}

/// Anything a client sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    /// First message on every connection.
    Hello {
        /// Oldest protocol version the client speaks.
        min_protocol: u32,
        /// Newest protocol version the client speaks.
        max_protocol: u32,
        /// Free-form client name for logs, such as `wallweaver-ui 0.1.0`.
        client: String,
    },
    /// A request; the daemon answers with a response carrying the same id.
    Request {
        /// Chosen by the client, echoed in the response.
        id: u64,
        /// What to do.
        request: Request,
    },
}

/// Anything the daemon sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    /// Handshake accepted.
    Welcome {
        /// The protocol version used from now on.
        protocol: u32,
        /// Daemon version, for display.
        daemon_version: String,
    },
    /// Handshake refused; the daemon closes the connection.
    Rejected {
        /// Why, in plain words.
        reason: String,
    },
    /// Answer to a request.
    Response {
        /// The request's id.
        id: u64,
        /// The outcome.
        result: Result<Response, RemoteError>,
    },
    /// Something changed. Only sent after [`Request::Subscribe`].
    Event {
        /// What changed.
        event: Event,
    },
}

/// Which monitor a request is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorTarget {
    /// Every monitor.
    All,
    /// One monitor by id.
    Id(MonitorId),
}

/// Where a wallpaper comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "from",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WallpaperSource {
    /// A package installed in the library, by id.
    Installed {
        /// Package id.
        id: String,
    },
    /// A folder with a `wallpaper.json` on this machine (local trust).
    Folder {
        /// Absolute path.
        path: String,
    },
    /// A single file the user dropped in: image, GIF, shader, video or web
    /// page (local trust).
    File {
        /// Absolute path.
        path: String,
    },
}

/// Requests a client can make.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    /// Daemon and per-monitor status.
    Status,
    /// Connected monitors.
    ListMonitors,
    /// Show a wallpaper.
    SetWallpaper {
        /// Where.
        monitor: MonitorTarget,
        /// What.
        source: WallpaperSource,
    },
    /// Remove the wallpaper and release its resources.
    ClearWallpaper {
        /// Where.
        monitor: MonitorTarget,
    },
    /// Pause every wallpaper until [`Request::ResumeAll`].
    PauseAll,
    /// Undo [`Request::PauseAll`] or [`Request::StopAll`].
    ResumeAll,
    /// Stop every wallpaper and free GPU memory.
    StopAll,
    /// Current governor policy.
    GetPolicy,
    /// Replace the governor policy. Values out of range are clamped.
    SetPolicy {
        /// The new policy.
        policy: Policy,
    },
    /// Change user-adjustable properties of the wallpaper on a monitor.
    SetProperties {
        /// Where.
        monitor: MonitorTarget,
        /// Property name to new value.
        values: serde_json::Map<String, serde_json::Value>,
    },
    /// What a wallpaper asks for, as consent-screen lines.
    GetConsent {
        /// The wallpaper.
        source: WallpaperSource,
    },
    /// Record the user's approval. Refused for anything a gallery wallpaper
    /// did not declare.
    GrantPermissions {
        /// Package id.
        package_id: String,
        /// What to allow.
        capabilities: Vec<Capability>,
    },
    /// Withdraw approvals.
    RevokePermissions {
        /// Package id.
        package_id: String,
        /// What to withdraw.
        capabilities: Vec<Capability>,
    },
    /// Start receiving events on this connection.
    Subscribe,
    /// Ask the daemon to exit.
    Shutdown,
}

/// Successful answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Response {
    /// Done, nothing to report.
    Ok,
    /// Answer to [`Request::Status`].
    Status(DaemonStatus),
    /// Answer to [`Request::ListMonitors`].
    Monitors {
        /// Connected monitors.
        monitors: Vec<MonitorInfo>,
    },
    /// Answer to [`Request::GetPolicy`] and [`Request::SetPolicy`].
    Policy {
        /// The active policy.
        policy: Policy,
    },
    /// Answer to [`Request::GetConsent`].
    Consent {
        /// Package id.
        package_id: String,
        /// One line per capability, riskiest first.
        items: Vec<ConsentLine>,
    },
    /// A response kind this build doesn't know.
    #[serde(other)]
    Unknown,
}

/// The daemon's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    /// Daemon version.
    pub version: String,
    /// Protocol version in use.
    pub protocol: u32,
    /// Milliseconds since the daemon started.
    pub uptime_ms: u64,
    /// Resident memory of the daemon process, if known.
    #[serde(default)]
    pub rss_bytes: Option<u64>,
    /// Graphics adapter in use, if one is open.
    #[serde(default)]
    pub adapter: Option<String>,
    /// Per-monitor state.
    pub monitors: Vec<MonitorStatus>,
}

/// One monitor as the daemon sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// Id used in requests.
    pub id: MonitorId,
    /// Name from the OS.
    pub name: String,
    /// Position and size in desktop pixels.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// True for the primary monitor.
    pub primary: bool,
}

/// What one monitor is showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    /// The monitor.
    pub monitor: MonitorInfo,
    /// The wallpaper, if any.
    #[serde(default)]
    pub wallpaper: Option<WallpaperInfo>,
    /// Governor state, if a wallpaper is set.
    #[serde(default)]
    pub state: Option<State>,
    /// Why it is in that state.
    #[serde(default)]
    pub reason: Option<Reason>,
}

/// A wallpaper that is set on a monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperInfo {
    /// Package id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Manifest type, such as `shader`.
    pub kind: String,
    /// Where it came from.
    pub source: WallpaperSource,
}

/// One consent-screen line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentLine {
    /// The capability.
    pub capability: Capability,
    /// Short summary.
    pub title: String,
    /// What exactly is shared.
    pub detail: String,
    /// How to flag it.
    pub risk: RiskLevel,
}

/// [`Risk`] on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

impl From<Risk> for RiskLevel {
    fn from(r: Risk) -> Self {
        match r {
            Risk::Low => RiskLevel::Low,
            Risk::Medium => RiskLevel::Medium,
            Risk::High => RiskLevel::High,
        }
    }
}

impl From<ww_permissions::ConsentItem> for ConsentLine {
    fn from(item: ww_permissions::ConsentItem) -> Self {
        ConsentLine {
            capability: item.capability,
            title: item.title,
            detail: item.detail,
            risk: item.risk.into(),
        }
    }
}

/// Things the daemon reports without being asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// The governor moved a monitor to a new state.
    StateChanged {
        /// The monitor.
        monitor: MonitorId,
        /// New state.
        state: State,
        /// Why.
        reason: Reason,
    },
    /// A wallpaper was set or cleared.
    WallpaperChanged {
        /// The monitor.
        monitor: MonitorId,
        /// The new wallpaper, or `None` if cleared.
        wallpaper: Option<WallpaperInfo>,
    },
    /// Monitors were connected, removed or rearranged.
    MonitorsChanged {
        /// The new list.
        monitors: Vec<MonitorInfo>,
    },
    /// An event kind this build doesn't know.
    #[serde(other)]
    Unknown,
}

/// Error categories a client can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// The request is malformed or names something that doesn't exist.
    BadRequest,
    /// The daemon doesn't know this request.
    Unsupported,
    /// Refused by the permission model.
    PermissionDenied,
    /// The wallpaper failed to load or verify.
    InvalidWallpaper,
    /// No GPU, no desktop host, or another platform problem.
    Platform,
    /// Anything else.
    Internal,
    /// An error code this build doesn't know.
    #[serde(other)]
    Unknown,
}

/// An error reported by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct RemoteError {
    /// Category.
    pub code: ErrorCode,
    /// Plain-language message, safe to show to the user.
    pub message: String,
}

impl RemoteError {
    /// Build an error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        RemoteError {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests;
