use std::fmt;

/// One problem found while validating a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Where the problem is, as a dotted path such as `permissions.network[0]`.
    pub path: String,
    /// What is wrong, in plain words.
    pub message: String,
}

impl Issue {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Why a manifest was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// The file is not valid UTF-8.
    #[error("wallpaper.json must be UTF-8 text")]
    NotUtf8,

    /// The text is not valid JSON.
    #[error("wallpaper.json is not valid JSON (line {line}, column {column}): {message}")]
    Syntax {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Parser message.
        message: String,
    },

    /// The JSON is valid but a field has the wrong shape: a missing field,
    /// an unknown field or a wrong type.
    #[error(
        "wallpaper.json has the wrong shape at {path} (line {line}, column {column}): {message}"
    )]
    Shape {
        /// Dotted path to the field, or `(root)`.
        path: String,
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// What serde reported.
        message: String,
    },

    /// The manifest parsed but failed one or more checks.
    #[error("{}", format_issues(.0))]
    Invalid(Vec<Issue>),
}

impl ManifestError {
    pub(crate) fn from_serde(err: serde_path_to_error::Error<serde_json::Error>) -> Self {
        let path = err.path().to_string();
        let inner = err.into_inner();
        let line = inner.line();
        let column = inner.column();
        let message = strip_position(&inner.to_string());
        match inner.classify() {
            serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                ManifestError::Syntax {
                    line,
                    column,
                    message,
                }
            }
            _ => ManifestError::Shape {
                path: if path == "." {
                    "(root)".to_string()
                } else {
                    path
                },
                line,
                column,
                message,
            },
        }
    }

    /// The individual issues, if this is a validation failure.
    pub fn issues(&self) -> &[Issue] {
        match self {
            ManifestError::Invalid(issues) => issues,
            _ => &[],
        }
    }
}

fn format_issues(issues: &[Issue]) -> String {
    let mut out = format!(
        "wallpaper.json has {} problem{}:",
        issues.len(),
        if issues.len() == 1 { "" } else { "s" }
    );
    for issue in issues {
        out.push_str("\n  - ");
        out.push_str(&issue.to_string());
    }
    out
}

/// serde_json appends " at line X column Y"; we report position separately.
fn strip_position(msg: &str) -> String {
    match msg.rfind(" at line ") {
        Some(idx) => msg[..idx].to_string(),
        None => msg.to_string(),
    }
}
