use std::net::IpAddr;

use url::{Host, Url};
use ww_manifest::NetworkOrigin;

use crate::{Capability, Effective};

/// Outcome of a network check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkDecision {
    /// The request may go ahead.
    Allow,
    /// Blocked: the wallpaper has no network permission that covers it.
    DenyNotGranted,
    /// Blocked: plain http/ws or another scheme without the local-network grant.
    DenyInsecure,
    /// Blocked: this computer or the local network without the local-network grant.
    DenyLocal,
}

impl NetworkDecision {
    /// True only for [`NetworkDecision::Allow`].
    pub fn is_allowed(self) -> bool {
        self == NetworkDecision::Allow
    }
}

/// The host-side network filter for one wallpaper. Web hosts call it from
/// their request interception hook as a second line of defense behind CSP.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkPolicy {
    origins: Vec<NetworkOrigin>,
    any_public: bool,
    local: bool,
}

impl NetworkPolicy {
    pub(crate) fn from_effective(e: &Effective) -> NetworkPolicy {
        let mut p = NetworkPolicy::default();
        for cap in e.iter() {
            match cap {
                Capability::Network(o) => p.origins.push(o.clone()),
                Capability::NetworkAny => p.any_public = true,
                Capability::LocalNetwork => p.local = true,
                _ => {}
            }
        }
        p
    }

    /// Decide whether a request to `url` may leave the machine. Requests for
    /// the wallpaper's own `ww:` files never come here.
    pub fn check(&self, url: &Url) -> NetworkDecision {
        let local_target = is_local_host(url);
        if local_target {
            return if self.local {
                NetworkDecision::Allow
            } else {
                NetworkDecision::DenyLocal
            };
        }
        match url.scheme() {
            "https" | "wss" => {}
            "http" | "ws" if self.local => {
                // Local-network wallpapers may use plain http, but only to
                // local targets, which returned above.
                return NetworkDecision::DenyInsecure;
            }
            _ => return NetworkDecision::DenyInsecure,
        }
        if self.any_public || self.origins.iter().any(|o| o.matches(url)) {
            NetworkDecision::Allow
        } else {
            NetworkDecision::DenyNotGranted
        }
    }

    /// Convenience: parse and check a URL string. Unparseable URLs are denied.
    pub fn check_str(&self, url: &str) -> NetworkDecision {
        match Url::parse(url) {
            Ok(u) => self.check(&u),
            Err(_) => NetworkDecision::DenyNotGranted,
        }
    }
}

/// True for loopback, private, link-local and other non-public addresses,
/// and for names that resolve locally by convention.
fn is_local_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => !is_public_ip(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => !is_public_ip(IpAddr::V6(ip)),
        Some(Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            d == "localhost"
                || !d.contains('.')
                || [
                    ".localhost",
                    ".local",
                    ".localdomain",
                    ".internal",
                    ".intranet",
                    ".lan",
                    ".home",
                    ".home.arpa",
                    ".corp",
                ]
                .iter()
                .any(|s| d.ends_with(s))
        }
        None => true,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 carrier NAT
                || (o[0] == 198 && (o[1] & 0xfe) == 18)) // 198.18.0.0/15 benchmarking
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00 // unique local
                || (seg[0] & 0xffc0) == 0xfe80) // link local
        }
    }
}
