+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 676: URL Normalization

Automatically prepend `https://` when the user types a bare domain like
`google.com` instead of `https://google.com`.

## Background

Currently the TUI passes URLs verbatim to Chromium. If the user types
`web google.com` or enters `google.com` in the URL bar, Chromium receives
`google.com` as-is, which may fail or trigger a search instead of navigation.

Users expect browser-like behavior: type `google.com`, get `https://google.com`.

## Experiment 1: Prepend https:// for bare domains

### Hypothesis

A simple `normalize_url` function in the TUI that detects bare domains (contains
a dot, no scheme) and prepends `https://` will give browser-like URL entry.

### Changes

#### 1. main.rs — add `normalize_url` function

```rust
fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    // Extract the host portion (before any path/query).
    let host = trimmed.split('/').next().unwrap_or(trimmed);
    if host.ends_with("localhost") || host.contains("localhost:") {
        return format!("http://{trimmed}");
    }
    if trimmed.contains('.') {
        return format!("https://{trimmed}");
    }
    trimmed.to_string()
}
```

The heuristic:

- Already has a scheme (`://`) — pass through (`https://google.com`,
  `http://localhost:3000`, `file:///tmp/foo.html`)
- Host is or contains `localhost` — prepend `http://` (`localhost:3000`,
  `myapp.localhost:3000`, `localhost/path`)
- Contains a dot — treat as domain, prepend `https://` (`google.com` →
  `https://google.com`)
- No dot, no scheme — pass through as-is (could be a search query in the future)

#### 2. main.rs — apply at both URL entry points

**CLI resolution** (after the `url` match):

```rust
let mut url = normalize_url(&url);
```

**Editor submit** (Edit mode, Enter key):

```rust
url = normalize_url(&new_url);
```

### Test

1. `cd tui && cargo build` — compiles without errors
2. `web google.com` → navigates to `https://google.com`
3. `web https://google.com` → still works (scheme preserved)
4. `web http://localhost:3000` → still works (http preserved)
5. `web localhost:3000` → `http://localhost:3000`
6. `web localhost` → `http://localhost`
7. `web myapp.localhost:3000` → `http://myapp.localhost:3000`
8. `web file:///tmp/test.html` → still works (file scheme preserved)
9. Type `github.com` in URL bar, press Enter → `https://github.com`
10. `web` (no args) → homepage unchanged (already has scheme)

### Result: PASS

Build succeeds. Bare domains get `https://`, localhost variants get `http://`,
explicit schemes are preserved. Works from both CLI args and the URL bar editor.

## Conclusion

`web` now normalizes URLs automatically. Type `google.com` and get
`https://google.com`. Type `localhost:3000` or `myapp.localhost:3000` and get
`http://`. Explicit schemes are never touched.

Single file changed: `tui/src/main.rs` — `normalize_url` function applied at
both URL entry points (CLI resolution and editor submit).
