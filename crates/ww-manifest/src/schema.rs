use crate::Manifest;

/// Canonical URL of the published schema, used as `$id`.
pub const SCHEMA_ID: &str =
    "https://raw.githubusercontent.com/cig13zs/wallweaver/main/schema/wallpaper.schema.json";

/// JSON Schema (draft 2020-12) for `wallpaper.json`, for editor completion
/// and for tools outside Rust. The Rust validator is stricter than the schema
/// (for example it parses SPDX expressions and network origins), so a file
/// that passes the schema can still be rejected by `wallweaver validate`.
pub fn json_schema() -> serde_json::Value {
    let mut schema = schemars::schema_for!(Manifest);
    if let Some(obj) = schema.as_object_mut() {
        obj.insert("$id".into(), serde_json::Value::String(SCHEMA_ID.into()));
    }
    schema.to_value()
}

/// [`json_schema`] as pretty-printed JSON with a trailing newline.
pub fn json_schema_string() -> String {
    let mut s = serde_json::to_string_pretty(&json_schema()).unwrap_or_default();
    s.push('\n');
    s
}
