+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 692: `web file` Subcommand

Add a `web file <path>` subcommand that opens local files in the browser pane.
`web file index.html` resolves the path to an absolute `file:///` URL and
navigates to it. No more typing `file:///Users/ryan/dev/project/index.html` by
hand.

## Background

The browser already supports `file:///` URLs — you can type
`web file:///absolute/path/to/file.html` and it works. But this is painful:

- You have to know the absolute path
- You have to type the `file:///` prefix
- Relative paths don't work
- Tab completion doesn't help

`web file index.html` should just work. The TUI resolves the relative path
against `$PWD`, canonicalizes it, and converts it to a `file:///` URL.

## Design

### New subcommand: `web file <path>`

Add a `File` variant to the `Commands` enum in `main.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    Url { url: String },
    Last,
    Status,
    /// Open a local file in the browser pane
    File {
        /// Path to the file (relative or absolute)
        path: String,
    },
}
```

### Path resolution

In the URL resolution block (line 269), handle the new subcommand:

```rust
Some(Commands::File { path }) => {
    let absolute = std::fs::canonicalize(&path).unwrap_or_else(|e| {
        eprintln!("Error: {}: {}", path, e);
        std::process::exit(1);
    });
    format!("file://{}", absolute.display())
}
```

`std::fs::canonicalize` resolves `.`, `..`, symlinks, and relative paths against
the current working directory. If the file doesn't exist, it returns an error —
which is the right behavior (no point navigating to a nonexistent file).

### Also support `file` in the Edit mode URL bar

When the user types a path into the URL bar (Edit mode) and presses Enter,
`normalize_url` currently prepends `https://` if it sees a dot. A bare filename
like `index.html` would become `https://index.html`.

Add file path detection to `normalize_url`:

```rust
fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    // Check if it looks like a file path (starts with / or ./ or ../).
    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        if let Ok(absolute) = std::fs::canonicalize(trimmed) {
            return format!("file://{}", absolute.display());
        }
    }
    // ... existing localhost/https logic ...
}
```

This only triggers for paths that are unambiguously file paths (absolute or
explicitly relative). A bare `index.html` in the URL bar still goes to
`https://index.html` — use `./index.html` to be explicit.

## Test

1. `web file index.html` — opens `file:///absolute/path/to/index.html`
2. `web file ./src/page.html` — relative path with `./`
3. `web file ../other/file.html` — relative path with `../`
4. `web file /tmp/test.html` — absolute path
5. `web file nonexistent.html` — prints error and exits
6. In Edit mode, type `./index.html` and press Enter — navigates to file URL
7. In Edit mode, type `/tmp/test.html` and press Enter — navigates to file URL
8. In Edit mode, type `index.html` and press Enter — still goes to
   `https://index.html` (no ambiguity)
9. `web file:///tmp/test.html` — existing behavior still works (not broken)

## Experiment 1: Add `File` subcommand and `normalize_url` file detection

### Hypothesis

If we add a `File` variant to the `Commands` enum, resolve the path with
`std::fs::canonicalize` in the URL matching block, and add file path detection
to `normalize_url`, then `web file index.html` opens local files and Edit mode
paths like `./index.html` resolve correctly — all in one file (`main.rs`).

### Changes

Three changes in `tui/src/main.rs`. No other files.

#### 1. Add `File` variant to `Commands` enum (line 192)

After `Status`:

```rust
/// Open a local file in the browser pane
File {
    /// Path to the file (relative or absolute)
    path: String,
},
```

#### 2. Handle `File` in the URL resolution match (line 269)

Add a new arm before `None`:

```rust
let raw_url = match cli.command {
    Some(Commands::Url { url }) => url,
    Some(Commands::File { path }) => {
        let absolute = std::fs::canonicalize(&path).unwrap_or_else(|e| {
            eprintln!("Error: {}: {}", path, e);
            std::process::exit(1);
        });
        format!("file://{}", absolute.display())
    }
    Some(Commands::Last) | Some(Commands::Status) => unreachable!(),
    None => cli.url.unwrap_or_else(|| {
        hello_homepage.unwrap_or_else(|| "https://termsurf.com/welcome".to_string())
    }),
};
```

`canonicalize` resolves relative paths against `$PWD`, follows symlinks, and
errors if the file doesn't exist.

#### 3. Add file path detection to `normalize_url` (line 694)

After the `://` check, before the localhost check:

```rust
// File paths: absolute or explicitly relative (Issue 692).
if trimmed.starts_with('/')
    || trimmed.starts_with("./")
    || trimmed.starts_with("../")
{
    if let Ok(absolute) = std::fs::canonicalize(trimmed) {
        return format!("file://{}", absolute.display());
    }
}
```

This handles Edit mode — typing `./index.html` or `/tmp/test.html` in the URL
bar resolves to a `file:///` URL. Bare filenames like `index.html` are not
matched (ambiguous with hostnames), so the user types `./index.html` to be
explicit. If `canonicalize` fails (file doesn't exist), it falls through to the
existing URL logic.

### Test

Same as the issue-level test plan:

1. `web file index.html` — opens `file:///absolute/path/to/index.html`
2. `web file ./src/page.html` — relative path with `./`
3. `web file ../other/file.html` — parent-relative path
4. `web file /tmp/test.html` — absolute path
5. `web file nonexistent.html` — prints error and exits
6. Edit mode: `./index.html` → navigates to file URL
7. Edit mode: `/tmp/test.html` → navigates to file URL
8. Edit mode: `index.html` → still goes to `https://index.html`
9. `web file:///tmp/test.html` → existing behavior unchanged

### Result: SUCCESS

All three changes work as designed. `web file index.html` resolves to
`file:///absolute/path/to/index.html` and opens in the browser. Edit mode paths
starting with `/`, `./`, or `../` resolve correctly. Bare filenames fall through
to the existing URL logic.

## Conclusion

`web file <path>` opens local files in the browser pane. Three changes in
`tui/src/main.rs`:

1. **`File` subcommand**: New `Commands::File { path }` variant parsed by clap.
2. **Path resolution**: `std::fs::canonicalize` resolves relative paths against
   `$PWD`, errors on nonexistent files, and formats as `file://` URL.
3. **Edit mode**: `normalize_url` detects paths starting with `/`, `./`, or
   `../` and resolves them to `file://` URLs. Bare filenames are left as URLs to
   avoid ambiguity.
