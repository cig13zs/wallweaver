use super::*;
use serde_json::json;

fn base() -> serde_json::Value {
    json!({
        "manifestVersion": 1,
        "id": "mira.aurora-loom",
        "name": "Aurora Loom",
        "version": "1.2.0",
        "type": "web",
        "entry": "index.html",
        "author": { "name": "mira", "url": "https://example.com/mira" },
        "license": "MIT",
        "description": "Slow northern-light curtains over a quiet ridge.",
        "tags": ["space", "audio-reactive"],
        "preview": "preview.webp",
        "properties": {
            "speed": { "type": "slider", "label": "Speed", "default": 0.6, "min": 0.1, "max": 3, "step": 0.1 },
            "glow": { "type": "color", "label": "Glow", "default": "#5EFFC4" },
            "reactToMusic": { "type": "bool", "label": "React to music", "default": true },
            "foreground": { "type": "combo", "label": "Foreground", "default": "ridgeLow",
                "options": [ { "value": "ridgeLow", "label": "Ridge, low" }, { "value": "none", "label": "None" } ] }
        },
        "permissions": {
            "audio": true,
            "system.stats": ["cpu", "gpu"],
            "network": ["https://api.open-meteo.com"],
            "input.mouse": true
        }
    })
}

fn parse(v: &serde_json::Value) -> Result<Manifest, ManifestError> {
    Manifest::from_json(&v.to_string())
}

fn issues_for(v: &serde_json::Value) -> Vec<Issue> {
    match parse(v) {
        Err(ManifestError::Invalid(issues)) => issues,
        other => panic!("expected validation issues, got {other:?}"),
    }
}

fn has_issue(issues: &[Issue], path: &str) -> bool {
    issues.iter().any(|i| i.path == path)
}

#[test]
fn parses_full_manifest() {
    let m = parse(&base()).expect("valid manifest");
    assert_eq!(m.id, "mira.aurora-loom");
    assert_eq!(m.kind, WallpaperType::Web);
    assert_eq!(m.version, semver::Version::new(1, 2, 0));
    assert_eq!(
        m.permissions.system_stats,
        vec![StatKind::Cpu, StatKind::Gpu]
    );
    assert!(m.permissions.input_mouse);
    assert!(!m.permissions.weather);
    let keys: Vec<_> = m.properties.keys().cloned().collect();
    assert_eq!(
        keys,
        ["speed", "glow", "reactToMusic", "foreground"],
        "order is kept"
    );
    let defaults = m.default_properties();
    assert_eq!(defaults["speed"], json!(0.6));
    assert_eq!(defaults["foreground"], json!("ridgeLow"));
}

#[test]
fn minimal_manifest_denies_everything() {
    let m = parse(&json!({
        "manifestVersion": 1, "id": "abc", "name": "A", "version": "0.1.0",
        "type": "video", "entry": "loop.webm", "author": { "name": "x" }, "license": "CC0-1.0"
    }))
    .unwrap();
    assert!(m.permissions.is_empty());
    assert!(m.properties.is_empty());
}

#[test]
fn round_trips_through_json() {
    let m = parse(&base()).unwrap();
    let again = Manifest::from_json(&m.to_json_pretty()).unwrap();
    assert_eq!(m, again);
}

#[test]
fn accepts_bom_and_schema_link() {
    let mut v = base();
    v["$schema"] = json!(SCHEMA_ID);
    let text = format!("\u{feff}{v}");
    assert!(Manifest::from_json(&text).is_ok());
}

#[test]
fn syntax_error_reports_position() {
    let err = Manifest::from_json("{\n  \"id\": \"x\",\n  oops\n}").unwrap_err();
    match err {
        ManifestError::Syntax { line, .. } => assert_eq!(line, 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_top_level_field_is_rejected_with_path() {
    let mut v = base();
    v["autostart"] = json!(true);
    let err = parse(&v).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ManifestError::Shape { .. }), "{err:?}");
    assert!(msg.contains("autostart"), "{msg}");
}

#[test]
fn unknown_permission_is_rejected() {
    let mut v = base();
    v["permissions"]["filesystem"] = json!(true);
    let err = parse(&v).unwrap_err();
    match &err {
        ManifestError::Shape { path, message, .. } => {
            assert!(path.starts_with("permissions"), "{path}");
            assert!(message.contains("filesystem"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn wrong_type_reports_field_path() {
    let mut v = base();
    v["permissions"]["audio"] = json!("yes");
    match parse(&v).unwrap_err() {
        ManifestError::Shape { path, .. } => assert_eq!(path, "permissions.audio"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_stat_kind_is_rejected() {
    let mut v = base();
    v["permissions"]["system.stats"] = json!(["cpu", "processes"]);
    assert!(matches!(parse(&v), Err(ManifestError::Shape { .. })));
}

#[test]
fn unknown_property_type_and_field_rejected() {
    let mut v = base();
    v["properties"]["speed"]["type"] = json!("knob");
    assert!(matches!(parse(&v), Err(ManifestError::Shape { .. })));
    let mut v = base();
    v["properties"]["glow"]["alpha"] = json!(true);
    assert!(matches!(parse(&v), Err(ManifestError::Shape { .. })));
}

#[test]
fn missing_required_field() {
    let mut v = base();
    v.as_object_mut().unwrap().remove("license");
    let msg = parse(&v).unwrap_err().to_string();
    assert!(msg.contains("license"), "{msg}");
}

#[test]
fn bad_version_string() {
    let mut v = base();
    v["version"] = json!("1.2");
    assert!(matches!(parse(&v), Err(ManifestError::Shape { .. })));
}

#[test]
fn unsupported_manifest_version() {
    let mut v = base();
    v["manifestVersion"] = json!(2);
    assert!(has_issue(&issues_for(&v), "manifestVersion"));
}

#[test]
fn id_rules() {
    for bad in [
        "ab",
        "Mira.Aurora",
        "-lead",
        "trail.",
        "a..b",
        "a/b",
        "a b c",
        "x--y",
        "a.-b",
    ] {
        let mut v = base();
        v["id"] = json!(bad);
        assert!(has_issue(&issues_for(&v), "id"), "{bad}");
    }
    for good in ["abc", "mira.aurora-loom", "a1.b2.c3", "x-y-z"] {
        let mut v = base();
        v["id"] = json!(good);
        assert!(parse(&v).is_ok(), "{good}");
    }
    let mut v = base();
    v["id"] = json!("a".repeat(65));
    assert!(has_issue(&issues_for(&v), "id"));
}

#[test]
fn entry_must_be_safe_and_match_type() {
    for bad in [
        "../index.html",
        "/index.html",
        "C:/x.html",
        "a\\b.html",
        "./index.html",
    ] {
        let mut v = base();
        v["entry"] = json!(bad);
        assert!(has_issue(&issues_for(&v), "entry"), "{bad}");
    }
    let mut v = base();
    v["entry"] = json!("main.js");
    let issues = issues_for(&v);
    assert!(issues[0].message.contains(".html"), "{issues:?}");

    let mut v = base();
    v["type"] = json!("video");
    v["entry"] = json!("clip.mov");
    assert!(has_issue(&issues_for(&v), "entry"));
    v["entry"] = json!("media/clip.MP4");
    assert!(parse(&v).is_ok());

    let mut v = base();
    v["type"] = json!("shader");
    v["entry"] = json!("aurora.frag");
    assert!(parse(&v).is_ok());
}

#[test]
fn license_must_be_spdx() {
    for bad in ["", "   ", "MIT-ish", "GPL 3", "MIT OR"] {
        let mut v = base();
        v["license"] = json!(bad);
        assert!(has_issue(&issues_for(&v), "license"), "{bad:?}");
    }
    for good in [
        "MIT",
        "Apache-2.0",
        "MIT OR Apache-2.0",
        "CC-BY-4.0",
        "CC0-1.0",
        "LicenseRef-Proprietary",
    ] {
        let mut v = base();
        v["license"] = json!(good);
        assert!(parse(&v).is_ok(), "{good}");
    }
}

#[test]
fn author_url_must_be_https() {
    let mut v = base();
    v["author"]["url"] = json!("http://example.com");
    assert!(has_issue(&issues_for(&v), "author.url"));
    v["author"]["url"] = json!("javascript:alert(1)");
    assert!(has_issue(&issues_for(&v), "author.url"));
    v["author"]["name"] = json!("  ");
    assert!(has_issue(&issues_for(&v), "author.name"));
}

#[test]
fn text_limits() {
    let mut v = base();
    v["name"] = json!("n".repeat(81));
    v["description"] = json!("d".repeat(501));
    let issues = issues_for(&v);
    assert!(has_issue(&issues, "name"));
    assert!(has_issue(&issues, "description"));

    let mut v = base();
    v["name"] = json!("bad\u{7}name");
    assert!(has_issue(&issues_for(&v), "name"));
}

#[test]
fn tag_rules() {
    let mut v = base();
    v["tags"] = json!(["Space", "ok", "ok", "x".repeat(25)]);
    let issues = issues_for(&v);
    assert!(has_issue(&issues, "tags[0]"));
    assert!(has_issue(&issues, "tags[2]"));
    assert!(has_issue(&issues, "tags[3]"));
    assert!(!has_issue(&issues, "tags[1]"));

    let mut v = base();
    v["tags"] = json!((0..11).map(|i| format!("t{i}")).collect::<Vec<_>>());
    assert!(has_issue(&issues_for(&v), "tags"));
}

#[test]
fn preview_rules() {
    let mut v = base();
    v["preview"] = json!("preview.gif");
    assert!(has_issue(&issues_for(&v), "preview"));
    v["preview"] = json!("../preview.png");
    assert!(has_issue(&issues_for(&v), "preview"));
}

#[test]
fn slider_rules() {
    let mut v = base();
    v["properties"]["speed"]["default"] = json!(5);
    assert!(has_issue(&issues_for(&v), "properties.speed.default"));

    let mut v = base();
    v["properties"]["speed"]["min"] = json!(3);
    assert!(has_issue(&issues_for(&v), "properties.speed.min"));

    let mut v = base();
    v["properties"]["speed"]["step"] = json!(0);
    assert!(has_issue(&issues_for(&v), "properties.speed.step"));

    let mut v = base();
    v["properties"]["speed"]["step"] = json!(10);
    assert!(has_issue(&issues_for(&v), "properties.speed.step"));
}

#[test]
fn color_rules() {
    for bad in ["5EFFC4", "#5EFFC", "#GGGGGG", "red", "#5EFFC4F"] {
        let mut v = base();
        v["properties"]["glow"]["default"] = json!(bad);
        assert!(
            has_issue(&issues_for(&v), "properties.glow.default"),
            "{bad}"
        );
    }
    let mut v = base();
    v["properties"]["glow"]["default"] = json!("#5effc480");
    assert!(parse(&v).is_ok());
}

#[test]
fn combo_rules() {
    let mut v = base();
    v["properties"]["foreground"]["default"] = json!("missing");
    assert!(has_issue(&issues_for(&v), "properties.foreground.default"));

    let mut v = base();
    v["properties"]["foreground"]["options"] = json!([]);
    assert!(has_issue(&issues_for(&v), "properties.foreground.options"));

    let mut v = base();
    v["properties"]["foreground"]["options"] = json!([
        { "value": "ridgeLow", "label": "A" }, { "value": "ridgeLow", "label": "B" }
    ]);
    assert!(has_issue(
        &issues_for(&v),
        "properties.foreground.options[1]"
    ));
}

#[test]
fn property_key_rules() {
    let mut v = base();
    let props = v["properties"].as_object_mut().unwrap();
    props.insert(
        "1bad-key".into(),
        json!({ "type": "bool", "label": "x", "default": false }),
    );
    assert!(has_issue(&issues_for(&v), "properties.1bad-key"));
}

#[test]
fn network_rules() {
    let mut v = base();
    v["permissions"]["network"] = json!([
        "https://api.open-meteo.com",
        "http://insecure.example.com",
        "https://localhost",
        "https://*.example.com",
        "https://API.open-meteo.com/",
        "https://10.0.0.1"
    ]);
    let issues = issues_for(&v);
    assert!(!has_issue(&issues, "permissions.network[0]"));
    for i in 1..=5 {
        assert!(
            has_issue(&issues, &format!("permissions.network[{i}]")),
            "{i}: {issues:?}"
        );
    }
    assert!(
        issues
            .iter()
            .any(|i| i.path == "permissions.network[4]" && i.message.contains("twice"))
    );

    let mut v = base();
    v["permissions"]["network"] = json!(
        (0..9)
            .map(|i| format!("https://h{i}.example.com"))
            .collect::<Vec<_>>()
    );
    assert!(has_issue(&issues_for(&v), "permissions.network"));
}

#[test]
fn duplicate_stats_rejected() {
    let mut v = base();
    v["permissions"]["system.stats"] = json!(["cpu", "cpu"]);
    assert!(has_issue(&issues_for(&v), "permissions.system.stats[1]"));
}

#[test]
fn reports_every_problem_at_once() {
    let mut v = base();
    v["id"] = json!("X");
    v["entry"] = json!("../x.html");
    v["license"] = json!("nope");
    let err = parse(&v).unwrap_err();
    assert_eq!(err.issues().len(), 3, "{err}");
    let text = err.to_string();
    assert!(text.starts_with("wallpaper.json has 3 problems:"), "{text}");
}

#[test]
fn network_origins_are_normalized() {
    let mut v = base();
    v["permissions"]["network"] = json!(["https://API.Open-Meteo.com/"]);
    let m = parse(&v).unwrap();
    assert_eq!(
        m.permissions.network_origins()[0].to_string(),
        "https://api.open-meteo.com"
    );
}

#[test]
fn schema_lists_permissions_and_rejects_unknown_fields() {
    let schema = json_schema();
    assert_eq!(schema["$id"], json!(SCHEMA_ID));
    assert_eq!(schema["additionalProperties"], json!(false));
    let required = schema["required"].as_array().unwrap();
    for field in [
        "manifestVersion",
        "id",
        "name",
        "version",
        "type",
        "entry",
        "author",
        "license",
    ] {
        assert!(
            required.contains(&json!(field)),
            "{field} should be required"
        );
    }
    let text = json_schema_string();
    for key in ["system.stats", "input.mouse", "script.eval", "network"] {
        assert!(text.contains(key), "{key} missing from schema");
    }
}

/// The schema file in the repo must match what the code generates. Run
/// `WW_UPDATE_SCHEMA=1 cargo test -p ww-manifest` after changing the types.
#[test]
fn checked_in_schema_is_current() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/wallpaper.schema.json");
    let generated = json_schema_string();
    if std::env::var_os("WW_UPDATE_SCHEMA").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let Ok(on_disk) = std::fs::read_to_string(&path) else {
        // Not running from a repo checkout (for example a packaged crate).
        return;
    };
    assert_eq!(
        on_disk.replace("\r\n", "\n"),
        generated,
        "schema/wallpaper.schema.json is stale; rerun with WW_UPDATE_SCHEMA=1"
    );
}
