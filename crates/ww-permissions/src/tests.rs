use std::str::FromStr;

use proptest::prelude::*;
use ww_manifest::{Manifest, StatKind};

use super::*;

fn manifest(perms: &str) -> Manifest {
    Manifest::from_json(&format!(
        r#"{{
          "manifestVersion": 1, "id": "mira.loom", "name": "Loom", "version": "1.0.0",
          "type": "web", "entry": "index.html", "author": {{ "name": "mira" }},
          "license": "MIT", "permissions": {perms}
        }}"#
    ))
    .unwrap()
}

fn cap(s: &str) -> Capability {
    Capability::from_str(s).unwrap()
}

fn full() -> Manifest {
    manifest(
        r#"{ "audio": true, "system.stats": ["cpu", "ram"], "network": ["https://api.open-meteo.com"], "input.mouse": true }"#,
    )
}

#[test]
fn nothing_is_allowed_without_a_record() {
    let m = full();
    let e = Effective::resolve(&m, Trust::Gallery, Some("AAAA"), None);
    assert!(e.is_empty());
    assert!(!e.allows(&Capability::Audio));
    assert_eq!(
        e.network().check_str("https://api.open-meteo.com/v1"),
        NetworkDecision::DenyNotGranted
    );
}

#[test]
fn declared_lists_everything_in_the_manifest() {
    let caps: Vec<String> = declared(&full()).iter().map(|c| c.to_string()).collect();
    assert_eq!(
        caps,
        [
            "audio",
            "system.stats:cpu",
            "system.stats:ram",
            "network:https://api.open-meteo.com",
            "input.mouse"
        ]
    );
    assert!(declared(&manifest("{}")).is_empty());
}

#[test]
fn gallery_wallpapers_only_get_what_they_declare() {
    let m = full();
    let mut rec = GrantRecord::new("mira.loom", Some("K1".into()), Trust::Gallery);
    rec.grant(&m, Capability::Audio).unwrap();
    assert_eq!(
        rec.grant(&m, Capability::Weather),
        Err(GrantError::NotDeclared(Capability::Weather))
    );
    assert_eq!(
        rec.grant(&m, cap("network:https://evil.example")),
        Err(GrantError::NotDeclared(cap("network:https://evil.example")))
    );
    assert_eq!(
        rec.grant(&m, Capability::NetworkAny),
        Err(GrantError::LocalOnly(Capability::NetworkAny))
    );
    assert_eq!(
        rec.grant(&m, Capability::LocalNetwork),
        Err(GrantError::LocalOnly(Capability::LocalNetwork))
    );
    let e = Effective::resolve(&m, Trust::Gallery, Some("K1"), Some(&rec));
    assert!(e.allows(&Capability::Audio));
    assert!(
        !e.allows(&Capability::InputMouse),
        "declared but not approved"
    );
}

#[test]
fn local_wallpapers_can_be_granted_anything() {
    let m = manifest("{}");
    let mut rec = GrantRecord::new("mira.loom", None, Trust::Local);
    for c in ["network:any", "network:local", "script.eval", "weather"] {
        rec.grant(&m, cap(c)).unwrap();
    }
    let e = Effective::resolve(&m, Trust::Local, None, Some(&rec));
    assert!(e.allows(&Capability::LocalNetwork));
    let net = e.network();
    assert!(net.check_str("https://anything.example/x").is_allowed());
    assert!(net.check_str("http://192.168.1.10:8123/api").is_allowed());
    assert!(net.check_str("ws://localhost:9000").is_allowed());
    assert_eq!(
        net.check_str("http://example.com"),
        NetworkDecision::DenyInsecure,
        "plain http only to local targets"
    );
}

#[test]
fn records_for_other_packages_signers_or_trust_count_for_nothing() {
    let m = full();
    let mut rec = GrantRecord::new("mira.loom", Some("K1".into()), Trust::Gallery);
    rec.grant_declared(&m);
    assert!(!Effective::resolve(&m, Trust::Gallery, Some("K1"), Some(&rec)).is_empty());
    assert!(Effective::resolve(&m, Trust::Gallery, Some("K2"), Some(&rec)).is_empty());
    assert!(Effective::resolve(&m, Trust::Gallery, None, Some(&rec)).is_empty());
    assert!(Effective::resolve(&m, Trust::Local, Some("K1"), Some(&rec)).is_empty());
    let mut other = rec.clone();
    other.package_id = "someone.else".into();
    assert!(Effective::resolve(&m, Trust::Gallery, Some("K1"), Some(&other)).is_empty());
}

#[test]
fn stale_gallery_grants_cannot_widen_an_update() {
    let old = full();
    let mut rec = GrantRecord::new("mira.loom", Some("K1".into()), Trust::Gallery);
    rec.grant_declared(&old);
    // A tampered record claims more than was ever declared.
    rec.granted.insert(Capability::NetworkAny);
    rec.granted.insert(Capability::Weather);
    let updated = manifest(r#"{ "audio": true }"#);
    let e = Effective::resolve(&updated, Trust::Gallery, Some("K1"), Some(&rec));
    let allowed: Vec<_> = e.iter().cloned().collect();
    assert_eq!(allowed, vec![Capability::Audio]);
}

#[test]
fn widened_reports_only_new_requests() {
    let old = manifest(r#"{ "audio": true, "network": ["https://a.example"] }"#);
    let new = manifest(
        r#"{ "audio": true, "network": ["https://a.example", "https://b.example"], "script.eval": true }"#,
    );
    assert_eq!(
        widened(&old, &new),
        vec![cap("network:https://b.example"), Capability::ScriptEval]
    );
    assert!(widened(&new, &old).is_empty());
}

#[test]
fn revoke_removes_access() {
    let m = full();
    let mut rec = GrantRecord::new("mira.loom", None, Trust::Gallery);
    rec.grant_declared(&m);
    rec.revoke(&Capability::Audio);
    rec.revoke(&Capability::Weather);
    let e = Effective::resolve(&m, Trust::Gallery, None, Some(&rec));
    assert!(!e.allows(&Capability::Audio));
    assert!(e.allows(&Capability::InputMouse));
}

#[test]
fn csp_is_default_deny() {
    let csp = Effective::deny_all().csp();
    assert!(csp.starts_with("default-src 'none'"));
    assert!(csp.contains("connect-src ww:;"));
    assert!(csp.contains("script-src ww: 'wasm-unsafe-eval';"));
    assert!(!csp.contains("'unsafe-eval'"));
    for d in [
        "frame-src 'none'",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'none'",
    ] {
        assert!(csp.contains(d), "{d}");
    }
    assert!(!csp.contains("http:"));
}

#[test]
fn csp_lists_granted_origins_and_eval() {
    let m = manifest(
        r#"{ "network": ["https://api.open-meteo.com", "wss://live.example.org:8443"], "script.eval": true }"#,
    );
    let mut rec = GrantRecord::new("mira.loom", None, Trust::Gallery);
    rec.grant_declared(&m);
    let csp = Effective::resolve(&m, Trust::Gallery, None, Some(&rec)).csp();
    assert!(
        csp.contains("connect-src ww: https://api.open-meteo.com wss://live.example.org:8443;"),
        "{csp}"
    );
    assert!(csp.contains("'unsafe-eval'"));
}

#[test]
fn csp_for_local_network_grants() {
    let m = manifest("{}");
    let mut rec = GrantRecord::new("mira.loom", None, Trust::Local);
    rec.grant(&m, Capability::LocalNetwork).unwrap();
    let csp = Effective::resolve(&m, Trust::Local, None, Some(&rec)).csp();
    assert!(
        csp.contains("connect-src ww: http: ws: https: wss:"),
        "{csp}"
    );
}

#[test]
fn network_filter_blocks_lookalikes_and_local_targets() {
    let m = full();
    let mut rec = GrantRecord::new("mira.loom", None, Trust::Gallery);
    rec.grant_declared(&m);
    let net = Effective::resolve(&m, Trust::Gallery, None, Some(&rec)).network();
    assert!(
        net.check_str("https://api.open-meteo.com/v1/forecast")
            .is_allowed()
    );
    assert!(net.check_str("https://API.open-meteo.com./v1").is_allowed());
    for (url, want) in [
        ("http://api.open-meteo.com/", NetworkDecision::DenyInsecure),
        (
            "https://api.open-meteo.com.evil.net/",
            NetworkDecision::DenyNotGranted,
        ),
        (
            "https://api.open-meteo.com:444/",
            NetworkDecision::DenyNotGranted,
        ),
        ("https://127.0.0.1/", NetworkDecision::DenyLocal),
        ("https://[::1]/", NetworkDecision::DenyLocal),
        ("https://[::ffff:10.0.0.1]/", NetworkDecision::DenyLocal),
        ("https://169.254.169.254/latest", NetworkDecision::DenyLocal),
        ("https://100.64.1.1/", NetworkDecision::DenyLocal),
        ("https://router.lan/", NetworkDecision::DenyLocal),
        ("https://printer/", NetworkDecision::DenyLocal),
        ("file:///etc/passwd", NetworkDecision::DenyLocal),
        ("ftp://files.example.com/", NetworkDecision::DenyInsecure),
        ("not a url", NetworkDecision::DenyNotGranted),
    ] {
        assert_eq!(net.check_str(url), want, "{url}");
    }
}

#[test]
fn consent_lines_are_plain_and_riskiest_first() {
    let caps = [
        Capability::Audio,
        Capability::ScriptEval,
        cap("network:https://api.open-meteo.com"),
        Capability::SystemStat(StatKind::Cpu),
    ];
    let items = consent_items(&caps);
    assert_eq!(items[0].capability, Capability::ScriptEval);
    assert_eq!(items[0].risk, Risk::High);
    assert_eq!(items[1].title, "Contact api.open-meteo.com");
    assert!(
        items
            .iter()
            .any(|i| i.title == "See what's playing (audio levels only)")
    );
    assert!(items.iter().any(|i| i.title == "Read your CPU usage"));
    for item in &items {
        assert!(!item.detail.is_empty());
    }
}

#[test]
fn capability_text_form_is_strict() {
    for good in [
        "audio",
        "system.stats:gpu",
        "network:https://api.example.com",
        "network:wss://x.example.org:8443",
        "network:any",
        "network:local",
        "input.mouse",
        "script.eval",
        "weather",
        "calendar",
    ] {
        assert_eq!(cap(good).to_string(), good);
    }
    for bad in [
        "Audio",
        "system.stats:disk",
        "network:http://x.example.com",
        "network:https://X.example.com",
        "network:https://x.example.com/",
        "filesystem",
        "",
    ] {
        assert!(Capability::from_str(bad).is_err(), "{bad}");
    }
}

#[test]
fn grant_records_round_trip_as_json() {
    let m = full();
    let mut rec = GrantRecord::new("mira.loom", Some("ABCD".into()), Trust::Gallery);
    rec.grant_declared(&m);
    let json = serde_json::to_string(&rec).unwrap();
    assert!(
        json.contains("\"network:https://api.open-meteo.com\""),
        "{json}"
    );
    let back: GrantRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rec);
    assert!(serde_json::from_str::<GrantRecord>(&json.replace("audio", "microphone")).is_err());
}

fn any_capability() -> impl Strategy<Value = Capability> {
    prop_oneof![
        Just(Capability::Audio),
        Just(Capability::SystemStat(StatKind::Cpu)),
        Just(Capability::SystemStat(StatKind::Net)),
        Just(Capability::Weather),
        Just(Capability::Calendar),
        Just(Capability::NetworkAny),
        Just(Capability::LocalNetwork),
        Just(Capability::InputMouse),
        Just(Capability::ScriptEval),
        "[a-z]{1,8}\\.(com|org|net)".prop_map(|h| cap(&format!("network:https://{h}"))),
    ]
}

proptest! {
    /// Whatever ends up in a saved record, a gallery wallpaper's effective
    /// set is always a subset of what its manifest declares.
    #[test]
    fn gallery_effective_is_subset_of_declared(caps in proptest::collection::vec(any_capability(), 0..12)) {
        let m = full();
        let mut rec = GrantRecord::new("mira.loom", None, Trust::Gallery);
        rec.granted.extend(caps);
        let e = Effective::resolve(&m, Trust::Gallery, None, Some(&rec));
        let decl = declared(&m);
        for c in e.iter() {
            prop_assert!(decl.contains(c));
            prop_assert!(!c.is_local_only());
        }
    }

    /// The text form of every capability parses back to itself.
    #[test]
    fn capability_text_round_trips(c in any_capability()) {
        prop_assert_eq!(Capability::from_str(&c.to_string()).unwrap(), c);
    }

    /// The CSP never opens plain http unless the local-network grant is held.
    #[test]
    fn csp_never_opens_http_without_local(caps in proptest::collection::vec(any_capability(), 0..12)) {
        let m = manifest("{}");
        let mut rec = GrantRecord::new("mira.loom", None, Trust::Local);
        rec.granted.extend(caps);
        let e = Effective::resolve(&m, Trust::Local, None, Some(&rec));
        let csp = e.csp();
        prop_assert_eq!(csp.contains(" http:"), e.allows(&Capability::LocalNetwork));
        prop_assert_eq!(csp.contains("'unsafe-eval'"), e.allows(&Capability::ScriptEval));
    }
}
