+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 618: RFC-3986 URI parser (std.Uri foundation)

## Description

Per the chosen direction (**hand-port `std.Uri`** rather than depend on a Rust
URL crate — none faithfully provides verbatim component slices / MAC-address
hosts / `raw_path`), this experiment ports the RFC-3986 generic-URI parser that
`os/uri.zig` builds on. It is the foundation; Exp 619 adds `os/uri`'s
MAC-address + `raw_path` options on top.

`std.Uri` is Zig stdlib (not vendored here), so this targets its **observable
behavior** as exercised by `os/uri.zig`'s test corpus — in particular the
component boundaries and the `InvalidPort` error that `os/uri` relies on for its
MAC-address fallback. The components are borrowed slices into the input
(`&'a str`), matching `std.Uri`'s slices-into-text behavior (which `os/uri`
needs for verbatim `raw_path`).

## Observable `std.Uri.parse` behavior (from `os/uri.zig`'s tests)

A URI is `scheme ":" hier-part ["?" query] ["#" fragment]`, where
`hier-part = "//" authority path-abempty / path-…`. Key inferred behaviors:

- **Scheme**: `ALPHA *(ALPHA / DIGIT / "+" / "-" / ".")` up to the first `:`.
  Missing/invalid → `InvalidFormat`.
- **Authority** (after `//`, up to the first `/`, `?`, or `#`):
  `[userinfo "@"] host [":" port]`.
- **Port split is greedy on the LAST `:`** (the test corpus proves this):
  - `00:12:34:56:78:90` → host `00:12:34:56:78`, port `90` (then Exp 619's MAC
    repair restores the full address).
  - `00:12:34:56:78:90:999` → host `00:12:34:56:78:90`, port `999`.
  - `12:34:56:78:90:aa` → host `12:34:56:78:90`, port `aa` → **`InvalidPort`**
    (a non-numeric port; `os/uri` catches this to re-parse as a MAC host).
  - IPv6 literals are bracketed (`[::1]:8080`); the port `:` is the one after
    the `]`.
- **Port**: the chars after the (host/port) `:` must be a valid `u16` (0–65535);
  non-digit or overflow → `InvalidPort`; an empty port (`host:`) →
  `port = null`.
- **Path / query / fragment**: `path` is everything from the authority's end to
  `?`/`#`/end; `query` follows `?` (up to `#`/end); `fragment` follows `#`.
- Components are returned as verbatim slices into `text` (percent-encoding
  preserved); `port` is the parsed integer.

## Rust mapping (`roastty/src/os/uri.rs`, new file)

```rust
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

/// URI parse errors (subset of `std.Uri.ParseError` that `os/uri` distinguishes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseError {
    /// The string is not a valid URI (missing/invalid scheme).
    InvalidFormat,
    /// The port component is not a valid number (the MAC-address case `os/uri` catches).
    InvalidPort,
}

/// Parse a full URI (`scheme ":" …`) — upstream `std.Uri.parse`.
pub(crate) fn parse(text: &str) -> Result<Uri<'_>, ParseError> {
    // Scheme: ALPHA *(ALPHA/DIGIT/+/-/.) up to ':'.
    let colon = text.find(':').ok_or(ParseError::InvalidFormat)?;
    let scheme = &text[..colon];
    if !is_valid_scheme(scheme) {
        return Err(ParseError::InvalidFormat);
    }
    parse_after_scheme(scheme, &text[colon + 1..])
}

/// Parse the part after `scheme:` — upstream `std.Uri.parseAfterScheme`. `rest` is
/// `hier-part ["?" query] ["#" fragment]`.
pub(crate) fn parse_after_scheme<'a>(scheme: &'a str, rest: &'a str) -> Result<Uri<'a>, ParseError> {
    // Split fragment, then query.
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
        // authority [path]. Authority runs up to the first '/'.
        let auth_end = after.find('/').unwrap_or(after.len());
        let authority = &after[..auth_end];
        uri.path = &after[auth_end..]; // includes the leading '/', or "" if none
        parse_authority(&mut uri, authority)?;
    } else {
        // No authority: the whole hier-part is the path.
        uri.path = hier;
    }

    Ok(uri)
}
```

`parse_authority` parses `[userinfo "@"] host [":" port]`:

```rust
fn parse_authority<'a>(uri: &mut Uri<'a>, authority: &'a str) -> Result<(), ParseError> {
    // userinfo (everything before the last '@'); user[:password].
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    if let Some(ui) = userinfo {
        let (user, password) = split_once(ui, ':');
        uri.user = Some(user);
        uri.password = password;
    }

    // host [":" port]. For a bracketed IPv6 literal, the port ':' is after the ']'; otherwise it is
    // the LAST ':'.
    let port_colon = if host_port.starts_with('[') {
        host_port.find(']').and_then(|rb| host_port[rb..].find(':').map(|c| rb + c))
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
                uri.port = Some(port_str.parse::<u16>().map_err(|_| ParseError::InvalidPort)?);
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
```

Registered in `os/mod.rs` as `pub(crate) mod uri;`.

### Notes / deviations

- **Targets `std.Uri`'s observable behavior** (the Zig std source isn't
  vendored), validated against `os/uri.zig`'s test corpus. The slice-into-text
  component model + the `InvalidPort` semantics are what `os/uri` (Exp 619)
  depends on.
- Components are borrowed `&'a str` (verbatim, percent-encoding preserved),
  enabling Exp 619's verbatim `raw_path`. `port` is a parsed `u16`.
- The `raw`/`percent_encoded` `Component` distinction in `std.Uri` is dropped:
  `os/uri` only reads component strings, and the distinction matters only for
  re-formatting (unused here).
- The port split is greedy on the last `:` (proven by the MAC test corpus); a
  non-numeric port is the `InvalidPort` `os/uri` catches.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests:
  - `parse_basic_http` — `https://example.com/path?q=1#frag` → scheme `https`,
    host `example.com`, path `/path`, query `q=1`, fragment `frag`, port `None`.
  - `parse_with_port` — `https://example.com:8080/` → port `8080`.
  - `parse_no_authority` — `mailto:user@example.com` → scheme `mailto`, no host,
    path `user@example.com`.
  - `parse_userinfo` — `ssh://user:pass@host:22/` → user `user`, password
    `pass`, host `host`, port `22`.
  - `parse_ipv6` — `http://[::1]:8080/x` → host `[::1]`, port `8080`.
  - `parse_mac_like_port_greedy` — `file://00:12:34:56:78:90/path` → host
    `00:12:34:56:78`, port `90` (the greedy last-`:` split; Exp 619 repairs it).
  - `parse_invalid_port` — `file://12:34:56:78:90:aa/path` → `Err(InvalidPort)`.
  - `parse_missing_scheme` — `example.com` (no `:`) → `Err(InvalidFormat)`.
  - `parse_empty_authority` (adopted Optional) — `file:///path` → host
    `Some("")`, path `/path`.
  - `parse_empty_path_locates_raw_start` (adopted Optional) —
    `file://localhost?x#y` → host `localhost`, an EMPTY path whose slice pointer
    sits right before `?` (so Exp 619's `raw_path` from the path start is
    `?x#y`), query `x`, fragment `y`.
  - `parse_port_overflow` (adopted Optional) — `https://h:65536/` →
    `Err(InvalidPort)`.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = the RFC-3986 parser splits a URI into verbatim component slices matching
`std.Uri`'s observable behavior (including the greedy port split and the
`InvalidPort` error), as the foundation for `os/uri` (Exp 619).

## Design Review

Codex reviewed the design and **APPROVED** it with **no Required findings**,
confirming: the greedy-last-`:` port split is the right inferred behavior for
the MAC-address corpus AND normal `host:port` cases (numeric MAC tail → port for
Exp 619's repair; alphabetic tail → `InvalidPort` for the fallback; bracketed
IPv6 uses the colon after `]`); the component splitting is sound (fragment →
query → authority/path; authority terminates before `/`/`?`/`#`; path keeps its
leading `/` or is empty); `rfind('@')` userinfo + `user[:password]` are
faithful; and the borrowed-slice `Uri<'a>` model is the right `raw_path`
foundation (dropping the raw/percent-encoded `Component` distinction is fine for
verbatim reads). Three Optional test additions adopted:

- `file:///path` (empty authority → `host = Some("")`, path `/path`).
- `file://localhost?x#y` (empty path whose slice pointer locates the raw-path
  start for Exp 619).
- `:65536` port overflow → `InvalidPort`.

Review artifacts:

- Prompt: `logs/codex-review/20260605-d618-prompt.md`
- Result: `logs/codex-review/20260605-d618-last-message.md`

## Result

**Result:** Pass

Implemented `roastty/src/os/uri.rs` (registered `pub(crate) mod uri;`),
hand-porting the RFC-3986 generic URI parser `os/uri` builds on: `Uri<'a>`
(borrowed verbatim component slices),
`ParseError { InvalidFormat, InvalidPort }`, `parse` / `parse_after_scheme`
(fragment → query → authority/path), and `parse_authority` (userinfo via
`rfind('@')`; host/port via the greedy last `:` — the `:` after `]` for
bracketed IPv6 — with digit-only/overflow → `InvalidPort` and empty port →
`None`). The Required completion-review fix is in: the port must be all ASCII
digits before `u16::parse` (rejecting the leading `+` Rust would otherwise
accept).

Thirteen tests cover the corpus: basic http, with-port, no-authority (mailto),
userinfo, IPv6, the greedy MAC-like split (→ Exp 619 repair target), invalid /
sign / overflow ports → `InvalidPort`, missing scheme → `InvalidFormat`, empty
authority (`file:///path`), and the empty/non-empty path raw-start pointer (for
Exp 619's `raw_path`). Gates: `cargo fmt --check` clean,
`cargo build -p roastty` no warnings, `cargo test -p roastty` **3407 passed / 0
failed** (3394 → 3407, +13), no-ghostty grep clean, `git diff --check` clean.

## Completion Review

Codex's first completion review raised **one Required** finding — `u16::parse`
accepts a leading `+`, but an RFC-3986 port is digits-only (`https://h:+80/`
should be `InvalidPort`). Fixed with a digit-only guard + a
`parse_port_rejects_sign` test, and the adopted non-empty raw-start test. Codex
**re-confirmed APPROVED**, noting the parser matches the observable `std.Uri`
behavior `os/uri` needs (greedy last-colon split, IPv6 brackets,
digit-only/overflow `InvalidPort`, verbatim borrowed slices, correct component
boundaries).

Review artifacts:

- Design prompt/result:
  `logs/codex-review/20260605-d618-{prompt,last-message}.md`
- Result prompt/result:
  `logs/codex-review/20260605-r618-{prompt,last-message}.md`
- Re-confirmation: `logs/codex-review/20260605-r618b-{prompt,last-message}.md`

## Conclusion

The RFC-3986 URI parser foundation is in place (per the chosen hand-port
direction). **Exp 619** layers `os/uri`'s options on top: the `mac_address`
fallback (re-parse a MAC-host URI on `InvalidPort`) + repair heuristic (the
14-char/4-colon/port-≤99 case), the `raw_path` option (the path slice's offset →
`text[path_start..]`, clearing query/fragment), and `is_valid_mac_address` —
completing the URI area. Issue 801 stays open and broad.
