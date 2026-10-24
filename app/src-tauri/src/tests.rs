use serde_json::json;

use crate::daemon::{Daemon, DaemonError, StubDaemon};
use crate::dto::{
    ApplyRequest, Fixtures, GalleryQuery, GallerySort, LinkId, PermissionKey, PresetId,
    PresetRequest, SpanMode,
};
use crate::links::url_for;

fn stub() -> StubDaemon {
    StubDaemon::bundled().expect("fixtures parse")
}

#[test]
fn shared_fixtures_match_the_rust_types() {
    let fx: Fixtures =
        serde_json::from_str(crate::daemon::FIXTURES_JSON).expect("fixture file matches dto.rs");
    assert_eq!(fx.gallery.len(), 16);
    // Round trip: what Rust sends back is what the UI expects to read.
    let back = serde_json::to_value(&fx).expect("serialize");
    let original: serde_json::Value =
        serde_json::from_str(crate::daemon::FIXTURES_JSON).expect("json");
    assert_eq!(numbers_as_f64(back), numbers_as_f64(original));
}

/// JSON has one number type; compare 2 and 2.0 as equal.
fn numbers_as_f64(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Number(n) => n.as_f64().map_or(Value::Null, |f| json!(f)),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers_as_f64).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, numbers_as_f64(v))).collect())
        }
        other => other,
    }
}

#[test]
fn requests_from_the_ui_deserialize() {
    let req: ApplyRequest = serde_json::from_value(json!({
        "wallpaperId": "aurora-loom", "displays": [2, 3], "mode": "each",
        "values": { "speed": 0.6, "glow": "#5EFFC4", "dimAfterSunset": true }, "paletteId": "borealis"
    }))
    .expect("apply request");
    assert_eq!(req.mode, SpanMode::Each);
    assert_eq!(req.values.len(), 3);

    let q: GalleryQuery = serde_json::from_value(
        json!({ "category": "space", "lowImpactOnly": true, "sort": "likes" }),
    )
    .expect("query");
    assert_eq!(q.sort, Some(GallerySort::Likes));

    let keys: Vec<PermissionKey> =
        serde_json::from_value(json!(["system.stats", "input.mouse"])).expect("keys");
    assert_eq!(
        keys,
        vec![PermissionKey::SystemStats, PermissionKey::InputMouse]
    );
}

#[test]
fn unknown_fields_are_rejected() {
    let r = serde_json::from_value::<ApplyRequest>(json!({
        "wallpaperId": "x", "displays": [], "mode": "same", "values": {}, "paletteId": "", "runAs": "admin"
    }));
    assert!(r.is_err());
}

#[test]
fn gallery_filters_like_the_browser_mock() {
    let d = stub();
    let space: Vec<String> = d
        .gallery(&GalleryQuery {
            category: Some("space".into()),
            ..Default::default()
        })
        .expect("gallery")
        .into_iter()
        .map(|w| w.id)
        .collect();
    assert_eq!(space, ["constellation", "deep-field", "saturn-hours"]);
    let petal = d
        .gallery(&GalleryQuery {
            text: Some("  PETAL ".into()),
            ..Default::default()
        })
        .expect("gallery");
    assert_eq!(petal.len(), 1);
    let light = d
        .gallery(&GalleryQuery {
            low_impact_only: Some(true),
            ..Default::default()
        })
        .expect("gallery");
    assert!(light.iter().all(|w| w.cost == crate::dto::Cost::Low));
    let lightest = d
        .gallery(&GalleryQuery {
            sort: Some(GallerySort::Lightest),
            ..Default::default()
        })
        .expect("gallery");
    assert_eq!(
        lightest.last().map(|w| w.cost),
        Some(crate::dto::Cost::High)
    );
}

#[test]
fn install_grants_only_what_was_allowed() {
    let d = stub();
    let res = d
        .install(
            "weather-glass",
            &[
                PermissionKey::Audio,
                PermissionKey::Network,
                PermissionKey::InputMouse,
                PermissionKey::Calendar,
            ],
        )
        .expect("install");
    assert_eq!(
        res.granted,
        vec![
            PermissionKey::Audio,
            PermissionKey::Network,
            PermissionKey::InputMouse
        ]
    );
    assert_eq!(res.denied, vec![PermissionKey::SystemStats]);
    let derived = d.install_request("constellation").expect("request");
    assert_eq!(
        derived
            .permissions
            .iter()
            .map(|p| p.key)
            .collect::<Vec<_>>(),
        vec![PermissionKey::InputMouse]
    );
    assert!(matches!(
        d.install_request("nope"),
        Err(DaemonError::UnknownWallpaper(_))
    ));
}

#[test]
fn apply_checks_displays_and_updates_now_playing() {
    let d = stub();
    let req = |displays: Vec<u32>| ApplyRequest {
        wallpaper_id: "contour".into(),
        displays,
        mode: SpanMode::Same,
        values: Default::default(),
        palette_id: String::new(),
    };
    assert_eq!(d.apply(&req(vec![9])), Err(DaemonError::UnknownDisplay(9)));
    assert_eq!(
        d.apply(&req(vec![1, 3])).expect("apply").applied,
        vec![1, 3]
    );
    assert_eq!(
        d.status().expect("status").now_playing.map(|n| n.title),
        Some("Contour".into())
    );
}

#[test]
fn power_settings_are_validated() {
    let d = stub();
    assert!(d.set_pause_rule("battery", "Pause").is_ok());
    assert!(matches!(
        d.set_pause_rule("battery", "Explode"),
        Err(DaemonError::UnsupportedAction { .. })
    ));
    assert!(matches!(
        d.set_pause_rule("nope", "Pause"),
        Err(DaemonError::UnknownRule(_))
    ));
    assert_eq!(
        d.set_preset(PresetId::Saver)
            .expect("preset")
            .settings
            .fps_cap,
        30
    );
    let mut s = d.performance().expect("perf").settings;
    s.hw_decode_available = false;
    s.fps_cap = 120;
    let after = d.set_perf_settings(&s).expect("settings").settings;
    assert!(
        after.hw_decode_available,
        "availability is reported by the daemon, not set by the UI"
    );
    assert_eq!(after.fps_cap, 120);
    assert!(matches!(
        d.set_app_rule("  ", "Stop"),
        Err(DaemonError::Empty(_))
    ));
}

#[test]
fn pause_all_round_trips() {
    let d = stub();
    assert_eq!(d.pause_all(true).expect("pause").displays_live, 0);
    assert!(
        d.performance()
            .expect("perf")
            .displays
            .iter()
            .all(|x| x.fps == 0)
    );
    assert_eq!(d.pause_all(false).expect("resume").displays_live, 2);
    assert!(
        d.performance()
            .expect("perf")
            .displays
            .iter()
            .any(|x| x.fps > 0)
    );
}

#[test]
fn presets_and_creator_flow() {
    let d = stub();
    let p = PresetRequest {
        wallpaper_id: "aurora-loom".into(),
        name: "Mine".into(),
        values: Default::default(),
        palette_id: "ember".into(),
    };
    assert_eq!(d.save_preset(&p).expect("save"), 4);
    assert_eq!(d.save_preset(&p).expect("save again"), 4);
    let origin = d.creator_project().expect("project").issues[0]
        .origin
        .clone();
    let after = d.declare_origin(&origin).expect("declare");
    assert!(after.issues.is_empty());
    assert!(after.declared.contains(&"network".to_owned()));
    assert_eq!(
        d.pack_and_sign().expect("pack").file,
        "aurora-loom-1.2.0.wwp"
    );
}

#[test]
fn links_stay_on_the_project_site() {
    assert_eq!(
        url_for(LinkId::Source, None),
        "https://github.com/cig13zs/wallweaver"
    );
    for id in [LinkId::Funding, LinkId::Sponsor, LinkId::Tip, LinkId::Store] {
        assert!(
            url_for(id, Some("https://evil.example"))
                .starts_with("https://github.com/cig13zs/wallweaver#")
        );
    }
    let report = url_for(LinkId::Report, Some("a/../b?x=1&y=<script>"));
    assert!(report.starts_with("https://github.com/cig13zs/wallweaver/issues/new?"));
    assert!(!report.contains('<') && !report.contains("&y=") && !report.contains("/../"));
}
