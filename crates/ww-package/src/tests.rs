//! Package tests, including the hostile archives the reader must refuse.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::*;

const MANIFEST: &str = r#"{
  "manifestVersion": 1,
  "id": "test.sample",
  "name": "Sample",
  "version": "1.0.0",
  "type": "web",
  "entry": "index.html",
  "author": { "name": "tester" },
  "license": "MIT"
}"#;

fn key(n: u8) -> SecretKey {
    SecretKey::from_seed(&[n; 32])
}

fn write_tree(root: &Path, files: &[(&str, &[u8])]) {
    for (path, data) in files {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
}

fn sample_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_tree(
        dir.path(),
        &[
            ("wallpaper.json", MANIFEST.as_bytes()),
            (
                "index.html",
                b"<!doctype html><script src=main.js></script>",
            ),
            ("main.js", b"console.log('hi')"),
            ("assets/bg.png", &[0x89, b'P', b'N', b'G', 1, 2, 3]),
        ],
    );
    dir
}

fn pack_bytes(dir: &Path) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    pack_dir(dir, &mut out).unwrap();
    out.into_inner()
}

fn signed_sample() -> Vec<u8> {
    let unsigned = pack_bytes(sample_dir().path());
    let mut out = Cursor::new(Vec::new());
    sign_package(Cursor::new(unsigned), &mut out, &key(1)).unwrap();
    out.into_inner()
}

fn verify_dev(bytes: &[u8]) -> Result<VerifiedPackage, PackageError> {
    verify(Cursor::new(bytes), &VerifyOptions::developer())
}

fn sha(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Build an archive by hand: CHECKSUMS describes `listed`, the zip holds `actual`.
fn forged(listed: &[(&str, u64, &[u8])], actual: &[(&str, &[u8])]) -> Vec<u8> {
    let checksums = Checksums::new(
        listed
            .iter()
            .map(|(p, size, data)| FileEntry {
                path: p.to_string(),
                size: *size,
                sha256: sha(data),
            })
            .collect(),
    )
    .unwrap();
    let mut entries: Vec<(&str, Vec<u8>)> = vec![(CHECKSUMS_PATH, checksums.render().into_bytes())];
    entries.extend(actual.iter().map(|(p, d)| (*p, d.to_vec())));
    raw_zip(&entries)
}

fn raw_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn honest(files: &[(&str, &[u8])]) -> Vec<u8> {
    let listed: Vec<_> = files
        .iter()
        .map(|(p, d)| (*p, d.len() as u64, *d))
        .collect();
    forged(&listed, files)
}

/// Patch a field of the central directory record for `name`.
fn patch_central(bytes: &mut [u8], name: &str, patch: impl Fn(&mut [u8])) {
    let sig = [0x50, 0x4b, 0x01, 0x02];
    let mut i = 0;
    while i + 46 <= bytes.len() {
        if bytes[i..i + 4] == sig {
            let name_len = u16::from_le_bytes([bytes[i + 28], bytes[i + 29]]) as usize;
            if &bytes[i + 46..i + 46 + name_len] == name.as_bytes() {
                patch(&mut bytes[i..i + 46]);
                return;
            }
        }
        i += 1;
    }
    panic!("central record for {name} not found");
}

fn replace_all(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
}

// ---------- happy paths ----------

#[test]
fn pack_then_verify_unsigned_in_developer_mode() {
    let bytes = pack_bytes(sample_dir().path());
    let pkg = verify_dev(&bytes).unwrap();
    assert_eq!(pkg.manifest.id, "test.sample");
    assert!(pkg.signer.is_none());
    let paths: Vec<_> = pkg
        .checksums
        .entries()
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["assets/bg.png", "index.html", "main.js", "wallpaper.json"]
    );
}

#[test]
fn unsigned_rejected_outside_developer_mode() {
    let bytes = pack_bytes(sample_dir().path());
    assert!(matches!(
        verify(Cursor::new(bytes), &VerifyOptions::signed()),
        Err(PackageError::Unsigned)
    ));
}

#[test]
fn signed_package_verifies_and_reports_signer() {
    let bytes = signed_sample();
    let pkg = verify(Cursor::new(&bytes), &VerifyOptions::signed()).unwrap();
    let signer = pkg.signer.unwrap();
    assert_eq!(signer.key_id, key(1).public_key().key_id());

    let trusted = VerifyOptions {
        allow_unsigned: false,
        trusted_keys: vec![key(1).public_key()],
    };
    assert!(verify(Cursor::new(&bytes), &trusted).is_ok());
}

#[test]
fn untrusted_signer_rejected() {
    let opts = VerifyOptions {
        allow_unsigned: false,
        trusted_keys: vec![key(2).public_key()],
    };
    assert!(matches!(
        verify(Cursor::new(signed_sample()), &opts),
        Err(PackageError::UntrustedSigner(_))
    ));
}

#[test]
fn packing_is_reproducible() {
    let dir = sample_dir();
    assert_eq!(pack_bytes(dir.path()), pack_bytes(dir.path()));
}

#[test]
fn resigning_replaces_the_signature() {
    let mut out = Cursor::new(Vec::new());
    let pkg = sign_package(Cursor::new(signed_sample()), &mut out, &key(3)).unwrap();
    assert_eq!(pkg.signer.unwrap().key_id, key(3).public_key().key_id());
    let again = verify(Cursor::new(out.into_inner()), &VerifyOptions::signed()).unwrap();
    assert_eq!(again.signer.unwrap().key_id, key(3).public_key().key_id());
}

#[test]
fn unpack_writes_verified_files() {
    let target = tempfile::tempdir().unwrap();
    let dest = target.path().join("test.sample");
    unpack(
        Cursor::new(signed_sample()),
        &dest,
        &VerifyOptions::signed(),
    )
    .unwrap();
    assert_eq!(
        fs::read(dest.join("main.js")).unwrap(),
        b"console.log('hi')"
    );
    assert!(dest.join("assets/bg.png").is_file());
    assert!(!dest.join(".wwp").exists(), "metadata is not unpacked");
    let leftovers: Vec<_> = fs::read_dir(target.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no staging folder left behind");
}

#[test]
fn unpack_refuses_existing_destination() {
    let target = tempfile::tempdir().unwrap();
    assert!(matches!(
        unpack(
            Cursor::new(signed_sample()),
            target.path(),
            &VerifyOptions::signed()
        ),
        Err(PackageError::DestinationExists(_))
    ));
}

#[test]
fn failed_unpack_leaves_nothing_behind() {
    // Last file (wallpaper.json sorts after index.html) is fine, but index.html lies.
    let bytes = forged(
        &[
            ("index.html", 5, b"hello"),
            ("wallpaper.json", MANIFEST.len() as u64, MANIFEST.as_bytes()),
        ],
        &[
            ("index.html", b"HELLO"),
            ("wallpaper.json", MANIFEST.as_bytes()),
        ],
    );
    let target = tempfile::tempdir().unwrap();
    let dest = target.path().join("pkg");
    assert!(matches!(
        unpack(Cursor::new(bytes), &dest, &VerifyOptions::developer()),
        Err(PackageError::HashMismatch(p)) if p == "index.html"
    ));
    assert!(!dest.exists());
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}

// ---------- tampering ----------

#[test]
fn modified_content_after_signing_is_caught() {
    let mut bytes = signed_sample();
    // main.js is deflated; flip a byte of the stored PNG instead, which is
    // stored uncompressed and easy to find.
    replace_all(
        &mut bytes,
        &[0x89, b'P', b'N', b'G', 1, 2, 3],
        &[0x89, b'P', b'N', b'G', 9, 9, 9],
    );
    let err = verify(Cursor::new(bytes), &VerifyOptions::signed()).unwrap_err();
    assert!(
        matches!(err, PackageError::HashMismatch(ref p) if p == "assets/bg.png"),
        "{err}"
    );
}

#[test]
fn modified_checksums_break_the_signature() {
    let signed = signed_sample();
    let mut archive = zip::ZipArchive::new(Cursor::new(&signed)).unwrap();
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).unwrap();
        entries.push((f.name().unwrap().to_string(), data));
    }
    // Swap main.js for something else and update CHECKSUMS to match, but keep
    // the old signature.
    let evil = b"fetch('https://evil.example')".to_vec();
    for (name, data) in &mut entries {
        if name == "main.js" {
            *data = evil.clone();
        }
    }
    let listed: Vec<FileEntry> = entries
        .iter()
        .filter(|(n, _)| !n.starts_with(".wwp/"))
        .map(|(n, d)| FileEntry {
            path: n.clone(),
            size: d.len() as u64,
            sha256: sha(d),
        })
        .collect();
    let new_checksums = Checksums::new(listed).unwrap().render().into_bytes();
    for (name, data) in &mut entries {
        if name == CHECKSUMS_PATH {
            *data = new_checksums.clone();
        }
    }
    let named: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.clone()))
        .collect();
    let bytes = raw_zip(&named);
    assert!(matches!(
        verify(Cursor::new(&bytes), &VerifyOptions::signed()),
        Err(PackageError::BadSignature)
    ));
    // A bad signature is never downgraded to "unsigned", even in developer mode.
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::BadSignature)
    ));
}

#[test]
fn garbage_signature_is_rejected() {
    let mut bytes = honest(&[
        ("index.html", b"x"),
        ("wallpaper.json", MANIFEST.as_bytes()),
    ]);
    // Rebuild with a junk SIGNATURE added.
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut d = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut d).unwrap();
        entries.push((f.name().unwrap().to_string(), d));
    }
    entries.push((SIGNATURE_PATH.to_string(), b"not a signature".to_vec()));
    let named: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.clone()))
        .collect();
    bytes = raw_zip(&named);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::MalformedSignature(_))
    ));
}

// ---------- hostile archives ----------

#[test]
fn path_traversal_entry_is_rejected() {
    for evil in [
        "../evil.js",
        "/etc/cron.d/evil",
        "a/../../evil",
        "C:/evil.js",
        "a\\..\\evil",
    ] {
        let bytes = raw_zip(&[(evil, b"x".to_vec())]);
        let err = verify_dev(&bytes).unwrap_err();
        assert!(matches!(err, PackageError::BadPath { .. }), "{evil}: {err}");
    }
}

#[test]
fn symlink_entry_is_rejected() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("link", "/etc/passwd", SimpleFileOptions::default())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(matches!(verify_dev(&bytes), Err(PackageError::Symlink(_))));
}

#[test]
fn directory_entry_is_rejected() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_directory("assets/", SimpleFileOptions::default())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let err = verify_dev(&bytes).unwrap_err();
    assert!(
        matches!(
            err,
            PackageError::NotAFile(_) | PackageError::BadPath { .. }
        ),
        "{err}"
    );
}

#[test]
fn duplicate_and_case_colliding_names_are_rejected() {
    let mut bytes = raw_zip(&[("aaaa.js", b"1".to_vec()), ("bbbb.js", b"2".to_vec())]);
    replace_all(&mut bytes, b"bbbb.js", b"aaaa.js");
    // Either our own check or the zip parser refuses it.
    let err = verify_dev(&bytes).unwrap_err();
    assert!(
        matches!(err, PackageError::Duplicate(_) | PackageError::Archive(_)),
        "{err}"
    );

    let bytes = raw_zip(&[("a.js", b"1".to_vec()), ("A.js", b"2".to_vec())]);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::CaseCollision(..))
    ));
}

#[test]
fn encrypted_and_exotic_compression_are_rejected() {
    let mut bytes = honest(&[
        ("index.html", b"x"),
        ("wallpaper.json", MANIFEST.as_bytes()),
    ]);
    patch_central(&mut bytes, "index.html", |h| h[8] |= 1);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::Encrypted(_))
    ));

    let mut bytes = honest(&[
        ("index.html", b"x"),
        ("wallpaper.json", MANIFEST.as_bytes()),
    ]);
    patch_central(&mut bytes, "index.html", |h| h[10] = 12); // bzip2
    let err = verify_dev(&bytes).unwrap_err();
    assert!(
        matches!(
            err,
            PackageError::UnsupportedCompression(_) | PackageError::Archive(_)
        ),
        "{err}"
    );
}

#[test]
fn non_utf8_name_is_rejected() {
    let mut bytes = raw_zip(&[("abc.js", b"1".to_vec())]);
    replace_all(&mut bytes, b"abc.js", b"ab\xffc.j");
    let err = verify_dev(&bytes).unwrap_err();
    assert!(
        matches!(err, PackageError::NonUtf8Name | PackageError::Archive(_)),
        "{err}"
    );
}

#[test]
fn reserved_metadata_names_are_rejected() {
    let bytes = raw_zip(&[(".wwp/evil", b"1".to_vec())]);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::ReservedPath(_))
    ));
}

#[test]
fn archive_without_checksums_is_not_a_package() {
    let bytes = raw_zip(&[("wallpaper.json", MANIFEST.as_bytes().to_vec())]);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::MissingChecksums)
    ));
}

#[test]
fn unlisted_and_missing_files_are_rejected() {
    let bytes = forged(
        &[
            ("index.html", 1, b"x"),
            ("wallpaper.json", MANIFEST.len() as u64, MANIFEST.as_bytes()),
        ],
        &[
            ("index.html", b"x"),
            ("wallpaper.json", MANIFEST.as_bytes()),
            ("sneaky.js", b"steal()"),
        ],
    );
    assert!(matches!(verify_dev(&bytes), Err(PackageError::UnlistedFile(p)) if p == "sneaky.js"));

    let bytes = forged(
        &[
            ("index.html", 1, b"x"),
            ("main.js", 1, b"y"),
            ("wallpaper.json", MANIFEST.len() as u64, MANIFEST.as_bytes()),
        ],
        &[
            ("index.html", b"x"),
            ("wallpaper.json", MANIFEST.as_bytes()),
        ],
    );
    assert!(matches!(verify_dev(&bytes), Err(PackageError::MissingFile(p)) if p == "main.js"));
}

#[test]
fn zip_bomb_is_cut_off_at_declared_size() {
    // CHECKSUMS claims 16 bytes; the entry inflates to 8 MiB of zeros.
    let bomb = vec![0u8; 8 * 1024 * 1024];
    let bytes = forged(
        &[
            ("index.html", 16, &bomb[..16]),
            ("wallpaper.json", MANIFEST.len() as u64, MANIFEST.as_bytes()),
        ],
        &[
            ("index.html", &bomb),
            ("wallpaper.json", MANIFEST.as_bytes()),
        ],
    );
    assert!(bytes.len() < 64 * 1024, "the bomb compresses well");
    assert!(matches!(verify_dev(&bytes), Err(PackageError::SizeMismatch(p)) if p == "index.html"));
}

#[test]
fn declared_oversize_is_rejected_before_reading() {
    let bytes = forged(
        &[
            ("index.html", 60 * 1024 * 1024, b"x"),
            ("wallpaper.json", MANIFEST.len() as u64, MANIFEST.as_bytes()),
        ],
        &[
            ("index.html", b"x"),
            ("wallpaper.json", MANIFEST.as_bytes()),
        ],
    );
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::TooLarge { .. })
    ));
}

#[test]
fn too_many_entries_rejected_from_end_record() {
    let entries: Vec<(String, Vec<u8>)> =
        (0..2_003).map(|i| (format!("f{i}"), Vec::new())).collect();
    let named: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, d)| (n.as_str(), d.clone()))
        .collect();
    let bytes = raw_zip(&named);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::TooManyFiles { .. })
    ));
}

#[test]
fn not_a_zip_and_truncated_files_are_rejected() {
    assert!(matches!(verify_dev(b""), Err(PackageError::Archive(_))));
    assert!(matches!(
        verify_dev(b"hello world, definitely not a zip file"),
        Err(PackageError::Archive(_))
    ));
    let bytes = signed_sample();
    let cut = &bytes[..bytes.len() / 2];
    assert!(verify_dev(cut).is_err());
}

#[test]
fn invalid_manifest_inside_package_is_rejected() {
    let bad = MANIFEST.replace("\"MIT\"", "\"not-a-license\"");
    let bytes = honest(&[("index.html", b"x"), ("wallpaper.json", bad.as_bytes())]);
    assert!(matches!(verify_dev(&bytes), Err(PackageError::Manifest(_))));
}

#[test]
fn entry_file_must_exist() {
    let bytes = honest(&[("wallpaper.json", MANIFEST.as_bytes())]);
    assert!(matches!(
        verify_dev(&bytes),
        Err(PackageError::ReferencedFileMissing { field: "entry", .. })
    ));
}

// ---------- packing a folder ----------

#[test]
fn pack_skips_clutter_folders() {
    let dir = sample_dir();
    write_tree(
        dir.path(),
        &[
            (".git/config", b"x"),
            ("node_modules/x/index.js", b"x"),
            (".DS_Store", b"x"),
        ],
    );
    let pkg = verify_dev(&pack_bytes(dir.path())).unwrap();
    assert_eq!(pkg.checksums.entries().len(), 4);
}

#[test]
fn pack_refuses_secret_keys() {
    let dir = sample_dir();
    write_tree(dir.path(), &[("mira.wwkey", b"whatever")]);
    let err = pack_dir(dir.path(), Cursor::new(Vec::new())).unwrap_err();
    assert!(matches!(err, PackageError::SecretKeyInSource(_)), "{err}");

    let dir = sample_dir();
    write_tree(
        dir.path(),
        &[("notes.txt", key(1).to_file_string().as_bytes())],
    );
    let err = pack_dir(dir.path(), Cursor::new(Vec::new())).unwrap_err();
    assert!(matches!(err, PackageError::SecretKeyInSource(_)), "{err}");
}

#[test]
fn pack_refuses_reserved_folder_and_bad_names() {
    let dir = sample_dir();
    write_tree(dir.path(), &[(".wwp/x", b"x")]);
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::ReservedPath(_))
    ));

    let dir = sample_dir();
    write_tree(dir.path(), &[("bad:name.txt", b"x")]);
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::BadPath { .. })
    ));
}

#[cfg(unix)]
#[test]
fn pack_refuses_symlinks() {
    let dir = sample_dir();
    std::os::unix::fs::symlink("/etc/passwd", dir.path().join("passwd")).unwrap();
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::Symlink(_))
    ));
}

#[test]
fn pack_requires_valid_manifest_and_entry() {
    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path(), &[("index.html", b"x")]);
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::Io { .. })
    ));

    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path(), &[("wallpaper.json", MANIFEST.as_bytes())]);
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::ReferencedFileMissing { .. })
    ));
}

#[test]
fn pack_enforces_file_count() {
    let dir = sample_dir();
    for i in 0..Limits::for_type(ww_manifest::WallpaperType::Web).max_files {
        fs::write(dir.path().join(format!("f{i}.txt")), b"").unwrap();
    }
    assert!(matches!(
        pack_dir(dir.path(), Cursor::new(Vec::new())),
        Err(PackageError::TooManyFiles { .. })
    ));
}

#[test]
fn slideshow_images_must_all_be_packed() {
    let manifest = r#"{
      "manifestVersion": 1, "id": "test.show", "name": "Show", "version": "1.0.0",
      "type": "slideshow", "entry": "a.jpg", "author": { "name": "t" }, "license": "CC0-1.0",
      "slideshow": { "images": ["a.jpg", "b.jpg"], "interval": 30 }
    }"#;
    let dir = tempfile::tempdir().unwrap();
    write_tree(
        dir.path(),
        &[("wallpaper.json", manifest.as_bytes()), ("a.jpg", b"jpg")],
    );
    let err = pack_dir(dir.path(), Cursor::new(Vec::new())).unwrap_err();
    assert!(
        matches!(
            &err,
            PackageError::ReferencedFileMissing { field: "slideshow.images", path } if path == "b.jpg"
        ),
        "{err:?}"
    );
    write_tree(dir.path(), &[("b.jpg", b"jpg2")]);
    let pkg = verify_dev(&pack_bytes(dir.path())).unwrap();
    assert_eq!(pkg.manifest.slideshow.unwrap().images.len(), 2);
    assert_eq!(
        Limits::for_type(WallpaperType::Slideshow).max_total_size,
        200 * MIB
    );
}
