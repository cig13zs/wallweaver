use std::collections::HashMap;

use ww_manifest::path::{check_relative_path, collision_key};

use crate::{META_DIR, PackageError, hex};

const HEADER: &str = "wallweaver-checksums v1";

/// One file listed in `.wwp/CHECKSUMS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Package-relative path.
    pub path: String,
    /// Exact size in bytes.
    pub size: u64,
    /// SHA-256 of the content.
    pub sha256: [u8; 32],
}

/// The parsed `.wwp/CHECKSUMS` file.
///
/// The text form is canonical: a header line, then one
/// `<sha256 hex> <size> <path>` line per file, sorted by path bytes, each
/// ending in `\n`. Parsing re-renders the list and requires the bytes to
/// match exactly, so there is only ever one valid encoding of a given file
/// list and the signature covers exactly what is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksums {
    entries: Vec<FileEntry>,
}

impl Checksums {
    /// Build from entries, validating paths and sorting.
    pub fn new(mut entries: Vec<FileEntry>) -> Result<Checksums, PackageError> {
        entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        let mut seen: HashMap<String, &str> = HashMap::new();
        for e in &entries {
            check_relative_path(&e.path).map_err(|issue| PackageError::BadPath {
                path: e.path.clone(),
                issue,
            })?;
            if e.path == META_DIR || e.path.starts_with(".wwp/") {
                return Err(PackageError::ReservedPath(e.path.clone()));
            }
            if let Some(prev) = seen.insert(collision_key(&e.path), &e.path) {
                return Err(if prev == e.path {
                    PackageError::Duplicate(e.path.clone())
                } else {
                    PackageError::CaseCollision(prev.to_string(), e.path.clone())
                });
            }
        }
        Ok(Checksums { entries })
    }

    /// Entries in canonical order.
    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Look up one entry.
    pub fn get(&self, path: &str) -> Option<&FileEntry> {
        self.entries
            .binary_search_by(|e| e.path.as_bytes().cmp(path.as_bytes()))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Sum of all file sizes.
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Canonical text form.
    pub fn render(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        for e in &self.entries {
            out.push_str(&hex(&e.sha256));
            out.push(' ');
            out.push_str(&e.size.to_string());
            out.push(' ');
            out.push_str(&e.path);
            out.push('\n');
        }
        out
    }

    /// Parse the canonical text form.
    pub fn parse(bytes: &[u8]) -> Result<Checksums, PackageError> {
        let bad = |msg: &str| PackageError::MalformedChecksums(msg.to_string());
        let text = std::str::from_utf8(bytes).map_err(|_| bad("not UTF-8"))?;
        let mut lines = text.split_terminator('\n');
        if lines.next() != Some(HEADER) {
            return Err(bad("unknown header"));
        }
        let mut entries = Vec::new();
        for (n, line) in lines.enumerate() {
            let lineno = n + 2;
            let mut parts = line.splitn(3, ' ');
            let (Some(h), Some(size), Some(path)) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(bad(&format!("line {lineno} has too few fields")));
            };
            let sha256 = parse_hash(h).ok_or_else(|| bad(&format!("line {lineno}: bad hash")))?;
            let size: u64 = size
                .parse()
                .map_err(|_| bad(&format!("line {lineno}: bad size")))?;
            entries.push(FileEntry {
                path: path.to_string(),
                size,
                sha256,
            });
        }
        let parsed = Checksums::new(entries)?;
        if parsed.render().as_bytes() != bytes {
            return Err(bad("not in canonical form"));
        }
        Ok(parsed)
    }
}

fn parse_hash(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64, fill: u8) -> FileEntry {
        FileEntry {
            path: path.into(),
            size,
            sha256: [fill; 32],
        }
    }

    #[test]
    fn renders_sorted_and_parses_back() {
        let c = Checksums::new(vec![
            entry("wallpaper.json", 10, 1),
            entry("assets/a.png", 20, 2),
            entry("index.html", 30, 3),
        ])
        .unwrap();
        let text = c.render();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], HEADER);
        assert!(lines[1].ends_with(" 20 assets/a.png"));
        assert!(lines[3].ends_with(" 10 wallpaper.json"));
        assert_eq!(Checksums::parse(text.as_bytes()).unwrap(), c);
        assert_eq!(c.total_size(), 60);
        assert_eq!(c.get("index.html").unwrap().size, 30);
        assert!(c.get("missing").is_none());
    }

    #[test]
    fn non_canonical_forms_are_rejected() {
        let c = Checksums::new(vec![entry("b", 1, 1), entry("a", 2, 2)]).unwrap();
        let text = c.render();
        // CRLF
        assert!(Checksums::parse(text.replace('\n', "\r\n").as_bytes()).is_err());
        // Missing trailing newline
        assert!(Checksums::parse(text.trim_end().as_bytes()).is_err());
        // Wrong order
        let mut lines: Vec<_> = text.lines().collect();
        lines.swap(1, 2);
        let swapped = lines.join("\n") + "\n";
        assert!(Checksums::parse(swapped.as_bytes()).is_err());
        // Uppercase hex
        let lower = Checksums::new(vec![entry("a", 1, 0xab)]).unwrap().render();
        assert!(Checksums::parse(lower.as_bytes()).is_ok());
        assert!(Checksums::parse(lower.replace("abab", "ABAB").as_bytes()).is_err());
        // Leading zero in size
        assert!(Checksums::parse(text.replace(" 1 b", " 01 b").as_bytes()).is_err());
        // Bad header
        assert!(Checksums::parse(text.replace("v1", "v2").as_bytes()).is_err());
    }

    #[test]
    fn rejects_bad_paths_duplicates_and_collisions() {
        assert!(matches!(
            Checksums::new(vec![entry("../x", 1, 1)]),
            Err(PackageError::BadPath { .. })
        ));
        assert!(matches!(
            Checksums::new(vec![entry("a", 1, 1), entry("a", 1, 1)]),
            Err(PackageError::Duplicate(_))
        ));
        assert!(matches!(
            Checksums::new(vec![entry("A.png", 1, 1), entry("a.png", 1, 1)]),
            Err(PackageError::CaseCollision(..))
        ));
        assert!(matches!(
            Checksums::new(vec![entry(".wwp/SIGNATURE", 1, 1)]),
            Err(PackageError::ReservedPath(_))
        ));
    }

    #[test]
    fn paths_with_spaces_parse() {
        let c = Checksums::new(vec![entry("my file.png", 1, 1)]).unwrap();
        assert_eq!(Checksums::parse(c.render().as_bytes()).unwrap(), c);
    }
}
