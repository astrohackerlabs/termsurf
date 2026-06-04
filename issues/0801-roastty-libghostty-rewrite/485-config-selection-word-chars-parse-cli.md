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

# Experiment 485: the config SelectionWordChars CLI parser (SelectionWordChars::parse_cli)

## Description

With the escape-aware codepoint iterator landed (Experiment 484), this
experiment ports `SelectionWordChars` (upstream `Config.SelectionWordChars`) —
the `selection-word-chars` config: the set of codepoints that bound a word
during selection. Its parser walks the value through `config::string`'s
codepoint iterator (so `\t`, `\u{2502}`, etc. are honored), always prepending
the null boundary (`U+0000`). The default is the terminal layer's
`DEFAULT_WORD_BOUNDARIES`. The `formatEntry` formatter stays deferred.

## Upstream behavior

In `config/Config.zig`, `Config.SelectionWordChars`:

```zig
pub const SelectionWordChars = struct {
    /// The parsed codepoints. Always includes null (U+0000) at index 0.
    codepoints: []const u21 = &terminal.selection_codepoints.default_word_boundaries,

    pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void {
        const value = input orelse return error.ValueRequired;

        // Parse string with Zig escape sequence support into codepoints
        var list: std.ArrayList(u21) = .empty;
        defer list.deinit(alloc);

        // Always include null as first boundary
        try list.append(alloc, 0);

        var it = string.codepointIterator(value);
        while (it.next() catch return error.InvalidValue) |codepoint| {
            try list.append(alloc, codepoint);
        }

        self.codepoints = try list.toOwnedSlice(alloc);
    }

    pub fn equal(self: Self, other: Self) bool {
        return std.mem.eql(u21, self.codepoints, other.codepoints);
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- A fresh list is built starting with the null codepoint (`0`) at index 0.
- Each codepoint from `string.codepointIterator(value)` is appended; an iterator
  failure (a bad escape / bad UTF-8) is `error.InvalidValue`.
- The built list replaces `self.codepoints`.
- The default is `terminal.selection_codepoints.default_word_boundaries` (which
  itself starts with the null boundary).

Upstream's tests: `" \t;,"` → `[0, ' ', '\t', ';', ',']` (null + 4); the
escape-equivalent `" \\t;,"` → the same; `"\\\\;"` → `[0, '\\', ';']`;
`"\\u{2502};"` → `[0, U+2502, ';']`.

roastty already has `terminal::color`-adjacent
`terminal::selection_codepoints::DEFAULT_WORD_BOUNDARIES` (the same
` \t'"│`|:;,()[]{}<>$`boundary set, starting with`0`), and the `config::string::codepoint_iterator`
from Experiment 484.

## Rust mapping

`roastty/src/terminal/mod.rs` / `selection_codepoints.rs` — widen
`selection_codepoints` and `DEFAULT_WORD_BOUNDARIES` from `mod`/`pub(super)` to
`pub(crate)`, since the config default references it (the existing in-`terminal`
callers are unaffected).

`roastty/src/config/mod.rs`:

```rust
use crate::config::string::codepoint_iterator;
use crate::terminal::selection_codepoints::DEFAULT_WORD_BOUNDARIES;

/// An error parsing `SelectionWordChars` (upstream `parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionWordCharsParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// A codepoint failed to parse (a bad escape / bad UTF-8; upstream
    /// `error.InvalidValue`).
    InvalidValue,
}

/// The `selection-word-chars` config (upstream `Config.SelectionWordChars`): the
/// word-boundary codepoints, always starting with the null codepoint. The
/// `formatEntry` formatter is ported later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionWordChars {
    pub codepoints: Vec<u32>,
}

impl Default for SelectionWordChars {
    fn default() -> Self {
        SelectionWordChars {
            codepoints: DEFAULT_WORD_BOUNDARIES.to_vec(),
        }
    }
}

impl SelectionWordChars {
    /// Parse the `selection-word-chars` value (upstream `parseCLI`): a missing
    /// value is `ValueRequired`; otherwise the codepoints (with escape support)
    /// are parsed into a fresh list that always starts with the null codepoint,
    /// and an iterator failure is `InvalidValue`.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), SelectionWordCharsParseError> {
        let value = input.ok_or(SelectionWordCharsParseError::ValueRequired)?;

        // Always include null as the first boundary.
        let mut list = vec![0u32];
        for cp in codepoint_iterator(value.as_bytes()) {
            list.push(cp.map_err(|_| SelectionWordCharsParseError::InvalidValue)?);
        }

        self.codepoints = list;
        Ok(())
    }
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the fresh list seeded
with the null codepoint, the escape-aware codepoint iteration (an iterator
failure → `InvalidValue`), and the replacement of `self.codepoints`. The default
is the terminal `DEFAULT_WORD_BOUNDARIES`. `Clone` / `PartialEq` / `Eq` are
derived (they compare/copy the full `codepoints`, exactly like upstream's
`clone` / `equal`).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `SelectionWordChars` struct (`codepoints`,
  defaulting to the terminal word-boundary set) and
  `SelectionWordChars::parse_cli` (upstream `parseCLI`), plus
  `SelectionWordCharsParseError`.
- **Faithful**: the `ValueRequired` guard; the always-null-first list; the
  escape-aware codepoint iteration via `config::string`; the iterator-failure →
  `InvalidValue`; the full-list replacement; the `DEFAULT_WORD_BOUNDARIES`
  default — exactly upstream's `parseCLI`. `Clone` / `PartialEq` compare the
  whole list, matching upstream's `clone` / `equal`.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>` (iterated over
  `as_bytes()`, like upstream's `[]const u8`); `[]const u21` → `Vec<u32>` (the
  reusable iterator already yields `u32`); the arena `ArrayList(u21)` → `Vec`;
  the two upstream errors → `SelectionWordCharsParseError`.
- **Faithful re-use**: the codepoint iteration reuses the Experiment 484
  `config::string::codepoint_iterator`; the default reuses
  `terminal::selection_codepoints::DEFAULT_WORD_BOUNDARIES` (widened to
  `pub(crate)`; upstream's table is `pub`).
- **Deferred**: `SelectionWordChars.formatEntry` (re-encodes the codepoints —
  minus the leading null — back to UTF-8; depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter. (Consumed by later
  slices; this experiment lands the parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/terminal/mod.rs`: widen `mod selection_codepoints;` to
   `pub(crate) mod selection_codepoints;`.
2. `roastty/src/terminal/selection_codepoints.rs`: widen
   `DEFAULT_WORD_BOUNDARIES` from `pub(super)` to `pub(crate)`.
3. `roastty/src/config/mod.rs`: add
   `SelectionWordCharsParseError { ValueRequired, InvalidValue }`, the
   `SelectionWordChars` struct (`codepoints: Vec<u32>`,
   `derive(Debug, Clone, PartialEq, Eq)`) with a `Default` of
   `DEFAULT_WORD_BOUNDARIES`, and `SelectionWordChars::parse_cli`.
4. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` tests: `" \t;,"` → `[0, ' ', '\t', ';', ',']`;
     the escape form `" \\t;,"` → the same; `"\\\\;"` → `[0, '\\', ';']`;
     `"\\u{2502};"` → `[0, 0x2502, ';']`.
   - errors: `None` → `ValueRequired`; a bad escape (`"\\q"`) → `InvalidValue`.
   - empty string (design-review Low): `Some("")` → `Ok` with
     `codepoints == [0]` (only `null` is `ValueRequired`; an empty value just
     seeds the null boundary).
   - no mutation on invalid (design-review Low): after a successful parse, a
     subsequent `parse_cli(Some("\\q"))` returns `InvalidValue` and leaves the
     prior `codepoints` intact (the assignment happens only after the full list
     parses).
   - default: a default `SelectionWordChars` has
     `codepoints == DEFAULT_WORD_BOUNDARIES`.
5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty selection_word_chars
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `SelectionWordChars::parse_cli` builds a fresh codepoint list starting with
  the null codepoint, parsing the value via the escape-aware iterator, returning
  `ValueRequired` on a missing value and `InvalidValue` on a bad codepoint —
  faithful to upstream's `parseCLI`; the default is `DEFAULT_WORD_BOUNDARIES`;
- the tests pass (the upstream cases; the error cases; the default), and the
  existing tests still pass;
- `formatEntry` and the broader config parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the codepoint list is wrong (missing/duplicate null,
escapes not honored, a bad codepoint accepted), a missing value does not error,
the default is wrong, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with no
Required/Recommended findings (two **Low**, folded in). It verified against the
vendored upstream that the type mapping, the `DEFAULT_WORD_BOUNDARIES` default,
the escape-aware iteration, the seeded leading null, the full-list replacement,
and the derived `Clone`/`PartialEq`/`Eq` are all faithful to upstream's
`parseCLI` / `clone` / `equal` (`Config.zig:6150`/`:6156`/`:6174`,
`selection_codepoints.zig:7`) — and that, unlike `RepeatableString`, deriving
the value semantics is correct here because `equal` compares the whole
`codepoints` slice with no excluded field.

- **Low (folded in):** add an empty-string test — `parse_cli(Some(""))` succeeds
  with `codepoints == [0]` (only `null` is `ValueRequired`; an empty value seeds
  the null boundary and the iterator yields nothing, `Config.zig:6163`).
- **Low (folded in):** add a no-mutation-on-invalid test — upstream assigns
  `self.codepoints` only after the full list parses, so invalid input leaves the
  prior value intact (`Config.zig:6160`/`:6171`); the Rust `?` in the loop
  returns before the assignment, matching this.

Review artifacts:

- Prompt: `logs/codex-review/20260604-142034-d485-prompt.md` (design)
- Result: `logs/codex-review/20260604-142034-d485-last-message.md` (design)

## Result

**Result:** Pass

`SelectionWordChars` was implemented exactly as designed — the struct
(`codepoints: Vec<u32>` defaulting to `DEFAULT_WORD_BOUNDARIES`), the derived
`Clone`/`PartialEq`/`Eq`, and `parse_cli` (the `ValueRequired` guard, the fresh
list seeded with the null codepoint, the escape-aware codepoint iteration via
the Exp 484 `config::string::codepoint_iterator`, the iterator-failure →
`InvalidValue`, and the full-list replacement only on success).
`terminal::selection_codepoints` and `DEFAULT_WORD_BOUNDARIES` were widened to
`pub(crate)`. The new test `selection_word_chars_parse_cli_parses_codepoints`
covers the upstream cases, the missing-value error, the empty-string `[0]` case,
the no-mutation-on-invalid case, and the default.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2968 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation is faithful to upstream `SelectionWordChars` (the
default uses the terminal boundary table, parsing seeds a fresh list with `0`,
consumes the escape-aware codepoint iterator, maps iterator errors to
`InvalidValue`, and replaces `self.codepoints` only after a successful full
parse — `Config.zig:6150`/ `:6156`, `selection_codepoints.zig:7`); the folded-in
tests cover the two edge cases (empty input → `[0]`; invalid input leaves the
prior value unchanged); gates are clean. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-142359-r485-prompt.md` (result)
- Result: `logs/codex-review/20260604-142359-r485-last-message.md` (result)

## Conclusion

`SelectionWordChars` now parses — the first consumer of the Experiment 484
`config::string` codepoint iterator, with escape-aware word-boundary codepoints
defaulting to the terminal boundary table. The config parse layer now spans
eleven value types plus the reusable `u8`/`u32`/bool parsers and the
`config::string` iterator. The next slice can port another self-contained config
value type's `parseCLI` (e.g. `RepeatableFontVariation`,
`RepeatableCodepointMap`, or — once a faithful `parseFloat` lands —
`QuickTerminalSize`), or begin the per-field parser dispatch, continuing toward
the full config loader.
