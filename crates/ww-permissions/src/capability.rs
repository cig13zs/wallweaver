use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ww_manifest::{NetworkOrigin, StatKind};

/// One thing a wallpaper can be allowed to do.
///
/// The text form (used in saved grants and over IPC) is stable:
/// `audio`, `system.stats:cpu`, `weather`, `calendar`,
/// `network:https://api.example.com`, `network:any`, `network:local`,
/// `input.mouse`, `script.eval`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Audio levels of what is playing (spectrum bars only).
    Audio,
    /// One system usage figure.
    SystemStat(StatKind),
    /// City-level weather from the weather plugin.
    Weather,
    /// Upcoming events from a calendar the user picks.
    Calendar,
    /// Contact exactly one https or wss origin.
    Network(NetworkOrigin),
    /// Contact any public https or wss origin. Local wallpapers only.
    NetworkAny,
    /// Contact this computer and the local network over any scheme.
    /// Local wallpapers only.
    LocalNetwork,
    /// Pointer position while the desktop is visible.
    InputMouse,
    /// `eval()` and `new Function()` in web wallpapers.
    ScriptEval,
}

impl Capability {
    /// True for capabilities that only the user's own wallpapers can hold.
    pub fn is_local_only(&self) -> bool {
        matches!(self, Capability::NetworkAny | Capability::LocalNetwork)
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Capability::Audio => f.write_str("audio"),
            Capability::SystemStat(s) => write!(f, "system.stats:{}", s.as_str()),
            Capability::Weather => f.write_str("weather"),
            Capability::Calendar => f.write_str("calendar"),
            Capability::Network(o) => write!(f, "network:{o}"),
            Capability::NetworkAny => f.write_str("network:any"),
            Capability::LocalNetwork => f.write_str("network:local"),
            Capability::InputMouse => f.write_str("input.mouse"),
            Capability::ScriptEval => f.write_str("script.eval"),
        }
    }
}

/// A capability string that isn't on the list.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{0}' is not a known permission")]
pub struct ParseCapabilityError(pub String);

impl FromStr for Capability {
    type Err = ParseCapabilityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseCapabilityError(s.to_string());
        Ok(match s {
            "audio" => Capability::Audio,
            "weather" => Capability::Weather,
            "calendar" => Capability::Calendar,
            "network:any" => Capability::NetworkAny,
            "network:local" => Capability::LocalNetwork,
            "input.mouse" => Capability::InputMouse,
            "script.eval" => Capability::ScriptEval,
            _ => {
                if let Some(stat) = s.strip_prefix("system.stats:") {
                    Capability::SystemStat(match stat {
                        "cpu" => StatKind::Cpu,
                        "gpu" => StatKind::Gpu,
                        "ram" => StatKind::Ram,
                        "net" => StatKind::Net,
                        _ => return Err(err()),
                    })
                } else if let Some(origin) = s.strip_prefix("network:") {
                    let parsed = NetworkOrigin::parse(origin).map_err(|_| err())?;
                    // Only the normalized spelling is accepted, so a saved
                    // grant has exactly one text form.
                    if parsed.to_string() != origin {
                        return Err(err());
                    }
                    Capability::Network(parsed)
                } else {
                    return Err(err());
                }
            }
        })
    }
}

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
