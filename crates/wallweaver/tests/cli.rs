#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn ww(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_wallweaver"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn keygen_pack_sign_verify_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plasma-shader");
    let example = example.to_str().unwrap();

    let (ok, out) = ww(d, &["validate", example]);
    assert!(ok, "{out}");
    assert!(out.contains("permissions: input.mouse"), "{out}");

    assert!(ww(d, &["keygen", "-o", "me"]).0);
    assert!(!ww(d, &["keygen", "-o", "me"]).0, "never overwrites a key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(d.join("me.wwkey"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    assert!(ww(d, &["pack", example, "-o", "p.wwp"]).0);
    let (ok, out) = ww(d, &["verify", "p.wwp"]);
    assert!(!ok && out.contains("not signed"), "{out}");
    assert!(ww(d, &["verify", "p.wwp", "--allow-unsigned"]).0);

    assert!(ww(d, &["sign", "p.wwp", "--key", "me.wwkey", "-o", "s.wwp"]).0);
    let (ok, out) = ww(d, &["verify", "s.wwp", "--trust", "me.wwpub"]);
    assert!(ok && out.contains("signed by"), "{out}");

    assert!(ww(d, &["keygen", "-o", "other"]).0);
    let (ok, out) = ww(d, &["verify", "s.wwp", "--trust", "other.wwpub"]);
    assert!(!ok && out.contains("not one of the trusted keys"), "{out}");
}

#[test]
fn schema_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out) = ww(dir.path(), &["schema"]);
    assert!(ok && out.contains("\"manifestVersion\""));
    let (ok, out) = ww(dir.path(), &["frobnicate"]);
    assert!(!ok && out.contains("unknown command"));
    let (ok, out) = ww(dir.path(), &["validate", "missing-folder"]);
    assert!(!ok && out.contains("reading"), "{out}");
}
