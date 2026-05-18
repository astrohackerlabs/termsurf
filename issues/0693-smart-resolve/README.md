+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 693: Smart Input Resolution

Make `web [whatever]` intelligently resolve its argument as a subcommand, URL,
or file path — so users never have to think about which mode they're in.

## Background

After Issue 692, we have three ways to open things:

- `web url google.com` — always a URL
- `web file index.html` — always a file
- `web google.com` — goes through `normalize_url`, which guesses

The explicit subcommands are escape hatches. The bare `web [whatever]` form is
what people will actually type 99% of the time. It needs to be smart enough that
the escape hatches are rarely needed.

Currently `web index.html` (no subcommand) runs `normalize_url`, which sees a
dot and returns `https://index.html`. That's wrong — the user almost certainly
meant the local file. The smart resolver fixes this.

## The Algorithm

When the user types `web [input]` (no subcommand), resolve in this order:

```
1. Has a scheme (contains "://")     → URL as-is
2. Is "devtools" or "devtools://..."  → DevTools (existing logic)
3. Starts with "/", "./", or "../"    → File path (error if not found)
4. Contains ":" (host:port pattern)   → URL (normalize)
5. File exists at that path           → File
6. Contains a dot (looks like a URL)  → URL (normalize)
7. Nothing matched                    → Error
```

### Why this order

**Step 1 — Scheme wins.** `https://google.com`, `file:///tmp/test.html`,
`devtools://42` — if you typed a scheme, you know what you want. No guessing.

**Step 2 — DevTools.** Bare `devtools` is a special case already handled by
existing code. Keep it where it is.

**Step 3 — Explicit paths.** `/tmp/test.html`, `./index.html`, `../page.html` —
these are unambiguously file paths. If the file doesn't exist, that's an error,
not a URL fallback. Nobody types `./index.html` meaning a website.

**Step 4 — Colon means port.** `localhost:3000`, `192.168.1.1:8080` — a colon in
a bare input (no scheme) almost always means host:port. While colons are
technically valid in filenames on macOS/Linux, nobody names files
`localhost:3000`. This check runs before the file-exists check so that
`localhost:3000` doesn't accidentally match a (very unlikely) file named
`localhost:3000`.

**Step 5 — File exists.** `index.html`, `README.md`, `dist/bundle.js` — if a
file exists at the given path relative to `$PWD`, open it. This is the key
insight: if the user is in a project directory and types `web index.html`, they
mean the file. This step only triggers if the file actually exists — no false
positives.

**Step 6 — URL fallback (with dot).** `google.com`, `github.com/anthropics` — if
it contains a dot, it's probably a URL. Prefix with `https://` (or `http://` for
localhost).

**Step 7 — Error.** `totalnonsense`, `asdf`, `foobar` — no scheme, no file, no
dot. This is not a URL, not a file, not a command. Print an error and exit. If
the user really means it, they can use `web url totalnonsense` to force URL
treatment.

### Edge cases

| Input                | Step | Result                                    |
| -------------------- | ---- | ----------------------------------------- |
| `https://x.com`      | 1    | `https://x.com`                           |
| `file:///tmp/a.html` | 1    | `file:///tmp/a.html`                      |
| `devtools`           | 2    | DevTools auto-target                      |
| `devtools://42`      | 2    | DevTools for tab 42                       |
| `./index.html`       | 3    | `file:///abs/path/index.html`             |
| `/tmp/test.html`     | 3    | `file:///tmp/test.html`                   |
| `./nonexistent`      | 3    | Error: file not found                     |
| `localhost:3000`     | 4    | `http://localhost:3000`                   |
| `192.168.1.1:8080`   | 4    | `https://192.168.1.1:8080`                |
| `index.html`         | 5    | `file:///abs/path/index.html` (if exists) |
| `index.html`         | 6    | `https://index.html` (if not exists)      |
| `google.com`         | 6    | `https://google.com`                      |
| `totalnonsense`      | 7    | Error: not a URL, file, or command        |
| `asdf`               | 7    | Error                                     |

### What about `normalize_url`?

`normalize_url` is used in two places: the smart resolver (step 6) and the Edit
mode URL bar. After this issue, both paths share the same logic — the smart
resolver IS `normalize_url`, upgraded.

## Design

Replace `normalize_url` with `resolve_input` in `main.rs`. One function, one
algorithm, used everywhere.

```rust
/// Resolve bare input to a URL or file:// path (Issue 693).
fn resolve_input(input: &str) -> Option<String> {
    let trimmed = input.trim();

    // Step 1: Has a scheme — use as-is.
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }

    // Step 3: Explicit file paths (/, ./, ../) — error on not found
    // is handled by the caller for the CLI path; in Edit mode, fall
    // through silently.
    if trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
    {
        if let Ok(absolute) = std::fs::canonicalize(trimmed) {
            return Some(format!("file://{}", absolute.display()));
        }
    }

    // Step 4: Contains ":" — treat as host:port URL.
    if trimmed.contains(':') {
        let host = trimmed.split(':').next().unwrap_or(trimmed);
        if host.ends_with("localhost") || host.contains("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 5: File exists — open as file.
    if let Ok(absolute) = std::fs::canonicalize(trimmed) {
        return Some(format!("file://{}", absolute.display()));
    }

    // Step 6: URL fallback (has a dot — looks like a domain).
    if trimmed.contains('.') {
        let host = trimmed.split('/').next().unwrap_or(trimmed);
        if host.ends_with("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 7: Nothing matched — not a URL, file, or command.
    // Return None so the caller can print an error.
    None
}
```

The return type changes to `Option<String>`. Callers handle `None`:

- **CLI path**: print an error and exit.
- **Edit mode URL bar**: show red error in command bar (same as invalid
  `:devtools` direction).

Then update the `None` arm in the URL match block to use `resolve_input` instead
of relying on `normalize_url` later:

```rust
None => {
    let input = cli.url.unwrap_or_else(|| {
        hello_homepage.unwrap_or_else(|| "https://termsurf.com/welcome".to_string())
    });
    resolve_input(&input)
}
```

And remove the separate `normalize_url(&raw_url)` call after the match — the
resolution is already done.

Step 2 (DevTools) is already handled by the existing `devtools://` / bare
`devtools` checks after the match block. Those stay as-is.

## Experiment 1: Replace `normalize_url` with `resolve_input`

### Hypothesis

If we replace `normalize_url` with `resolve_input` (returning `Option<String>`),
the smart 7-step algorithm handles all inputs correctly: files are opened when
they exist, URLs are normalized when they look like URLs, and garbage inputs
produce clear errors instead of silently navigating to nonsense.

### Changes

All in `tui/src/main.rs`.

#### 1. Replace `normalize_url` with `resolve_input`

Delete `normalize_url` (line 705) and replace with `resolve_input` returning
`Option<String>`, implementing the 7-step algorithm from the design section.

#### 2. Update CLI path (line 274–300)

The `None` arm of the match already produces `raw_url`. The devtools check
(step 2) happens after the match at line 288. The `normalize_url` call is at
line 299. Replace that call:

```rust
let mut url = if is_devtools {
    raw_url
} else {
    match resolve_input(&raw_url) {
        Some(resolved) => resolved,
        None => {
            eprintln!("Error: '{}' is not a URL, file, or command", raw_url);
            std::process::exit(1);
        }
    }
};
```

#### 3. Update Edit mode path (line 550)

Replace `normalize_url(&new_url)` with `resolve_input` and handle `None`:

```rust
match resolve_input(&new_url) {
    Some(resolved) => {
        url = resolved;
        editor_url = url.clone();
        mode = Mode::Browse;
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_navigate(pid, &url);
            conn.send_mode_changed(pid, true);
        }
    }
    None => {
        command_error = Some(format!(
            "'{}' is not a URL or file", new_url
        ));
        mode = Mode::Command;
    }
}
```

On `None`, show the error in the command bar (red border, error text) and switch
to Command mode so the user sees the feedback. The existing `command_error`
infrastructure from Issue 690 handles the display.

### Test

Same as the issue-level test plan:

1. `web google.com` → `https://google.com`
2. `web localhost:3000` → `http://localhost:3000`
3. `web index.html` (file exists) → `file:///abs/path/index.html`
4. `web index.html` (file doesn't exist) → `https://index.html`
5. `web ./index.html` (file exists) → `file:///abs/path/index.html`
6. `web ./nonexistent.html` → falls through (no error in smart mode)
7. `web https://example.com` → `https://example.com`
8. `web file:///tmp/test.html` → `file:///tmp/test.html`
9. `web devtools` → DevTools auto-target (unchanged)
10. `web totalnonsense` → error: not a URL, file, or command
11. `web asdf` → error
12. `web url totalnonsense` → navigates (explicit override)
13. Edit mode: type `index.html` in URL bar (file exists) → file URL
14. Edit mode: type `google.com` → `https://google.com`
15. Edit mode: type `localhost:3000` → `http://localhost:3000`
16. Edit mode: type `asdf` → red error in command bar
17. `web url google.com` → `https://google.com` (explicit, unchanged)
18. `web file index.html` → file (explicit, unchanged)

### Result: SUCCESS

The 7-step algorithm works as designed. Files are opened when they exist, URLs
are normalized when they look like URLs, and garbage inputs produce clear
errors.

## Conclusion

`web [whatever]` now uses a smart 7-step resolver (`resolve_input`) that
replaces the old `normalize_url`. The resolution order:

1. Has `://` → URL as-is
2. `devtools` → DevTools (existing, unchanged)
3. Starts with `/`, `./`, `../` → file path
4. Contains `:` → URL (host:port)
5. File exists at path → file
6. Contains a dot → URL (normalize with `https://`)
7. Nothing matched → error

`resolve_input` returns `Option<String>`. `None` produces a clear error at the
CLI (`exit(1)`) or a red command bar in Edit mode. The explicit subcommands
`web url` and `web file` bypass the resolver entirely as escape hatches.
