//! The `.wwp` package format.
//!
//! A `.wwp` file is a zip archive with this layout:
//!
//! ```text
//! .wwp/CHECKSUMS     canonical list of every file: sha256, size, path
//! .wwp/SIGNATURE     optional ed25519 signature over CHECKSUMS
//! wallpaper.json     the manifest (also listed in CHECKSUMS)
//! ...                the package's own files
//! ```
//!
//! Reading a package is defensive by default:
//!
//! * the signature is checked before any file content is trusted, and a bad
//!   signature is always an error (it never falls back to "unsigned")
//! * every entry must be a regular file whose path passes
//!   [`ww_manifest::path::check_relative_path`]; symlinks, directories,
//!   encrypted entries, duplicate names and names that collide on a
//!   case-insensitive file system are refused
//! * the set of files must match `CHECKSUMS` exactly, and each file is hashed
//!   while it is read, reading at most its declared size plus one byte, so
//!   zip headers that lie about sizes don't matter
//! * file count and total size are capped (500 MB for video packages,
//!   50 MB for everything else)
//! * [`unpack`] writes into a fresh staging folder with `create_new` and only
//!   moves it into place once every hash has matched
//!
//! Packing is deterministic: files are sorted, timestamps and permissions are
//! fixed, so the same folder always produces the same bytes.

mod checksums;
mod error;
mod keys;
mod pack;
mod read;
mod sign;

pub use checksums::{Checksums, FileEntry};
pub use error::PackageError;
pub use keys::{KeyId, PublicKey, SecretKey};
pub use pack::{PackReport, pack_dir};
pub use read::{Signer, VerifiedPackage, VerifyOptions, unpack, verify};
pub use sign::sign_package;

use ww_manifest::WallpaperType;

/// Path of the checksum list inside the archive.
pub const CHECKSUMS_PATH: &str = ".wwp/CHECKSUMS";
/// Path of the signature inside the archive.
pub const SIGNATURE_PATH: &str = ".wwp/SIGNATURE";
/// Folder reserved for package metadata.
pub const META_DIR: &str = ".wwp";

/// File extension for packages.
pub const PACKAGE_EXTENSION: &str = "wwp";

const MIB: u64 = 1024 * 1024;

/// Size and count caps applied when packing and reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of files, not counting `.wwp/` metadata.
    pub max_files: usize,
    /// Maximum total uncompressed size of all files, in bytes.
    pub max_total_size: u64,
    /// Maximum size of `wallpaper.json`, in bytes.
    pub max_manifest_size: u64,
}

impl Limits {
    /// The limits for a package of the given type.
    pub fn for_type(kind: WallpaperType) -> Limits {
        let max_total_size = match kind {
            WallpaperType::Video => 500 * MIB,
            _ => 50 * MIB,
        };
        Limits {
            max_files: 2_000,
            max_total_size,
            max_manifest_size: 64 * 1024,
        }
    }

    /// The loosest limits of any type, used before the manifest is known.
    pub fn ceiling() -> Limits {
        Limits::for_type(WallpaperType::Video)
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests;
