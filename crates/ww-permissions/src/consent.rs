use ww_manifest::StatKind;

use crate::Capability;

/// How much a capability should worry the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Risk {
    /// Read-only data that says little about you.
    Low,
    /// Data about you or your machine, or contact with one named site.
    Medium,
    /// Broad access: running generated code, any site, your local network.
    High,
}

/// One line on the consent screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentItem {
    /// The capability this line is about.
    pub capability: Capability,
    /// Short plain-language summary, e.g. "See what's playing".
    pub title: String,
    /// One sentence on exactly what is shared.
    pub detail: String,
    /// How the UI should flag it.
    pub risk: Risk,
}

/// Plain-language lines for a set of capabilities, riskiest first.
pub fn consent_items<'a>(caps: impl IntoIterator<Item = &'a Capability>) -> Vec<ConsentItem> {
    let mut items: Vec<ConsentItem> = caps.into_iter().map(describe).collect();
    items.sort_by(|a, b| b.risk.cmp(&a.risk).then(a.capability.cmp(&b.capability)));
    items
}

fn describe(cap: &Capability) -> ConsentItem {
    let (title, detail, risk) = match cap {
        Capability::Audio => (
            "See what's playing (audio levels only)".to_string(),
            "Gets 64 volume bars of the sound your computer plays. Never the microphone, never the audio itself.".to_string(),
            Risk::Low,
        ),
        Capability::SystemStat(kind) => {
            let what = match kind {
                StatKind::Cpu => "CPU usage",
                StatKind::Gpu => "GPU usage",
                StatKind::Ram => "memory usage",
                StatKind::Net => "network speed",
            };
            (
                format!("Read your {what}"),
                format!("Gets your total {what} once a second. No process names, files or addresses."),
                Risk::Low,
            )
        }
        Capability::Weather => (
            "Show local weather".to_string(),
            "Gets city-level weather from Wallweaver. The wallpaper never learns your exact location.".to_string(),
            Risk::Medium,
        ),
        Capability::Calendar => (
            "Show upcoming events".to_string(),
            "Gets the titles and times of upcoming events from a calendar you choose.".to_string(),
            Risk::Medium,
        ),
        Capability::Network(origin) => (
            format!("Contact {}", origin.host()),
            format!("Can send and receive data from {origin} and nowhere else."),
            Risk::Medium,
        ),
        Capability::NetworkAny => (
            "Contact any website".to_string(),
            "Can send and receive data from any public https site. Only for your own wallpapers.".to_string(),
            Risk::High,
        ),
        Capability::LocalNetwork => (
            "Contact this computer and your local network".to_string(),
            "Can reach services on your machine and devices on your network, such as a router or a home server. Only for your own wallpapers.".to_string(),
            Risk::High,
        ),
        Capability::InputMouse => (
            "Follow your mouse pointer".to_string(),
            "Gets the pointer position while the desktop is showing. No clicks, no keys.".to_string(),
            Risk::Low,
        ),
        Capability::ScriptEval => (
            "Run code it builds while running".to_string(),
            "Allows eval() and new Function(). This makes the wallpaper harder to review.".to_string(),
            Risk::High,
        ),
    };
    ConsentItem {
        capability: cap.clone(),
        title,
        detail,
        risk,
    }
}
