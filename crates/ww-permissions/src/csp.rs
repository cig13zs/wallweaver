use crate::{Capability, Effective};

/// Build the `Content-Security-Policy` header for a web wallpaper.
///
/// Content is only ever served from the `ww:` scheme. Everything not listed
/// falls back to `default-src 'none'`. Network origins appear in
/// `connect-src` only when granted; `'unsafe-eval'` only with
/// [`Capability::ScriptEval`]. Frames, plugins, forms and `<base>` are always
/// off.
pub fn content_security_policy(effective: &Effective) -> String {
    let mut connect = vec!["ww:".to_string()];
    let mut any = false;
    let mut local = false;
    for cap in effective.iter() {
        match cap {
            Capability::Network(o) => connect.push(o.to_string()),
            Capability::NetworkAny => any = true,
            Capability::LocalNetwork => local = true,
            _ => {}
        }
    }
    if any {
        connect.retain(|c| c == "ww:");
        connect.push("https:".into());
        connect.push("wss:".into());
    }
    if local {
        for s in ["http:", "ws:"] {
            connect.push(s.into());
        }
        if !any {
            connect.push("https:".into());
            connect.push("wss:".into());
        }
    }

    let mut script = vec!["ww:", "'wasm-unsafe-eval'"];
    if effective.allows(&Capability::ScriptEval) {
        script.push("'unsafe-eval'");
    }

    let directives = [
        ("default-src", "'none'".to_string()),
        ("script-src", script.join(" ")),
        ("style-src", "ww: 'unsafe-inline'".to_string()),
        ("img-src", "ww: data: blob:".to_string()),
        ("media-src", "ww: blob:".to_string()),
        ("font-src", "ww: data:".to_string()),
        ("worker-src", "ww: blob:".to_string()),
        ("connect-src", connect.join(" ")),
        ("frame-src", "'none'".to_string()),
        ("object-src", "'none'".to_string()),
        ("base-uri", "'none'".to_string()),
        ("form-action", "'none'".to_string()),
        ("frame-ancestors", "'none'".to_string()),
    ];
    directives
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join("; ")
}
