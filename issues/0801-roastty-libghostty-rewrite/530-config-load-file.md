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

# Experiment 530: config file IO (Config::load_file)

## Description

With the end-to-end string loader (`Config::load_str`, Experiment 529), this
experiment adds **file IO** — `Config::load_file` — reading a config-file path
into `load_str`, the roastty analog of upstream `Config.loadFile` →
`loadReader`. It reads the file to a string, skips a leading UTF-8 byte-order
mark, and drives `load_str`.

## Upstream behavior

`loadFile` → `loadFsFile` → `loadReader` (`Config.zig:3897`–`3926`):

```zig
pub fn loadFile(self, alloc, path) !void {
    assert(std.fs.path.isAbsolute(path));
    var file = file_load.open(path) catch |err| switch (err) {
        error.NotAFile => { log.warn("config-file {s}: not reading because it is not a file"); return; },
        else => return err,
    };
    defer file.close();
    try self.loadFsFile(alloc, &file, path);   // → loadReader
}

fn loadReader(self, alloc, reader, path) !void {
    bom: {  // skip a leading UTF-8 BOM (EF BB BF)
        const bom = &.{ 0xef, 0xbb, 0xbf };
        const str = reader.peek(bom.len) catch break :bom;
        if (std.mem.eql(u8, str, bom)) { reader.toss(bom.len); }
    }
    var iter = cli.args.LineIterator{ .r = reader, .filepath = path };
    try self.loadIter(alloc, &iter);                 // → the line driver (load_str)
    try self.expandPaths(std.fs.path.dirname(path).?); // resolve relative Path fields
}
```

So `loadFile`:

- the path must be absolute; a path that opens but is **not a file** logs a
  warning and returns (no error); other open errors propagate.
- a leading UTF-8 **BOM** (`EF BB BF`) is skipped.
- the file's lines drive the loader (`load_str`'s line iteration).
- `expandPaths` resolves relative `Path`-typed fields against the config
  directory.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl Config {
    /// Load config from a file (upstream `Config.loadFile` → `loadReader`): read the
    /// file, skip a leading UTF-8 byte-order mark, and drive `load_str`. Returns the
    /// per-line diagnostics; an open/read error propagates as `io::Error`.
    pub(crate) fn load_file(&mut self, path: &std::path::Path) -> std::io::Result<Vec<ConfigDiagnostic>> {
        let text = std::fs::read_to_string(path)?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
        Ok(self.load_str(text))
    }
}
```

`std::fs::read_to_string` reads the file as UTF-8; a leading UTF-8 BOM decodes
to `'\u{FEFF}'`, stripped with `strip_prefix`. `load_str` does the line
iteration and diagnostics. An open/read failure is an `io::Error`.

## Scope / faithfulness notes

- **Ported (bridged)**: `Config.loadFile` → `loadReader`'s file read + BOM
  skip + line-driver dispatch, as `Config::load_file`.
- **Faithful**: read the file, skip a leading UTF-8 BOM, then drive the line
  loader (`load_str`).
- **Faithful adaptation**: the streaming `std.Io.Reader` + `LineIterator`
  (2048-byte read buffer, 4096-byte `MAX_LINE_SIZE` per line) →
  `read_to_string` + `load_str` over the in-memory string; upstream's
  `peek`/`toss` BOM skip → `strip_prefix`; open/read errors → `io::Error`.
- **Documented narrowings / N-A**:
  - The streaming `MAX_LINE_SIZE = 4096` per-line truncation is not modeled (the
    file is read whole); only pathological >4096-byte lines would differ.
  - The absolute-path `assert` and the `NotAFile` "warn and continue"
    special-case are not replicated — an open/read error is returned as
    `io::Error` for the caller to handle (roastty has no logging diagnostics
    layer here yet).
  - `expandPaths` is not applicable — roastty's `Config` has no `Path`-typed
    fields.
  - `read_to_string` requires the whole file to be valid UTF-8 and errors
    otherwise; upstream reads bytes from a `Reader` and only interprets them as
    needed, so a non-UTF-8 file is not rejected at the read boundary. Acceptable
    since roastty's loader/parser APIs are `&str` / `String`.
- **Deferred**: the default config-path resolution (`loadDefaultFiles`); the
  `--key=value` CLI-arg form; the logging/diagnostics-reporting layer.
  `background-image-opacity` stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `Config::load_file`.
2. Tests (in `config/mod.rs`): write a config to a temp file and `load_file` it
   — the fields apply (verified via `format_config`) with no diagnostics; a file
   with a leading UTF-8 BOM still parses (BOM skipped); a file with a bad line
   yields the expected diagnostic; a nonexistent path returns an `io::Error`.
   (The temp file is created under `std::env::temp_dir()` with a process-unique
   name and removed after.)
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_load_file
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config::load_file` reads the file, skips a leading UTF-8 BOM, and drives
  `load_str`, returning its diagnostics; an open/read error is an `io::Error`;
- the tests pass (a clean file load, a BOM file, a file with a bad line, a
  missing path), and the existing tests still pass;
- the default-path resolution and CLI form stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the file load diverges from upstream (esp. not
skipping the BOM, or not driving the same line loader), an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (folded into the narrowings): document that `read_to_string`
requires the whole file to be valid UTF-8 and errors otherwise, whereas upstream
reads bytes from a `Reader` and only interprets them as needed, so a non-UTF-8
file is not rejected at the read boundary (`Config.zig:3915`) — acceptable since
roastty's loader/parser APIs are `&str` / `String`.

Codex confirmed everything else is faithful: `read_to_string +
strip_prefix('\u{FEFF}')

- load_str`is the right Rust adaptation of opening the file, peeking/tossing the UTF-8 BOM, then driving`LineIterator` (`Config.zig:3897`/`:3925`); a UTF-8 BOM decodes to a leading `U+FEFF`, so `strip_prefix('\u{FEFF}')`is correct; the documented differences (no streaming`MAX_LINE_SIZE`, no absolute-path assert / `NotAFile`warn-and-return, no`expandPaths`) are acceptable for this slice; and returning `io::Result<Vec<ConfigDiagnostic>>`
  is a reasonable Rust shape (IO failure as the outer error, parse issues as
  diagnostics on a successful read).

Review artifacts:

- Prompt: `logs/codex-review/20260604-190530-d530-prompt.md` (design)
- Result: `logs/codex-review/20260604-190530-d530-last-message.md` (design)
