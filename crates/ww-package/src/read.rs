use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ww_manifest::path::{check_relative_path, collision_key};
use ww_manifest::{MANIFEST_FILE, Manifest};
use zip::{CompressionMethod, ZipArchive};

use crate::pack::check_references;
use crate::sign::{Signature, signed_message};
use crate::{
    CHECKSUMS_PATH, Checksums, FileEntry, KeyId, Limits, PackageError, PublicKey, SIGNATURE_PATH,
    hex,
};

const MAX_CHECKSUMS_SIZE: u64 = 1024 * 1024;
const MAX_SIGNATURE_SIZE: u64 = 4 * 1024;

/// How strict verification should be.
#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    /// Accept packages with no signature. Only for developer mode.
    pub allow_unsigned: bool,
    /// If not empty, a signed package must be signed by one of these keys.
    pub trusted_keys: Vec<PublicKey>,
}

impl VerifyOptions {
    /// Require a valid signature from any key.
    pub fn signed() -> VerifyOptions {
        VerifyOptions::default()
    }

    /// Developer mode: unsigned packages are accepted, bad signatures are not.
    pub fn developer() -> VerifyOptions {
        VerifyOptions {
            allow_unsigned: true,
            trusted_keys: Vec::new(),
        }
    }
}

/// Who signed a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    /// Fingerprint of the signing key.
    pub key_id: KeyId,
    /// The signing key.
    pub public_key: PublicKey,
}

/// A package whose signature, file list and file contents all checked out.
#[derive(Debug, Clone)]
pub struct VerifiedPackage {
    /// The validated manifest.
    pub manifest: Manifest,
    /// Every file in the package.
    pub checksums: Checksums,
    /// The signer, or `None` for an unsigned package (developer mode only).
    pub signer: Option<Signer>,
}

pub(crate) struct Opened<R> {
    pub archive: ZipArchive<R>,
    pub files: HashMap<String, usize>,
    pub checksums_index: usize,
    pub checksums: Checksums,
    pub manifest: Manifest,
    pub signer: Option<Signer>,
}

impl<R: Read + Seek> Opened<R> {
    pub fn open(mut reader: R, opts: &VerifyOptions) -> Result<Opened<R>, PackageError> {
        let ceiling = Limits::ceiling();
        let declared_entries = precheck_end_record(&mut reader, ceiling.max_files + 2)?;
        let mut archive = ZipArchive::new(reader).map_err(PackageError::zip)?;
        // The zip parser indexes entries by name, so a repeated name would be
        // silently dropped. Compare against the raw count to catch that.
        if archive.len() != declared_entries {
            return Err(PackageError::Archive(
                "archive contains duplicate or hidden entries".into(),
            ));
        }
        if archive.len() > ceiling.max_files + 2 {
            return Err(PackageError::TooManyFiles {
                count: archive.len(),
                limit: ceiling.max_files,
            });
        }

        let mut files = HashMap::new();
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut checksums_index = None;
        let mut signature_index = None;
        for i in 0..archive.len() {
            let f = archive.by_index_raw(i).map_err(PackageError::zip)?;
            let name = std::str::from_utf8(f.name_raw())
                .map_err(|_| PackageError::NonUtf8Name)?
                .to_string();
            check_relative_path(&name).map_err(|issue| PackageError::BadPath {
                path: name.clone(),
                issue,
            })?;
            if f.is_symlink() {
                return Err(PackageError::Symlink(name));
            }
            if f.is_dir() || !f.is_file() {
                return Err(PackageError::NotAFile(name));
            }
            if f.encrypted() {
                return Err(PackageError::Encrypted(name));
            }
            if !matches!(
                f.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            ) {
                return Err(PackageError::UnsupportedCompression(name));
            }
            if let Some(prev) = seen.insert(collision_key(&name), name.clone()) {
                return Err(if prev == name {
                    PackageError::Duplicate(name)
                } else {
                    PackageError::CaseCollision(prev, name)
                });
            }
            match name.as_str() {
                CHECKSUMS_PATH => checksums_index = Some(i),
                SIGNATURE_PATH => signature_index = Some(i),
                other if other.starts_with(".wwp/") => {
                    return Err(PackageError::ReservedPath(name));
                }
                _ => {
                    files.insert(name, i);
                }
            }
        }

        let checksums_index = checksums_index.ok_or(PackageError::MissingChecksums)?;
        let checksums_bytes = read_small(
            &mut archive,
            checksums_index,
            MAX_CHECKSUMS_SIZE,
            CHECKSUMS_PATH,
        )?;

        // Check the signature over the raw bytes before trusting anything else.
        let signer = match signature_index {
            Some(i) => {
                let bytes = read_small(&mut archive, i, MAX_SIGNATURE_SIZE, SIGNATURE_PATH)?;
                let sig = Signature::parse(&bytes)?;
                if !sig
                    .public_key
                    .verify(&signed_message(&checksums_bytes), &sig.signature)
                {
                    return Err(PackageError::BadSignature);
                }
                Some(Signer {
                    key_id: sig.public_key.key_id(),
                    public_key: sig.public_key,
                })
            }
            None => None,
        };
        match &signer {
            None if !opts.allow_unsigned => return Err(PackageError::Unsigned),
            Some(s)
                if !opts.trusted_keys.is_empty() && !opts.trusted_keys.contains(&s.public_key) =>
            {
                return Err(PackageError::UntrustedSigner(s.key_id));
            }
            _ => {}
        }

        let checksums = Checksums::parse(&checksums_bytes)?;
        for entry in checksums.entries() {
            if !files.contains_key(&entry.path) {
                return Err(PackageError::MissingFile(entry.path.clone()));
            }
        }
        let sorted: BTreeMap<_, _> = files.iter().collect();
        for name in sorted.keys() {
            if checksums.get(name).is_none() {
                return Err(PackageError::UnlistedFile((*name).clone()));
            }
        }

        let manifest_entry = checksums
            .get(MANIFEST_FILE)
            .ok_or(PackageError::MissingManifest)?
            .clone();
        if manifest_entry.size > ceiling.max_manifest_size {
            return Err(PackageError::TooLarge {
                size: manifest_entry.size,
                limit: ceiling.max_manifest_size,
            });
        }
        let mut manifest_bytes = Vec::with_capacity(manifest_entry.size as usize);
        copy_verified(
            &mut archive,
            files[MANIFEST_FILE],
            &manifest_entry,
            &mut manifest_bytes,
        )?;
        let manifest = Manifest::from_slice(&manifest_bytes)?;

        let limits = Limits::for_type(manifest.kind);
        if checksums.entries().len() > limits.max_files {
            return Err(PackageError::TooManyFiles {
                count: checksums.entries().len(),
                limit: limits.max_files,
            });
        }
        if checksums.total_size() > limits.max_total_size {
            return Err(PackageError::TooLarge {
                size: checksums.total_size(),
                limit: limits.max_total_size,
            });
        }
        check_references(&manifest, &checksums)?;

        Ok(Opened {
            archive,
            files,
            checksums_index,
            checksums,
            manifest,
            signer,
        })
    }

    pub fn copy_file(
        &mut self,
        entry: &FileEntry,
        out: &mut impl Write,
    ) -> Result<(), PackageError> {
        let index = self.files[&entry.path];
        copy_verified(&mut self.archive, index, entry, out)
    }

    pub fn verify_all(&mut self) -> Result<(), PackageError> {
        let entries = self.checksums.entries().to_vec();
        for entry in &entries {
            self.copy_file(entry, &mut std::io::sink())?;
        }
        Ok(())
    }

    pub fn into_verified(self) -> VerifiedPackage {
        VerifiedPackage {
            manifest: self.manifest,
            checksums: self.checksums,
            signer: self.signer,
        }
    }
}

/// Fully verify a package without writing anything to disk.
pub fn verify<R: Read + Seek>(
    reader: R,
    opts: &VerifyOptions,
) -> Result<VerifiedPackage, PackageError> {
    let mut opened = Opened::open(reader, opts)?;
    opened.verify_all()?;
    Ok(opened.into_verified())
}

/// Verify a package and unpack it into `dest`, which must not exist yet.
///
/// Files are written to a sibling staging folder created for this call, each
/// with `create_new`, and hashed as they are written. The staging folder is
/// renamed to `dest` only after every file matched; on any error it is
/// removed and `dest` is never created.
pub fn unpack<R: Read + Seek>(
    reader: R,
    dest: &Path,
    opts: &VerifyOptions,
) -> Result<VerifiedPackage, PackageError> {
    if fs::symlink_metadata(dest).is_ok() {
        return Err(PackageError::DestinationExists(dest.display().to_string()));
    }
    let mut opened = Opened::open(reader, opts)?;

    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce)
        .map_err(|e| PackageError::Key(format!("no secure randomness available: {e}")))?;
    let staging = parent.join(format!(".{name}.staging-{}", hex(&nonce)));
    fs::create_dir(&staging)
        .map_err(|e| PackageError::io(format!("creating {}", staging.display()), e))?;

    let result = write_all_files(&mut opened, &staging).and_then(|()| {
        fs::rename(&staging, dest)
            .map_err(|e| PackageError::io(format!("moving package into {}", dest.display()), e))
    });
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    Ok(opened.into_verified())
}

fn write_all_files<R: Read + Seek>(
    opened: &mut Opened<R>,
    root: &Path,
) -> Result<(), PackageError> {
    let entries = opened.checksums.entries().to_vec();
    for entry in &entries {
        let mut target = root.to_path_buf();
        target.extend(entry.path.split('/'));
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| PackageError::io(format!("creating folder for {}", entry.path), e))?;
        }
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| PackageError::io(format!("creating {}", entry.path), e))?;
        let mut writer = BufWriter::new(file);
        opened.copy_file(entry, &mut writer)?;
        writer
            .flush()
            .map_err(|e| PackageError::io(format!("writing {}", entry.path), e))?;
    }
    Ok(())
}

/// Stream one entry into `out`, reading at most its declared size plus one
/// byte, and check size and hash.
fn copy_verified<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    entry: &FileEntry,
    out: &mut impl Write,
) -> Result<(), PackageError> {
    let file = archive.by_index(index).map_err(PackageError::zip)?;
    let mut limited = file.take(entry.size.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match limited.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // The zip reader reports a CRC mismatch as an I/O error.
            Err(_) => return Err(PackageError::HashMismatch(entry.path.clone())),
        };
        total += n as u64;
        if total > entry.size {
            return Err(PackageError::SizeMismatch(entry.path.clone()));
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])
            .map_err(|e| PackageError::io(format!("writing {}", entry.path), e))?;
    }
    if total != entry.size {
        return Err(PackageError::SizeMismatch(entry.path.clone()));
    }
    let digest: [u8; 32] = hasher.finalize().into();
    if digest != entry.sha256 {
        return Err(PackageError::HashMismatch(entry.path.clone()));
    }
    Ok(())
}

fn read_small<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    cap: u64,
    name: &str,
) -> Result<Vec<u8>, PackageError> {
    let file = archive.by_index(index).map_err(PackageError::zip)?;
    let mut out = Vec::new();
    file.take(cap + 1)
        .read_to_end(&mut out)
        .map_err(|_| PackageError::Archive(format!("{name} could not be read")))?;
    if out.len() as u64 > cap {
        return Err(PackageError::Archive(format!("{name} is too large")));
    }
    Ok(out)
}

/// Look at the end-of-central-directory record before handing the file to
/// the zip parser, so a forged entry count can't make it allocate millions
/// of records. Packages never need ZIP64, so ZIP64 archives are refused.
/// Returns the number of entries the record declares.
fn precheck_end_record<R: Read + Seek>(
    r: &mut R,
    max_entries: usize,
) -> Result<usize, PackageError> {
    const EOCD: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const ZIP64_LOCATOR: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
    let io = |e| PackageError::io("reading archive", e);

    let len = r.seek(SeekFrom::End(0)).map_err(io)?;
    if len < 22 {
        return Err(PackageError::Archive("file is too short".into()));
    }
    let tail_len = len.min(22 + 65_535);
    r.seek(SeekFrom::Start(len - tail_len)).map_err(io)?;
    let mut tail = vec![0u8; tail_len as usize];
    r.read_exact(&mut tail).map_err(io)?;

    let found = (0..=tail.len() - 22).rev().find(|&p| {
        tail[p..p + 4] == EOCD && {
            let comment_len = u16::from_le_bytes([tail[p + 20], tail[p + 21]]) as usize;
            p + 22 + comment_len == tail.len()
        }
    });
    let Some(p) = found else {
        return Err(PackageError::Archive(
            "end of central directory not found".into(),
        ));
    };
    let disk_entries = u16::from_le_bytes([tail[p + 8], tail[p + 9]]);
    let total_entries = u16::from_le_bytes([tail[p + 10], tail[p + 11]]);
    let zip64 = disk_entries == 0xFFFF
        || total_entries == 0xFFFF
        || (p >= 20 && tail[p - 20..p - 16] == ZIP64_LOCATOR);
    if zip64 {
        return Err(PackageError::Archive(
            "ZIP64 archives are not accepted".into(),
        ));
    }
    if disk_entries != total_entries {
        return Err(PackageError::Archive(
            "multi-disk archives are not accepted".into(),
        ));
    }
    if total_entries as usize > max_entries {
        return Err(PackageError::TooManyFiles {
            count: total_entries as usize,
            limit: max_entries,
        });
    }
    r.seek(SeekFrom::Start(0)).map_err(io)?;
    Ok(total_entries as usize)
}
