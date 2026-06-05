//! RFC-3986 generic URI parsing (hand-port of the `std.Uri.parse` behavior `os/uri` relies on; no
//! URL crate, since none provides verbatim component slices, MAC-address hosts, or `raw_path`).
//! Exp 619 layers `os/uri`'s MAC-address + `raw_path` options on top.

/// A parsed URI; string components are verbatim slices into the parsed input (percent-encoding
/// preserved), mirroring `std.Uri`'s slices-into-text model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Uri<'a> {
    pub(crate) scheme: &'a str,
    pub(crate) user: Option<&'a str>,
    pub(crate) password: Option<&'a str>,
    pub(crate) host: Option<&'a str>,
    pub(crate) port: Option<u16>,
    pub(crate) path: &'a str,
    pub(crate) query: Option<&'a str>,
    pub(crate) fragment: Option<&'a str>,
}

/// URI parse errors (the subset of `std.Uri.ParseError` that `os/uri` distinguishes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseError {
    /// The string is not a valid URI (missing/invalid scheme).
    InvalidFormat,
    /// The port component is not a valid number (the MAC-address case `os/uri` catches).
    InvalidPort,
}

/// Parse a full URI (`scheme ":" …`) — upstream `std.Uri.parse`.
pub(crate) fn parse(text: &str) -> Result<Uri<'_>, ParseError> {
    // Scheme: ALPHA *(ALPHA / DIGIT / "+" / "-" / ".") up to the first ':'.
    let colon = text.find(':').ok_or(ParseError::InvalidFormat)?;
    let scheme = &text[..colon];
    if !is_valid_scheme(scheme) {
        return Err(ParseError::InvalidFormat);
    }
    parse_after_scheme(scheme, &text[colon + 1..])
}

/// Parse the part after `scheme:` — upstream `std.Uri.parseAfterScheme`. `rest` is
/// `hier-part ["?" query] ["#" fragment]`.
pub(crate) fn parse_after_scheme<'a>(
    scheme: &'a str,
    rest: &'a str,
) -> Result<Uri<'a>, ParseError> {
    // Split off the fragment, then the query.
    let (before_fragment, fragment) = split_once(rest, '#');
    let (hier, query) = split_once(before_fragment, '?');

    let mut uri = Uri {
        scheme,
        user: None,
        password: None,
        host: None,
        port: None,
        path: "",
        query,
        fragment,
    };

    if let Some(after) = hier.strip_prefix("//") {
        // authority [path]. The authority runs up to the first '/'.
        let auth_end = after.find('/').unwrap_or(after.len());
        let authority = &after[..auth_end];
        uri.path = &after[auth_end..]; // keeps the leading '/', or "" if none
        parse_authority(&mut uri, authority)?;
    } else {
        // No authority: the whole hier-part is the path.
        uri.path = hier;
    }

    Ok(uri)
}

fn parse_authority<'a>(uri: &mut Uri<'a>, authority: &'a str) -> Result<(), ParseError> {
    // userinfo (everything before the last '@'): user[:password].
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    if let Some(ui) = userinfo {
        let (user, password) = split_once(ui, ':');
        uri.user = Some(user);
        uri.password = password;
    }

    // host [":" port]. For a bracketed IPv6 literal, the port ':' is the one after the ']';
    // otherwise it is the LAST ':' (so a `host:port`-shaped MAC address splits its last octet as a
    // numeric port for `os/uri`'s later repair).
    let port_colon = if host_port.starts_with('[') {
        host_port
            .find(']')
            .and_then(|rb| host_port[rb..].find(':').map(|c| rb + c))
    } else {
        host_port.rfind(':')
    };

    match port_colon {
        Some(c) => {
            uri.host = Some(&host_port[..c]);
            let port_str = &host_port[c + 1..];
            if port_str.is_empty() {
                uri.port = None;
            } else {
                // RFC 3986 port is digits only; reject what `u16::parse` would otherwise accept
                // (e.g. a leading `+`).
                if !port_str.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ParseError::InvalidPort);
                }
                uri.port = Some(
                    port_str
                        .parse::<u16>()
                        .map_err(|_| ParseError::InvalidPort)?,
                );
            }
        }
        None => uri.host = Some(host_port),
    }
    Ok(())
}

fn is_valid_scheme(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
}

/// `(before, Some(after))` if `sep` is present, else `(s, None)`.
fn split_once(s: &str, sep: char) -> (&str, Option<&str>) {
    match s.find(sep) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_http() {
        let u = parse("https://example.com/path?q=1#frag").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, Some("example.com"));
        assert_eq!(u.port, None);
        assert_eq!(u.path, "/path");
        assert_eq!(u.query, Some("q=1"));
        assert_eq!(u.fragment, Some("frag"));
    }

    #[test]
    fn parse_with_port() {
        let u = parse("https://example.com:8080/").unwrap();
        assert_eq!(u.host, Some("example.com"));
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/");
    }

    #[test]
    fn parse_no_authority() {
        let u = parse("mailto:user@example.com").unwrap();
        assert_eq!(u.scheme, "mailto");
        assert_eq!(u.host, None);
        assert_eq!(u.path, "user@example.com");
    }

    #[test]
    fn parse_userinfo() {
        let u = parse("ssh://user:pass@host:22/").unwrap();
        assert_eq!(u.user, Some("user"));
        assert_eq!(u.password, Some("pass"));
        assert_eq!(u.host, Some("host"));
        assert_eq!(u.port, Some(22));
    }

    #[test]
    fn parse_ipv6() {
        let u = parse("http://[::1]:8080/x").unwrap();
        assert_eq!(u.host, Some("[::1]"));
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/x");
    }

    #[test]
    fn parse_mac_like_port_greedy() {
        // The greedy last-`:` split takes the trailing octet as the port; Exp 619 repairs it.
        let u = parse("file://00:12:34:56:78:90/path").unwrap();
        assert_eq!(u.host, Some("00:12:34:56:78"));
        assert_eq!(u.port, Some(90));
        assert_eq!(u.path, "/path");
    }

    #[test]
    fn parse_invalid_port() {
        // The alphabetic MAC tail is not a number → `InvalidPort` (the fallback `os/uri` catches).
        assert_eq!(
            parse("file://12:34:56:78:90:aa/path"),
            Err(ParseError::InvalidPort)
        );
    }

    #[test]
    fn parse_missing_scheme() {
        assert_eq!(parse("example.com"), Err(ParseError::InvalidFormat));
    }

    #[test]
    fn parse_empty_authority() {
        let u = parse("file:///path").unwrap();
        assert_eq!(u.host, Some(""));
        assert_eq!(u.path, "/path");
    }

    #[test]
    fn parse_empty_path_locates_raw_start() {
        let text = "file://localhost?x#y";
        let u = parse(text).unwrap();
        assert_eq!(u.host, Some("localhost"));
        assert_eq!(u.path, "");
        assert_eq!(u.query, Some("x"));
        assert_eq!(u.fragment, Some("y"));
        // The empty path's slice pointer locates the raw-path start (where Exp 619's raw_path begins).
        let path_start = u.path.as_ptr() as usize - text.as_ptr() as usize;
        assert_eq!(&text[path_start..], "?x#y");
    }

    #[test]
    fn parse_port_overflow() {
        assert_eq!(parse("https://h:65536/"), Err(ParseError::InvalidPort));
    }

    #[test]
    fn parse_port_rejects_sign() {
        // `u16::parse` accepts a leading `+`, but an RFC-3986 port is digits only.
        assert_eq!(parse("https://h:+80/"), Err(ParseError::InvalidPort));
    }

    #[test]
    fn parse_non_empty_path_locates_raw_start() {
        let text = "file://localhost/path??#fragment";
        let u = parse(text).unwrap();
        assert_eq!(u.host, Some("localhost"));
        assert_eq!(u.path, "/path");
        // The path slice pointer locates Exp 619's raw_path start (path + query + fragment).
        let path_start = u.path.as_ptr() as usize - text.as_ptr() as usize;
        assert_eq!(&text[path_start..], "/path??#fragment");
    }
}
