use std::collections::HashSet;

use crate::path::{check_relative_path, extension};
use crate::{Issue, MANIFEST_VERSION, Manifest, NetworkOrigin, Property};

const MAX_NAME: usize = 80;
const MAX_DESCRIPTION: usize = 500;
const MAX_TAGS: usize = 10;
const MAX_TAG_LEN: usize = 24;
const MAX_PROPERTIES: usize = 64;
const MAX_LABEL: usize = 40;
const MAX_COMBO_OPTIONS: usize = 32;
const MAX_NETWORK_ORIGINS: usize = 8;
const PREVIEW_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

pub(crate) fn check(m: &Manifest) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut push = |path: &str, msg: String| issues.push(Issue::new(path, msg));

    if m.manifest_version != MANIFEST_VERSION {
        push(
            "manifestVersion",
            format!(
                "version {} is not supported; this version of Wallweaver reads manifestVersion {MANIFEST_VERSION}",
                m.manifest_version
            ),
        );
    }

    if let Err(msg) = check_id(&m.id) {
        push("id", msg);
    }

    check_text(&m.name, "name", MAX_NAME, true, &mut push);

    match check_relative_path(&m.entry) {
        Err(e) => push("entry", e.to_string()),
        Ok(()) => {
            let allowed = m.kind.entry_extensions();
            let ext = extension(&m.entry).unwrap_or_default();
            if !allowed.contains(&ext.as_str()) {
                push(
                    "entry",
                    format!(
                        "a {} package needs an entry ending in {}",
                        m.kind,
                        allowed
                            .iter()
                            .map(|e| format!(".{e}"))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ),
                );
            }
        }
    }

    check_text(&m.author.name, "author.name", MAX_NAME, true, &mut push);
    if let Some(url) = &m.author.url {
        match url::Url::parse(url) {
            Ok(u) if u.scheme() == "https" => {}
            Ok(_) => push("author.url", "must be an https:// link".to_string()),
            Err(_) => push("author.url", format!("'{url}' is not a valid URL")),
        }
    }

    if m.license.trim().is_empty() {
        push(
            "license",
            "is empty; use an SPDX id such as MIT or CC-BY-4.0".to_string(),
        );
    } else if let Err(e) = spdx::Expression::parse(&m.license) {
        push(
            "license",
            format!(
                "'{}' is not a valid SPDX expression ({}); use an id such as MIT, CC-BY-4.0 or LicenseRef-MyLicense",
                m.license, e.reason
            ),
        );
    }

    if let Some(d) = &m.description {
        check_text(d, "description", MAX_DESCRIPTION, false, &mut push);
    }

    if m.tags.len() > MAX_TAGS {
        push("tags", format!("at most {MAX_TAGS} tags are allowed"));
    }
    let mut seen_tags = HashSet::new();
    for (i, tag) in m.tags.iter().enumerate() {
        let path = format!("tags[{i}]");
        let valid = !tag.is_empty()
            && tag.len() <= MAX_TAG_LEN
            && tag
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            push(
                &path,
                format!(
                    "'{tag}' must be 1 to {MAX_TAG_LEN} characters of lowercase letters, digits and dashes"
                ),
            );
        } else if !seen_tags.insert(tag.as_str()) {
            push(&path, format!("'{tag}' is listed twice"));
        }
    }

    if let Some(preview) = &m.preview {
        match check_relative_path(preview) {
            Err(e) => push("preview", e.to_string()),
            Ok(()) => {
                let ext = extension(preview).unwrap_or_default();
                if !PREVIEW_EXTENSIONS.contains(&ext.as_str()) {
                    push("preview", "must be a .png, .jpg or .webp image".to_string());
                }
            }
        }
    }

    if m.properties.len() > MAX_PROPERTIES {
        push(
            "properties",
            format!("at most {MAX_PROPERTIES} properties are allowed"),
        );
    }
    for (key, prop) in &m.properties {
        let base = format!("properties.{key}");
        if !valid_property_key(key) {
            push(
                &base,
                "property names must start with a letter and use only letters, digits and '_' (max 32)"
                    .to_string(),
            );
        }
        check_text(
            prop.label(),
            &format!("{base}.label"),
            MAX_LABEL,
            true,
            &mut push,
        );
        check_property(prop, &base, &mut push);
    }

    let perms = &m.permissions;
    let mut seen_stats = HashSet::new();
    for (i, stat) in perms.system_stats.iter().enumerate() {
        if !seen_stats.insert(*stat) {
            push(
                &format!("permissions.system.stats[{i}]"),
                format!("'{}' is listed twice", stat.as_str()),
            );
        }
    }
    if perms.network.len() > MAX_NETWORK_ORIGINS {
        push(
            "permissions.network",
            format!("at most {MAX_NETWORK_ORIGINS} origins are allowed"),
        );
    }
    let mut seen_origins = HashSet::new();
    for (i, origin) in perms.network.iter().enumerate() {
        let path = format!("permissions.network[{i}]");
        match NetworkOrigin::parse(origin) {
            Err(e) => push(&path, e.to_string()),
            Ok(o) => {
                if !seen_origins.insert(o.clone()) {
                    push(&path, format!("'{o}' is listed twice"));
                }
            }
        }
    }

    issues
}

fn check_id(id: &str) -> Result<(), String> {
    let len_ok = (3..=64).contains(&id.len());
    let chars_ok = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    let shape_ok = !id.starts_with(['.', '-'])
        && !id.ends_with(['.', '-'])
        && !id.contains("..")
        && !id.contains("--")
        && !id.contains(".-")
        && !id.contains("-.");
    if len_ok && chars_ok && shape_ok {
        Ok(())
    } else {
        Err(format!(
            "'{id}' is not a valid id; use 3 to 64 lowercase letters, digits, dots and single dashes, like 'mira.aurora-loom'"
        ))
    }
}

fn check_text(
    value: &str,
    path: &str,
    max_chars: usize,
    required: bool,
    push: &mut impl FnMut(&str, String),
) {
    if required && value.trim().is_empty() {
        push(path, "must not be empty".to_string());
        return;
    }
    if value.chars().count() > max_chars {
        push(path, format!("must be at most {max_chars} characters"));
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        push(path, "must not contain control characters".to_string());
    }
}

fn valid_property_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && key.len() <= 32
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn valid_color(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else {
        return false;
    };
    (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn check_property(prop: &Property, base: &str, push: &mut impl FnMut(&str, String)) {
    match prop {
        Property::Slider {
            default,
            min,
            max,
            step,
            ..
        } => {
            if ![*default, *min, *max].iter().all(|v| v.is_finite()) {
                push(base, "slider values must be finite numbers".to_string());
                return;
            }
            if min >= max {
                push(
                    &format!("{base}.min"),
                    format!("min ({min}) must be less than max ({max})"),
                );
            } else if default < min || default > max {
                push(
                    &format!("{base}.default"),
                    format!("default ({default}) must be between min ({min}) and max ({max})"),
                );
            }
            if let Some(step) = step
                && (!step.is_finite() || *step <= 0.0 || (max > min && *step > max - min))
            {
                push(
                    &format!("{base}.step"),
                    "step must be greater than 0 and no larger than max - min".to_string(),
                );
            }
        }
        Property::Color { default, .. } => {
            if !valid_color(default) {
                push(
                    &format!("{base}.default"),
                    format!("'{default}' is not a color; use #RRGGBB or #RRGGBBAA"),
                );
            }
        }
        Property::Bool { .. } => {}
        Property::Combo {
            default, options, ..
        } => {
            if options.is_empty() || options.len() > MAX_COMBO_OPTIONS {
                push(
                    &format!("{base}.options"),
                    format!("needs between 1 and {MAX_COMBO_OPTIONS} options"),
                );
            }
            let mut values = HashSet::new();
            for (i, opt) in options.iter().enumerate() {
                let path = format!("{base}.options[{i}]");
                if opt.value.is_empty() || opt.value.len() > 64 {
                    push(&path, "value must be 1 to 64 characters".to_string());
                }
                if !values.insert(opt.value.as_str()) {
                    push(&path, format!("value '{}' is listed twice", opt.value));
                }
                check_text(&opt.label, &format!("{path}.label"), MAX_LABEL, true, push);
            }
            if !options.is_empty() && !values.contains(default.as_str()) {
                push(
                    &format!("{base}.default"),
                    format!("'{default}' is not one of the option values"),
                );
            }
        }
    }
}
