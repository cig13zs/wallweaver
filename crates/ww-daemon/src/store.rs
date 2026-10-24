//! Files the daemon keeps: settings, permission grants, installed packages.

use std::path::{Path, PathBuf};

use ww_governor::Policy;
use ww_ipc::WallpaperSource;
use ww_manifest::{MANIFEST_FILE, Manifest};
use ww_permissions::{GrantRecord, Trust};

/// The per-user data folder.
pub fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Wallweaver")
    }
    #[cfg(target_vendor = "apple")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Application Support/Wallweaver")
    }
    #[cfg(not(any(windows, target_vendor = "apple")))]
    {
        match std::env::var_os("XDG_DATA_HOME") {
            Some(d) if !d.is_empty() => PathBuf::from(d).join("wallweaver"),
            _ => std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".local/share/wallweaver"),
        }
    }
}

/// A wallpaper source resolved to files on disk.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Package id (synthesized for single files).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Manifest type.
    pub kind: ww_manifest::WallpaperType,
    /// The manifest, for packages and folders.
    pub manifest: Option<Manifest>,
    /// Package root, for packages and folders.
    pub root: Option<PathBuf>,
    /// The file, for single files.
    pub file: Option<PathBuf>,
    /// Where it came from.
    pub trust: Trust,
    /// Signer key id, for signed packages.
    pub signer: Option<String>,
    /// The original request.
    pub source: WallpaperSource,
}

/// Turn a source into files, validating the manifest.
pub fn resolve(data_dir: &Path, source: &WallpaperSource) -> Result<Resolved, String> {
    match source {
        WallpaperSource::Installed { id } => {
            if ww_manifest::path::check_relative_path(id).is_err() || id.contains('/') {
                return Err(format!("'{id}' is not a valid package id"));
            }
            let root = data_dir.join("packages").join(id);
            let manifest = read_manifest(&root)?;
            // The installer writes the signer's key id next to the package.
            let signer =
                std::fs::read_to_string(data_dir.join("packages").join(format!("{id}.signer")))
                    .ok()
                    .map(|s| s.trim().to_string());
            Ok(Resolved {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                kind: manifest.kind,
                manifest: Some(manifest),
                root: Some(root),
                file: None,
                trust: Trust::Gallery,
                signer,
                source: source.clone(),
            })
        }
        WallpaperSource::Folder { path } => {
            let root = PathBuf::from(path);
            let manifest = read_manifest(&root)?;
            Ok(Resolved {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                kind: manifest.kind,
                manifest: Some(manifest),
                root: Some(root),
                file: None,
                trust: Trust::Local,
                signer: None,
                source: source.clone(),
            })
        }
        WallpaperSource::File { path } => {
            let file = PathBuf::from(path);
            if !file.is_file() {
                return Err(format!("{} is not a file", file.display()));
            }
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "wallpaper".into());
            let ext = file
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            // The synthesized manifest only needs an ASCII-safe name.
            let m = Manifest::for_local_file(&format!("{}.{ext}", ascii_stem(&name)))
                .ok_or_else(|| format!("Wallweaver can't show .{ext} files yet"))?;
            Ok(Resolved {
                id: m.id.clone(),
                name: file
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(m.name.clone()),
                kind: m.kind,
                manifest: None,
                root: None,
                file: Some(file),
                trust: Trust::Local,
                signer: None,
                source: source.clone(),
            })
        }
    }
}

fn ascii_stem(name: &str) -> String {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if s.trim_matches('-').is_empty() {
        "wallpaper".into()
    } else {
        s
    }
}

fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let path = root.join(MANIFEST_FILE);
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Manifest::from_json(&text).map_err(|e| e.to_string())
}

/// Saved settings and grants.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    /// Governor policy.
    pub policy: Policy,
    /// Permission grants.
    pub grants: Vec<GrantRecord>,
}

impl Store {
    /// Load from `dir`, falling back to defaults for missing or broken files.
    pub fn load(dir: &Path) -> Store {
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        let policy = read("policy.json")
            .and_then(|t| serde_json::from_str::<Policy>(&t).ok())
            .unwrap_or_default();
        let grants = read("grants.json")
            .and_then(|t| serde_json::from_str::<Vec<GrantRecord>>(&t).ok())
            .unwrap_or_default();
        Store {
            dir: dir.to_path_buf(),
            policy,
            grants,
        }
    }

    /// Write both files atomically (write to a temp file, then rename).
    pub fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let write = |name: &str, text: String| -> Result<(), String> {
            let tmp = self.dir.join(format!(".{name}.tmp"));
            std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, self.dir.join(name)).map_err(|e| e.to_string())
        };
        write(
            "policy.json",
            serde_json::to_string_pretty(&self.policy).map_err(|e| e.to_string())?,
        )?;
        write(
            "grants.json",
            serde_json::to_string_pretty(&self.grants).map_err(|e| e.to_string())?,
        )
    }

    /// The grant record for a wallpaper, if any.
    pub fn record(&self, r: &Resolved) -> Option<&GrantRecord> {
        self.grants
            .iter()
            .find(|g| g.package_id == r.id && g.signer == r.signer && g.trust == r.trust)
    }

    /// The grant record for a wallpaper, created if missing.
    pub fn record_mut(&mut self, r: &Resolved) -> &mut GrantRecord {
        let pos = self
            .grants
            .iter()
            .position(|g| g.package_id == r.id && g.signer == r.signer && g.trust == r.trust);
        match pos {
            Some(i) => &mut self.grants[i],
            None => {
                self.grants
                    .push(GrantRecord::new(r.id.clone(), r.signer.clone(), r.trust));
                let last = self.grants.len() - 1;
                &mut self.grants[last]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_files_resolve_with_local_trust() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("Café night.PNG");
        std::fs::write(&f, b"png").unwrap();
        let r = resolve(
            dir.path(),
            &WallpaperSource::File {
                path: f.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(r.trust, Trust::Local);
        assert_eq!(r.kind, ww_manifest::WallpaperType::Image);
        assert_eq!(r.name, "Café night");
        assert!(r.id.starts_with("local."));

        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(
            resolve(
                dir.path(),
                &WallpaperSource::File {
                    path: txt.to_string_lossy().into_owned()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn installed_ids_cannot_escape_the_package_folder() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["../x", "a/b", "/etc", ""] {
            assert!(
                resolve(dir.path(), &WallpaperSource::Installed { id: id.into() }).is_err(),
                "{id}"
            );
        }
    }

    #[test]
    fn store_round_trips_and_survives_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::load(dir.path());
        s.policy.max_fps = 30;
        s.save().unwrap();
        assert_eq!(Store::load(dir.path()).policy.max_fps, 30);
        std::fs::write(dir.path().join("policy.json"), "{not json").unwrap();
        assert_eq!(Store::load(dir.path()).policy, Policy::default());
    }
}
