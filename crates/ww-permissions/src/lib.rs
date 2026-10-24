//! Wallweaver's permission model.
//!
//! Everything is denied unless two things are true: the user approved it,
//! and the wallpaper is allowed to hold it. What a wallpaper may hold depends
//! on where it came from:
//!
//! * [`Trust::Gallery`] covers anything made by someone else, signed or not.
//!   It can only be granted what its `wallpaper.json` declares, and only the
//!   forms the manifest format allows (https/wss origins on public domains).
//! * [`Trust::Local`] covers wallpapers the user made or dropped in
//!   themselves. The user can grant them anything on the list, including
//!   capabilities the manifest never declared, such as any https origin or
//!   the local network.
//!
//! Grants are recorded per package id *and* signer, so a package re-signed
//! by a different key starts again from nothing.
//!
//! The [`Effective`] set built from a manifest, its trust level and the
//! user's grants is the single thing the rest of the app consults: the
//! renderer asks it about mouse input, the web host takes its CSP header and
//! network filter, and the manager UI shows its consent lines.

mod capability;
mod consent;
mod csp;
mod network;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ww_manifest::Manifest;

pub use capability::{Capability, ParseCapabilityError};
pub use consent::{ConsentItem, Risk, consent_items};
pub use csp::content_security_policy;
pub use network::{NetworkDecision, NetworkPolicy};

/// Where a wallpaper came from, which decides what it may be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    /// Made by someone else: gallery downloads, shared `.wwp` files.
    Gallery,
    /// Made or imported by the user on this machine.
    Local,
}

/// Why a grant was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// A gallery wallpaper asked for something its manifest doesn't declare.
    #[error(
        "'{0}' is not declared in this wallpaper's manifest, and wallpapers from other people can only be granted what they declare"
    )]
    NotDeclared(Capability),
    /// A capability only local wallpapers can hold.
    #[error("'{0}' can only be granted to your own local wallpapers")]
    LocalOnly(Capability),
}

/// The capabilities a manifest declares, in a stable order.
pub fn declared(manifest: &Manifest) -> BTreeSet<Capability> {
    let p = &manifest.permissions;
    let mut out = BTreeSet::new();
    if p.audio {
        out.insert(Capability::Audio);
    }
    for stat in &p.system_stats {
        out.insert(Capability::SystemStat(*stat));
    }
    if p.weather {
        out.insert(Capability::Weather);
    }
    if p.calendar {
        out.insert(Capability::Calendar);
    }
    for origin in p.network_origins() {
        out.insert(Capability::Network(origin));
    }
    if p.input_mouse {
        out.insert(Capability::InputMouse);
    }
    if p.script_eval {
        out.insert(Capability::ScriptEval);
    }
    out
}

/// Capabilities in `new` that `old` did not declare. A non-empty result
/// means an update widens what the wallpaper asks for, and the user must
/// see a consent screen again before the update is applied.
pub fn widened(old: &Manifest, new: &Manifest) -> Vec<Capability> {
    let before = declared(old);
    declared(new)
        .into_iter()
        .filter(|c| !before.contains(c))
        .collect()
}

/// The user's decisions for one package from one signer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRecord {
    /// Package id from the manifest.
    pub package_id: String,
    /// Key id of the signer, or `None` for unsigned local wallpapers.
    pub signer: Option<String>,
    /// Trust level the grants were made under.
    pub trust: Trust,
    /// What the user approved.
    pub granted: BTreeSet<Capability>,
}

impl GrantRecord {
    /// An empty record: nothing granted.
    pub fn new(package_id: impl Into<String>, signer: Option<String>, trust: Trust) -> Self {
        GrantRecord {
            package_id: package_id.into(),
            signer,
            trust,
            granted: BTreeSet::new(),
        }
    }

    /// Record the user's approval of one capability, if this wallpaper is
    /// allowed to hold it.
    pub fn grant(&mut self, manifest: &Manifest, cap: Capability) -> Result<(), GrantError> {
        check_grantable(manifest, self.trust, &cap)?;
        self.granted.insert(cap);
        Ok(())
    }

    /// Approve everything the manifest declares (the consent screen's
    /// "Allow" button). Never adds anything undeclared.
    pub fn grant_declared(&mut self, manifest: &Manifest) {
        self.granted.extend(declared(manifest));
    }

    /// Withdraw one capability. Revoking something never granted is fine.
    pub fn revoke(&mut self, cap: &Capability) {
        self.granted.remove(cap);
    }
}

/// Check whether a wallpaper of the given trust may hold `cap` at all.
pub fn check_grantable(
    manifest: &Manifest,
    trust: Trust,
    cap: &Capability,
) -> Result<(), GrantError> {
    match trust {
        Trust::Local => Ok(()),
        Trust::Gallery => {
            if cap.is_local_only() {
                Err(GrantError::LocalOnly(cap.clone()))
            } else if declared(manifest).contains(cap) {
                Ok(())
            } else {
                Err(GrantError::NotDeclared(cap.clone()))
            }
        }
    }
}

/// What a running wallpaper may actually do right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Effective {
    allowed: BTreeSet<Capability>,
}

impl Effective {
    /// Nothing allowed. What every wallpaper gets before the user decides.
    pub fn deny_all() -> Effective {
        Effective::default()
    }

    /// Combine a manifest, the wallpaper's trust and signer, and the user's
    /// saved record. A record for another package, another signer or a
    /// different trust level counts for nothing. Gallery grants that the
    /// current manifest no longer declares are dropped, so a stale record can
    /// never widen an updated package.
    pub fn resolve(
        manifest: &Manifest,
        trust: Trust,
        signer: Option<&str>,
        record: Option<&GrantRecord>,
    ) -> Effective {
        let Some(record) = record else {
            return Effective::deny_all();
        };
        if record.package_id != manifest.id
            || record.signer.as_deref() != signer
            || record.trust != trust
        {
            return Effective::deny_all();
        }
        let allowed = record
            .granted
            .iter()
            .filter(|c| check_grantable(manifest, trust, c).is_ok())
            .cloned()
            .collect();
        Effective { allowed }
    }

    /// True if `cap` is allowed.
    pub fn allows(&self, cap: &Capability) -> bool {
        self.allowed.contains(cap)
    }

    /// Every allowed capability.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.allowed.iter()
    }

    /// True if nothing is allowed.
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    /// The network filter for this wallpaper.
    pub fn network(&self) -> NetworkPolicy {
        NetworkPolicy::from_effective(self)
    }

    /// The CSP header value for this wallpaper's web view.
    pub fn csp(&self) -> String {
        content_security_policy(self)
    }
}

#[cfg(test)]
mod tests;
