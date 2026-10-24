use ww_manifest::ManifestError;
use ww_manifest::path::PathIssue;

use crate::KeyId;

/// Everything that can go wrong packing, signing, verifying or unpacking.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// File system or stream error.
    #[error("{context}: {source}")]
    Io {
        /// What was being done.
        context: String,
        /// The underlying error.
        source: std::io::Error,
    },

    /// The zip container itself is damaged or uses features we don't accept.
    #[error("not a valid package archive: {0}")]
    Archive(String),

    /// The archive has no `.wwp/CHECKSUMS`, so it wasn't made by `wallweaver pack`.
    #[error(
        "not a Wallweaver package: .wwp/CHECKSUMS is missing (build packages with `wallweaver pack`)"
    )]
    MissingChecksums,

    /// `.wwp/CHECKSUMS` is not in canonical form.
    #[error("malformed .wwp/CHECKSUMS: {0}")]
    MalformedChecksums(String),

    /// `.wwp/SIGNATURE` could not be parsed.
    #[error("malformed .wwp/SIGNATURE: {0}")]
    MalformedSignature(String),

    /// The signature does not match the checksums. The package was changed
    /// after signing, or the signature is forged.
    #[error("signature check failed: the package was modified after it was signed")]
    BadSignature,

    /// The package is unsigned and the caller did not allow that.
    #[error("package is not signed (unsigned packages are only allowed in developer mode)")]
    Unsigned,

    /// Signed, but not by any of the keys the caller trusts.
    #[error("package is signed by key {0}, which is not one of the trusted keys")]
    UntrustedSigner(KeyId),

    /// A path in the archive or source folder breaks the path rules.
    #[error("bad path '{path}': {issue}")]
    BadPath {
        /// The offending path (lossy if not UTF-8).
        path: String,
        /// Which rule it breaks.
        issue: PathIssue,
    },

    /// A file name is not UTF-8.
    #[error("a file name in the archive is not valid UTF-8")]
    NonUtf8Name,

    /// The same path appears twice.
    #[error("'{0}' appears more than once")]
    Duplicate(String),

    /// Two paths differ only by letter case and would overwrite each other on
    /// Windows or macOS.
    #[error("'{0}' and '{1}' differ only in letter case")]
    CaseCollision(String, String),

    /// A symbolic link, in the archive or in the source folder.
    #[error("'{0}' is a symbolic link; packages may only contain regular files")]
    Symlink(String),

    /// A directory entry, device or other non-file entry.
    #[error("'{0}' is not a regular file")]
    NotAFile(String),

    /// An encrypted zip entry.
    #[error("'{0}' is encrypted, which packages never are")]
    Encrypted(String),

    /// A compression method other than stored or deflate.
    #[error("'{0}' uses an unsupported compression method")]
    UnsupportedCompression(String),

    /// More files than the limit.
    #[error("package has {count} files; the limit is {limit}")]
    TooManyFiles {
        /// Files found.
        count: usize,
        /// The cap.
        limit: usize,
    },

    /// Larger than the limit for its type.
    #[error("package contents total {size} bytes; the limit for this type is {limit} bytes")]
    TooLarge {
        /// Total bytes.
        size: u64,
        /// The cap.
        limit: u64,
    },

    /// A file listed in CHECKSUMS is not in the archive.
    #[error("'{0}' is listed in CHECKSUMS but missing from the archive")]
    MissingFile(String),

    /// A file in the archive is not listed in CHECKSUMS.
    #[error("'{0}' is in the archive but not listed in CHECKSUMS")]
    UnlistedFile(String),

    /// A file's content does not match its checksum.
    #[error("'{0}' does not match its checksum")]
    HashMismatch(String),

    /// A file is longer or shorter than CHECKSUMS says.
    #[error("'{0}' is not the size CHECKSUMS says it is")]
    SizeMismatch(String),

    /// `wallpaper.json` is missing.
    #[error("wallpaper.json is missing from the package root")]
    MissingManifest,

    /// `wallpaper.json` failed to parse or validate.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The manifest's `entry` or `preview` file is not in the package.
    #[error("{field} file '{path}' is not in the package")]
    ReferencedFileMissing {
        /// `entry` or `preview`.
        field: &'static str,
        /// The path from the manifest.
        path: String,
    },

    /// The source folder contains a Wallweaver secret key.
    #[error("refusing to pack '{0}': it looks like a secret signing key")]
    SecretKeyInSource(String),

    /// The source folder uses the reserved `.wwp/` folder.
    #[error("'{0}' is inside .wwp/, which is reserved for package metadata")]
    ReservedPath(String),

    /// The unpack destination already exists.
    #[error("destination '{0}' already exists")]
    DestinationExists(String),

    /// A key file could not be parsed or a key could not be generated.
    #[error("key error: {0}")]
    Key(String),
}

impl PackageError {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        PackageError::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn zip(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::Io(e) => PackageError::io("reading archive", e),
            other => PackageError::Archive(other.to_string()),
        }
    }
}
