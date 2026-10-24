use std::fmt;

/// A network origin a package may contact, such as `https://api.open-meteo.com`.
///
/// Only `https` and `wss` are accepted. The origin must name a public domain:
/// IP literals, `localhost` and local-network suffixes are refused so a
/// wallpaper can't probe your router or services on your machine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetworkOrigin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

/// Why an origin string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OriginError {
    /// Not a URL at all.
    #[error("'{0}' is not a valid URL")]
    Unparseable(String),
    /// http or ws instead of https or wss.
    #[error("'{0}' is not encrypted; use https:// or wss://")]
    NotSecure(String),
    /// Some other scheme, such as file: or data:.
    #[error("'{0}' must start with https:// or wss://")]
    UnsupportedScheme(String),
    /// Contains `*`.
    #[error("'{0}' contains a wildcard; list each origin separately")]
    Wildcard(String),
    /// Has a user name or password.
    #[error("'{0}' must not contain a user name or password")]
    Credentials(String),
    /// Has a path, query or fragment.
    #[error("'{0}' must be an origin only, with no path, query or fragment")]
    NotAnOrigin(String),
    /// Uses an IP address.
    #[error("'{0}' uses an IP address; use a domain name")]
    IpLiteral(String),
    /// Points at the local machine or local network.
    #[error("'{0}' points at this computer or the local network, which is never allowed")]
    LocalHost(String),
}

const LOCAL_SUFFIXES: &[&str] = &[
    ".localhost",
    ".local",
    ".localdomain",
    ".internal",
    ".intranet",
    ".lan",
    ".home",
    ".home.arpa",
    ".corp",
    ".test",
    ".invalid",
];

impl NetworkOrigin {
    /// Parse and normalize an origin string.
    pub fn parse(input: &str) -> Result<NetworkOrigin, OriginError> {
        let owned = || input.to_string();
        if input.contains('*') {
            return Err(OriginError::Wildcard(owned()));
        }
        if input.trim() != input || input.is_empty() {
            return Err(OriginError::Unparseable(owned()));
        }
        let url = url::Url::parse(input).map_err(|_| OriginError::Unparseable(owned()))?;
        match url.scheme() {
            "https" | "wss" => {}
            "http" | "ws" => return Err(OriginError::NotSecure(owned())),
            _ => return Err(OriginError::UnsupportedScheme(owned())),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OriginError::Credentials(owned()));
        }
        if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
            return Err(OriginError::NotAnOrigin(owned()));
        }
        // "https://example.com/" parses to path "/" too; accept the trailing
        // slash but nothing after it.
        let after_scheme = &input[url.scheme().len() + 3..];
        if let Some(slash) = after_scheme.find('/')
            && slash + 1 != after_scheme.len()
        {
            return Err(OriginError::NotAnOrigin(owned()));
        }
        let host = match url.host() {
            Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
            Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_)) => {
                return Err(OriginError::IpLiteral(owned()));
            }
            None => return Err(OriginError::Unparseable(owned())),
        };
        let host = host.trim_end_matches('.').to_string();
        if host == "localhost"
            || !host.contains('.')
            || LOCAL_SUFFIXES.iter().any(|s| host.ends_with(s))
        {
            return Err(OriginError::LocalHost(owned()));
        }
        Ok(NetworkOrigin {
            scheme: url.scheme().to_string(),
            host,
            port: url.port(),
        })
    }

    /// `https` or `wss`.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Lowercase host name.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Explicit port, if it differs from the scheme default.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// True if `url` is on exactly this origin (scheme, host and port).
    pub fn matches(&self, url: &url::Url) -> bool {
        let Some(url::Host::Domain(host)) = url.host() else {
            return false;
        };
        url.scheme() == self.scheme
            && host.trim_end_matches('.').eq_ignore_ascii_case(&self.host)
            && url.port() == self.port
    }
}

impl fmt::Display for NetworkOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_good_origins() {
        let o = NetworkOrigin::parse("https://API.Open-Meteo.com").unwrap();
        assert_eq!(o.to_string(), "https://api.open-meteo.com");
        let o = NetworkOrigin::parse("https://example.com/").unwrap();
        assert_eq!(o.to_string(), "https://example.com");
        let o = NetworkOrigin::parse("https://example.com:443").unwrap();
        assert_eq!(o.to_string(), "https://example.com");
        let o = NetworkOrigin::parse("wss://stream.example.org:8443").unwrap();
        assert_eq!(o.to_string(), "wss://stream.example.org:8443");
    }

    #[test]
    fn rejects_bad_origins() {
        use OriginError::*;
        type Case = (&'static str, fn(String) -> OriginError);
        let cases: &[Case] = &[
            ("http://example.com", NotSecure),
            ("ws://example.com", NotSecure),
            ("file:///etc/passwd", UnsupportedScheme),
            ("data:text/plain,hi", UnsupportedScheme),
            ("https://*.example.com", Wildcard),
            ("https://user:pw@example.com", Credentials),
            ("https://example.com/api", NotAnOrigin),
            ("https://example.com/?q=1", NotAnOrigin),
            ("https://example.com#x", NotAnOrigin),
            ("https://127.0.0.1", IpLiteral),
            ("https://[::1]", IpLiteral),
            ("https://192.168.1.1:8080", IpLiteral),
            ("https://localhost", LocalHost),
            ("https://printer.local", LocalHost),
            ("https://nas.home.arpa", LocalHost),
            ("https://intranet", LocalHost),
            ("https://app.localhost", LocalHost),
            ("not a url", Unparseable),
            (" https://example.com", Unparseable),
        ];
        for (input, want) in cases {
            assert_eq!(
                NetworkOrigin::parse(input),
                Err(want(input.to_string())),
                "{input}"
            );
        }
    }

    #[test]
    fn decimal_ip_hosts_are_ip_literals() {
        // The URL parser turns these into IPv4 addresses.
        assert!(matches!(
            NetworkOrigin::parse("https://2130706433"),
            Err(OriginError::IpLiteral(_))
        ));
        assert!(matches!(
            NetworkOrigin::parse("https://0x7f.1"),
            Err(OriginError::IpLiteral(_))
        ));
    }

    #[test]
    fn matches_exact_origin_only() {
        let o = NetworkOrigin::parse("https://api.open-meteo.com").unwrap();
        let yes = url::Url::parse("https://api.open-meteo.com/v1/forecast?x=1").unwrap();
        let other_host = url::Url::parse("https://evil.open-meteo.com/").unwrap();
        let other_scheme = url::Url::parse("http://api.open-meteo.com/").unwrap();
        let other_port = url::Url::parse("https://api.open-meteo.com:8443/").unwrap();
        let suffix = url::Url::parse("https://api.open-meteo.com.evil.net/").unwrap();
        assert!(o.matches(&yes));
        assert!(!o.matches(&other_host));
        assert!(!o.matches(&other_scheme));
        assert!(!o.matches(&other_port));
        assert!(!o.matches(&suffix));
    }
}
