use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use ww_manifest::path::{check_relative_path, collision_key, extension};
use ww_manifest::{MANIFEST_FILE, Manifest};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::{CHECKSUMS_PATH, Checksums, FileEntry, Limits, META_DIR, PackageError, SecretKey};

/// Names skipped when walking a source folder.
const IGNORED_NAMES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "node_modules",
];

/// Extensions stored without compression because they are already compressed.
const STORED_EXTENSIONS: &[&str] = &[
    "mp4", "webm", "m4v", "png", "jpg", "jpeg", "webp", "avif", "gif", "woff", "woff2", "mp3",
    "ogg", "opus", "m4a", "zip", "gz", "br",
];

/// What [`pack_dir`] produced.
#[derive(Debug, Clone)]
pub struct PackReport {
    /// The validated manifest.
    pub manifest: Manifest,
    /// The file list written to `.wwp/CHECKSUMS`.
    pub checksums: Checksums,
}

/// Pack a folder into an unsigned `.wwp` archive written to `out`.
///
/// The folder must contain a valid `wallpaper.json`. Symbolic links, files in
/// `.wwp/`, secret key files and paths that break the package path rules are
/// errors; VCS folders, `node_modules` and OS clutter files are skipped.
pub fn pack_dir<W: Write + Seek>(src: &Path, out: W) -> Result<PackReport, PackageError> {
    let manifest_path = src.join(MANIFEST_FILE);
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|e| PackageError::io(format!("reading {}", manifest_path.display()), e))?;
    let manifest = Manifest::from_slice(&manifest_bytes)?;
    let limits = Limits::for_type(manifest.kind);
    if manifest_bytes.len() as u64 > limits.max_manifest_size {
        return Err(PackageError::TooLarge {
            size: manifest_bytes.len() as u64,
            limit: limits.max_manifest_size,
        });
    }

    let mut files = Vec::new();
    walk(src, "", &mut files)?;
    files.sort();

    if files.len() > limits.max_files {
        return Err(PackageError::TooManyFiles {
            count: files.len(),
            limit: limits.max_files,
        });
    }

    let mut seen: HashMap<String, String> = HashMap::new();
    let mut entries = Vec::with_capacity(files.len());
    let mut total: u64 = 0;
    for rel in &files {
        if let Some(prev) = seen.insert(collision_key(rel), rel.clone()) {
            return Err(PackageError::CaseCollision(prev, rel.clone()));
        }
        let full = src.join(rel);
        let (size, sha256) = hash_file(&full, rel)?;
        total = total.saturating_add(size);
        if total > limits.max_total_size {
            return Err(PackageError::TooLarge {
                size: total,
                limit: limits.max_total_size,
            });
        }
        entries.push(FileEntry {
            path: rel.clone(),
            size,
            sha256,
        });
    }
    let checksums = Checksums::new(entries)?;
    check_references(&manifest, &checksums)?;

    let mut zip = ZipWriter::new(out);
    let base = SimpleFileOptions::default()
        .last_modified_time(DateTime::DEFAULT)
        .unix_permissions(0o644);
    zip.start_file(
        CHECKSUMS_PATH,
        base.compression_method(CompressionMethod::Deflated),
    )
    .map_err(PackageError::zip)?;
    zip.write_all(checksums.render().as_bytes())
        .map_err(|e| PackageError::io("writing archive", e))?;

    for entry in checksums.entries() {
        let method = match extension(&entry.path) {
            Some(ext) if STORED_EXTENSIONS.contains(&ext.as_str()) => CompressionMethod::Stored,
            _ => CompressionMethod::Deflated,
        };
        zip.start_file(entry.path.as_str(), base.compression_method(method))
            .map_err(PackageError::zip)?;
        let mut f = fs::File::open(src.join(&entry.path))
            .map_err(|e| PackageError::io(format!("opening {}", entry.path), e))?;
        // Copy exactly the bytes we hashed. A file that changed in the
        // meantime is an error rather than a silently wrong package.
        let mut hashing = HashingWriter {
            inner: &mut zip,
            hasher: Sha256::new(),
        };
        let copied = std::io::copy(&mut (&mut f).take(entry.size + 1), &mut hashing)
            .map_err(|e| PackageError::io(format!("packing {}", entry.path), e))?;
        if copied != entry.size {
            return Err(PackageError::SizeMismatch(entry.path.clone()));
        }
        let digest: [u8; 32] = hashing.hasher.finalize().into();
        if digest != entry.sha256 {
            return Err(PackageError::HashMismatch(entry.path.clone()));
        }
    }
    zip.finish().map_err(PackageError::zip)?;
    Ok(PackReport {
        manifest,
        checksums,
    })
}

struct HashingWriter<'a, W: Write> {
    inner: &'a mut W,
    hasher: Sha256,
}

impl<W: Write> Write for HashingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub(crate) fn check_references(
    manifest: &Manifest,
    checksums: &Checksums,
) -> Result<(), PackageError> {
    if checksums.get(&manifest.entry).is_none() {
        return Err(PackageError::ReferencedFileMissing {
            field: "entry",
            path: manifest.entry.clone(),
        });
    }
    if let Some(preview) = &manifest.preview
        && checksums.get(preview).is_none()
    {
        return Err(PackageError::ReferencedFileMissing {
            field: "preview",
            path: preview.clone(),
        });
    }
    Ok(())
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), PackageError> {
    let read =
        fs::read_dir(dir).map_err(|e| PackageError::io(format!("listing {}", dir.display()), e))?;
    for item in read {
        let item = item.map_err(|e| PackageError::io(format!("listing {}", dir.display()), e))?;
        let name = item.file_name();
        let Some(name) = name.to_str() else {
            return Err(PackageError::NonUtf8Name);
        };
        if IGNORED_NAMES.contains(&name) {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        if rel == META_DIR {
            return Err(PackageError::ReservedPath(rel));
        }
        let meta = fs::symlink_metadata(item.path())
            .map_err(|e| PackageError::io(format!("reading {rel}"), e))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(PackageError::Symlink(rel));
        } else if ft.is_dir() {
            walk(&item.path(), &rel, out)?;
        } else if ft.is_file() {
            check_relative_path(&rel).map_err(|issue| PackageError::BadPath {
                path: rel.clone(),
                issue,
            })?;
            if extension(&rel).as_deref() == Some("wwkey") {
                return Err(PackageError::SecretKeyInSource(rel));
            }
            out.push(rel);
        } else {
            return Err(PackageError::NotAFile(rel));
        }
    }
    Ok(())
}

fn hash_file(path: &Path, rel: &str) -> Result<(u64, [u8; 32]), PackageError> {
    let mut f = fs::File::open(path).map_err(|e| PackageError::io(format!("opening {rel}"), e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    let mut first = true;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| PackageError::io(format!("reading {rel}"), e))?;
        if n == 0 {
            break;
        }
        if first && SecretKey::looks_like_key_file(&buf[..n]) {
            return Err(PackageError::SecretKeyInSource(rel.to_string()));
        }
        first = false;
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hasher.finalize().into()))
}
