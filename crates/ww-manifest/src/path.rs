//! Rules for file paths inside a package.
//!
//! The same rules apply to `entry` and `preview` in the manifest and to every
//! file name in a `.wwp` archive. They are deliberately narrower than what any
//! one OS allows, so a package that unpacks on Linux also unpacks identically
//! on Windows and macOS:
//!
//! * relative, `/`-separated, no `.` or `..` components, no empty components
//! * printable ASCII only (no Unicode normalization surprises)
//! * none of `\ : * ? " < > |` and no control characters
//! * no component that Windows treats specially (`CON`, `nul.txt`, `COM1`, ...)
//!   and no component ending in a dot or a space
//! * at most 240 bytes and 16 levels deep

use std::fmt;

/// Maximum length of a package path in bytes.
pub const MAX_PATH_LEN: usize = 240;
/// Maximum number of components in a package path.
pub const MAX_DEPTH: usize = 16;

/// Why a path was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathIssue {
    /// The path is empty.
    Empty,
    /// Longer than [`MAX_PATH_LEN`].
    TooLong,
    /// Deeper than [`MAX_DEPTH`].
    TooDeep,
    /// Starts with `/`, or with a drive letter such as `C:`.
    Absolute,
    /// Contains a `..` component.
    ParentDir,
    /// Contains a `.` component, a trailing slash or `//`.
    NotNormalized,
    /// Uses a backslash as a separator.
    Backslash,
    /// Contains a character outside printable ASCII, or one Windows forbids.
    BadChar(char),
    /// A component Windows reserves, such as `CON` or `com1.txt`.
    Reserved(String),
    /// A component ending in `.` or a space, which Windows silently strips.
    TrailingDotOrSpace(String),
}

impl fmt::Display for PathIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathIssue::Empty => f.write_str("path is empty"),
            PathIssue::TooLong => write!(f, "path is longer than {MAX_PATH_LEN} bytes"),
            PathIssue::TooDeep => write!(f, "path is nested more than {MAX_DEPTH} folders deep"),
            PathIssue::Absolute => f.write_str("path must be relative to the package root"),
            PathIssue::ParentDir => f.write_str("path must not contain '..'"),
            PathIssue::NotNormalized => {
                f.write_str("path must not contain '.', empty segments or a trailing '/'")
            }
            PathIssue::Backslash => f.write_str("use '/' as the separator, not '\\'"),
            PathIssue::BadChar(c) if c.is_ascii_graphic() || *c == ' ' => {
                write!(f, "path contains the character '{c}', which is not allowed")
            }
            PathIssue::BadChar(c) => write!(
                f,
                "path contains U+{:04X}; only printable ASCII is allowed",
                *c as u32
            ),
            PathIssue::Reserved(name) => {
                write!(f, "'{name}' is a reserved file name on Windows")
            }
            PathIssue::TrailingDotOrSpace(name) => {
                write!(f, "'{name}' ends in a dot or space, which Windows strips")
            }
        }
    }
}

impl std::error::Error for PathIssue {}

const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Check a package-relative path against the rules in the module docs.
pub fn check_relative_path(path: &str) -> Result<(), PathIssue> {
    if path.is_empty() {
        return Err(PathIssue::Empty);
    }
    if path.len() > MAX_PATH_LEN {
        return Err(PathIssue::TooLong);
    }
    if path.starts_with('/') {
        return Err(PathIssue::Absolute);
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(PathIssue::Absolute);
    }
    if path.contains('\\') {
        return Err(PathIssue::Backslash);
    }
    for c in path.chars() {
        let printable = c == ' ' || c.is_ascii_graphic();
        if !printable || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return Err(PathIssue::BadChar(c));
        }
    }
    let mut depth = 0;
    for component in path.split('/') {
        depth += 1;
        match component {
            "" | "." => return Err(PathIssue::NotNormalized),
            ".." => return Err(PathIssue::ParentDir),
            _ => {}
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(PathIssue::TrailingDotOrSpace(component.to_string()));
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .trim_end()
            .to_ascii_lowercase();
        if WINDOWS_RESERVED.contains(&stem.as_str()) {
            return Err(PathIssue::Reserved(component.to_string()));
        }
    }
    if depth > MAX_DEPTH {
        return Err(PathIssue::TooDeep);
    }
    Ok(())
}

/// Key used to detect two paths that would collide on a case-insensitive
/// file system (Windows, default macOS).
pub fn collision_key(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// The lowercase extension of the last component, if any.
pub fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        None
    } else {
        Some(ext.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_paths() {
        for p in [
            "index.html",
            "assets/ridge.svg",
            "shaders/aurora.frag",
            "a/b/c/d.txt",
            "my file.png",
            ".well-known/x",
            "con-artist.png",
            "console.js",
        ] {
            assert_eq!(check_relative_path(p), Ok(()), "{p}");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        assert_eq!(check_relative_path("../x"), Err(PathIssue::ParentDir));
        assert_eq!(check_relative_path("a/../../x"), Err(PathIssue::ParentDir));
        assert_eq!(check_relative_path("/etc/passwd"), Err(PathIssue::Absolute));
        assert_eq!(check_relative_path("C:/Windows"), Err(PathIssue::Absolute));
        assert_eq!(check_relative_path("c:x"), Err(PathIssue::Absolute));
        assert_eq!(check_relative_path("a\\..\\x"), Err(PathIssue::Backslash));
    }

    #[test]
    fn rejects_unnormalized() {
        for p in ["./a", "a/./b", "a//b", "a/", ""] {
            assert!(check_relative_path(p).is_err(), "{p}");
        }
    }

    #[test]
    fn rejects_windows_hazards() {
        assert!(matches!(
            check_relative_path("CON"),
            Err(PathIssue::Reserved(_))
        ));
        assert!(matches!(
            check_relative_path("assets/nul.txt"),
            Err(PathIssue::Reserved(_))
        ));
        assert!(matches!(
            check_relative_path("Com1.js"),
            Err(PathIssue::Reserved(_))
        ));
        assert!(matches!(
            check_relative_path("file."),
            Err(PathIssue::TrailingDotOrSpace(_))
        ));
        assert!(matches!(
            check_relative_path("dir /x"),
            Err(PathIssue::TrailingDotOrSpace(_))
        ));
        assert_eq!(
            check_relative_path("file.txt:stream"),
            Err(PathIssue::BadChar(':'))
        );
        assert_eq!(check_relative_path("a?.txt"), Err(PathIssue::BadChar('?')));
    }

    #[test]
    fn rejects_non_ascii_and_control() {
        assert_eq!(
            check_relative_path("caf\u{e9}.png"),
            Err(PathIssue::BadChar('\u{e9}'))
        );
        assert_eq!(check_relative_path("a\0b"), Err(PathIssue::BadChar('\0')));
        assert_eq!(check_relative_path("a\nb"), Err(PathIssue::BadChar('\n')));
    }

    #[test]
    fn rejects_long_and_deep() {
        let long = "a".repeat(MAX_PATH_LEN + 1);
        assert_eq!(check_relative_path(&long), Err(PathIssue::TooLong));
        let deep = vec!["d"; MAX_DEPTH + 1].join("/");
        assert_eq!(check_relative_path(&deep), Err(PathIssue::TooDeep));
        let ok = vec!["d"; MAX_DEPTH].join("/");
        assert_eq!(check_relative_path(&ok), Ok(()));
    }

    #[test]
    fn extension_and_collisions() {
        assert_eq!(extension("a/b/Index.HTML").as_deref(), Some("html"));
        assert_eq!(extension("README"), None);
        assert_eq!(extension(".hidden"), None);
        assert_eq!(collision_key("Assets/A.PNG"), collision_key("assets/a.png"));
    }
}
