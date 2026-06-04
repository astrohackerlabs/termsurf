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

# Experiment 489: the config WorkingDirectory CLI parser (WorkingDirectory::parse_cli)

## Description

This experiment ports `WorkingDirectory` (upstream `Config.WorkingDirectory`) —
the `working-directory` config: either a keyword (`home` / `inherit`) or an
explicit path (optionally wrapped in double quotes). Its parser trims
whitespace, strips a surrounding pair of quotes, and matches the two keywords or
falls back to a path. The `finalize` tilde/home expansion (OS-dependent) and the
`formatEntry` formatter stay deferred. (`parseFloat`-needing config types remain
deferred — Zig's `std.fmt.parseFloat` is a full Eisel-Lemire parser, too large
to port faithfully in one slice.)

## Upstream behavior

In `config/Config.zig`, `Config.WorkingDirectory`:

```zig
pub const WorkingDirectory = union(enum) {
    home,
    inherit,
    path: []const u8,

    pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void {
        var input = input_ orelse return error.ValueRequired;
        input = std.mem.trim(u8, input, &std.ascii.whitespace);
        if (input.len == 0) return error.ValueRequired;

        // Match path.zig behavior for quoted values.
        if (input.len >= 2 and input[0] == '"' and input[input.len - 1] == '"') {
            input = input[1 .. input.len - 1];
        }

        if (std.mem.eql(u8, input, "home")) { self.* = .home; return; }
        if (std.mem.eql(u8, input, "inherit")) { self.* = .inherit; return; }

        self.* = .{ .path = try alloc.dupe(u8, input) };
    }

    pub fn value(self: Self) ?[]const u8 {
        return switch (self) {
            .path => |path| path,
            .home, .inherit => null,
        };
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- The input is trimmed of the full Zig whitespace set (`std.ascii.whitespace` =
  space, `\t`, `\n`, `\r`, vertical tab `0x0B`, form feed `0x0C`); an empty
  result is `error.ValueRequired`.
- If the trimmed input is at least two bytes and starts **and** ends with `"`,
  the surrounding quotes are stripped.
- `home` / `inherit` keywords yield the corresponding variant; anything else is
  a `path`.
- `value` returns the path for `.path`, else `null`.

Upstream's test: `"inherit"` → `inherit`; `"home"` → `home`; `"~/projects/…"` →
`path "~/projects/…"`; `"\"/tmp path\""` → `path "/tmp path"` (quotes stripped).
The `working-directory` config field is `?WorkingDirectory = null`, so the union
itself has no inherent default.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing `WorkingDirectory` (upstream `error.ValueRequired`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkingDirectoryParseError {
    /// No value, or an empty/all-whitespace value (upstream `error.ValueRequired`).
    ValueRequired,
}

/// The `working-directory` config (upstream `Config.WorkingDirectory`): a keyword
/// (`home` / `inherit`) or an explicit path. The `finalize` (tilde expansion) and
/// `formatEntry` are ported later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkingDirectory {
    Home,
    Inherit,
    Path(String),
}

impl WorkingDirectory {
    /// Parse the `working-directory` value (upstream `parseCLI`): trim whitespace,
    /// strip a surrounding pair of quotes, then match `home` / `inherit` or fall
    /// back to a `Path`. A missing or empty/all-whitespace value is `ValueRequired`.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), WorkingDirectoryParseError> {
        let input = input.ok_or(WorkingDirectoryParseError::ValueRequired)?;
        let input = input.trim_matches(|c: char| c.is_ascii() && is_ascii_ws_zig(c as u8));
        if input.is_empty() {
            return Err(WorkingDirectoryParseError::ValueRequired);
        }

        // Match the path quoting behavior: strip a surrounding pair of quotes.
        let input = if input.len() >= 2 && input.starts_with('"') && input.ends_with('"') {
            &input[1..input.len() - 1]
        } else {
            input
        };

        *self = match input {
            "home" => WorkingDirectory::Home,
            "inherit" => WorkingDirectory::Inherit,
            other => WorkingDirectory::Path(other.to_string()),
        };
        Ok(())
    }

    /// The explicit path, if any (upstream `value`): `Some` for `Path`, else `None`.
    pub(crate) fn value(&self) -> Option<&str> {
        match self {
            WorkingDirectory::Path(path) => Some(path),
            WorkingDirectory::Home | WorkingDirectory::Inherit => None,
        }
    }
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the full-whitespace
trim (reusing `is_ascii_ws_zig`, the `std.ascii.whitespace` set ported in
Experiment 480) with the empty-after-trim `ValueRequired`, the `>= 2` +
both-ends quote strip, and the `home` / `inherit` keyword match or `Path`
fallback. `value` mirrors upstream's `value`. `Clone` / `PartialEq` / `Eq` are
derived (full comparison, matching upstream's `clone` / value semantics).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `WorkingDirectory` enum,
  `WorkingDirectory::parse_cli` (upstream `parseCLI`), and `value`, plus
  `WorkingDirectoryParseError`.
- **Faithful**: the `ValueRequired` guard; the full Zig-whitespace trim and the
  empty-after-trim `ValueRequired`; the `len >= 2` + starts-and-ends-with-`"`
  quote strip; the `home` / `inherit` keyword match; the `Path` fallback; the
  `value` accessor — exactly upstream's `parseCLI` / `value`.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; `union(enum)` →
  `WorkingDirectory`; the arena-`dupe` path → an owned `String`;
  `std.mem.trim(_, std.ascii.whitespace)` → `trim_matches` over the reused
  `is_ascii_ws_zig`; the one upstream error → `WorkingDirectoryParseError`. The
  `union` has no inherent default (the field is `?WorkingDirectory`), so no
  `Default` is derived.
- **Deferred**: `WorkingDirectory.finalize` (expands a leading `~/` via the OS
  home directory; OS-dependent, no consumer yet) and
  `WorkingDirectory.formatEntry` (writes the keyword `@tagName` or the path;
  depends on the not-yet-ported config `EntryFormatter`), and the broader config
  parser/formatter. `clone` is covered by the derive. (Consumed by later slices;
  this experiment lands the parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add
   `WorkingDirectoryParseError { ValueRequired }`, the `WorkingDirectory` enum
   (`Home` / `Inherit` / `Path(String)`, `derive(Debug, Clone, PartialEq, Eq)`),
   `parse_cli`, and `value`.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` test (with a `ghostty`-free path): `"inherit"`
     → `Inherit`; `"home"` → `Home`; `"~/projects/app"` →
     `Path("~/projects/app")`; `"\"/tmp path\""` → `Path("/tmp path")` (quotes
     stripped).
   - whitespace: `" home "` → `Home`; a plain path `"/usr/local"` →
     `Path("/usr/local")`.
   - errors: `None` → `ValueRequired`; `""` → `ValueRequired`; `"   "` →
     `ValueRequired`.
   - `value`: `Inherit.value()` is `None`; `Path("x").value()` is `Some("x")`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty working_directory
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `WorkingDirectory::parse_cli` trims whitespace, strips a surrounding quote
  pair, and matches `home` / `inherit` or falls back to a `Path`, returning
  `ValueRequired` on a missing/empty value — faithful to upstream's `parseCLI`;
  `value` returns the path or `None`;
- the tests pass (the upstream cases; the whitespace/plain-path/error/`value`
  cases), and the existing tests still pass;
- `finalize` / `formatEntry` and the broader config parser/formatter stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a value is parsed wrong (wrong trim, wrong quote
strip, wrong keyword/path resolution), a missing/empty value does not error, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: `None` and an
empty-after-whitespace-trim both return `ValueRequired`, quotes are stripped
only when both ends are `"` and the length is `>= 2`, `home` / `inherit` are
exact keyword matches after the quote handling, and everything else becomes
`Path(String)` (`Config.zig:5302`/`:5307`); reusing `is_ascii_ws_zig` is the
right equivalent for `std.ascii.whitespace`, and `trim_matches` trims both ends
like `std.mem.trim`; the absence of an inherent `Default` is correct since the
field is `?WorkingDirectory = null` (`:1537`); deriving `Clone`/`PartialEq`/`Eq`
is faithful for this union shape; and deferring `finalize` + `formatEntry` is
the right scope.

Review artifacts:

- Prompt: `logs/codex-review/20260604-145344-d489-prompt.md` (design)
- Result: `logs/codex-review/20260604-145344-d489-last-message.md` (design)

## Result

**Result:** Pass

`WorkingDirectory` was implemented exactly as designed — the enum (`Home` /
`Inherit` / `Path(String)`), `parse_cli` (the `ValueRequired` guard, the full
Zig-whitespace trim via `is_ascii_ws_zig` with the empty-after-trim
`ValueRequired`, the `>= 2` + both-ends quote strip, and the `home` / `inherit`
keyword match or `Path` fallback), and `value`. The new test
`working_directory_parse_cli_parses_keywords_and_paths` covers the upstream
cases (a quoted path has its quotes stripped), whitespace, a plain path, the
missing/empty errors, and `value`.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2973 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean (a test comment was reworded off the literal name); `git diff --check`
  clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `WorkingDirectory.parseCLI`
(required input, full ASCII-whitespace trim, empty-after-trim → `ValueRequired`,
paired quote stripping, exact `home` / `inherit` keywords, and the path fallback
with owned storage — `Config.zig:5302`), and `value()` matches upstream (a path
only for the path variant, `:5347`); the tests cover the upstream cases plus
trim, plain path, missing/empty input, and `value()`; gates are clean. "Approved
with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-145718-r489-prompt.md` (result)
- Result: `logs/codex-review/20260604-145718-r489-last-message.md` (result)

## Conclusion

`WorkingDirectory` now parses — a keyword-or-quoted-path union reusing the Zig
whitespace set from Experiment 480. The config parse layer now spans thirteen
value types plus the reusable `parse_uint` / `config::string` /
`config::unicode_range` helpers. The next slice can port another self-contained
value type (e.g. `BackgroundBlur`), the font `CodepointMap` storage (toward
`RepeatableCodepointMap`), or begin the per-field parser dispatch / the config
`EntryFormatter` (which unblocks the deferred `formatEntry` side), continuing
toward the full config loader.
