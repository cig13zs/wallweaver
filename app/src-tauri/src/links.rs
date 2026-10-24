//! The only external pages the manager can open.

use crate::dto::LinkId;

const REPO: &str = "https://github.com/cig13zs/wallweaver";

/// Maps a link id to its page. `subject` (a creator or wallpaper id) only ever
/// fills a query value and is percent-encoded, so it cannot change the host or path.
pub fn url_for(id: LinkId, subject: Option<&str>) -> String {
    match id {
        LinkId::Source => REPO.to_owned(),
        LinkId::Funding | LinkId::Sponsor | LinkId::Tip | LinkId::Store => {
            format!("{REPO}#support")
        }
        LinkId::Report => match subject {
            Some(s) if !s.is_empty() => format!(
                "{REPO}/issues/new?labels=report&title=Report%3A%20{}",
                encode(s)
            ),
            _ => format!("{REPO}/issues/new?labels=report"),
        },
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes().take(120) {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
