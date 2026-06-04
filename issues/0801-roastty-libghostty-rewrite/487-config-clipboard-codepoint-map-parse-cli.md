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

# Experiment 487: the config RepeatableClipboardCodepointMap CLI parser (parse_cli)

## Description

With the Unicode range parser landed (Experiment 486), this experiment ports
`RepeatableClipboardCodepointMap` (upstream
`Config.RepeatableClipboardCodepointMap`) — the `clipboard-codepoint-map`
config: it maps codepoint ranges to a **replacement** (another codepoint
`U+XXXX`, or a literal string) applied when copying to the clipboard. Its parser
splits `ranges=replacement`, parses the replacement, and reuses the Experiment
486 `UnicodeRangeParser` for the range key. The `ClipboardCodepointMap.add`
storage is a plain append, so the map is a `Vec`. The `formatEntry` formatter
stays deferred.

## Upstream behavior

In `config/Config.zig`, `Config.RepeatableClipboardCodepointMap`, and
`config/ClipboardCodepointMap.zig`:

```zig
pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void {
    const input = input_ orelse return error.ValueRequired;
    const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
    const whitespace = " \t";
    const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
    const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);

    // Parse the replacement value - either a codepoint or string
    const replacement: ClipboardCodepointMap.Replacement = if (std.mem.startsWith(u8, value, "U+")) blk: {
        const cp_str = value[2..]; // Skip "U+"
        const cp = std.fmt.parseInt(u21, cp_str, 16) catch return error.InvalidValue;
        break :blk .{ .codepoint = cp };
    } else blk: {
        if (!std.unicode.utf8ValidateSlice(value)) return error.InvalidValue;
        const value_copy = try alloc.dupe(u8, value);
        break :blk .{ .string = value_copy };
    };

    var p: UnicodeRangeParser = .{ .input = key };
    while (try p.next()) |range| {
        try self.map.add(alloc, .{ .range = range, .replacement = replacement });
    }
}
```

```zig
// ClipboardCodepointMap.add:
pub fn add(self: *ClipboardCodepointMap, alloc: Allocator, entry: Entry) !void {
    assert(entry.range[0] <= entry.range[1]);
    try self.list.append(alloc, entry);  // plain append; later entries take priority
}
```

- A missing value is `error.ValueRequired`; no `=` is `error.InvalidValue`.
- The key and value are split on the first `=` and trimmed of `" \t"`.
- The **replacement** is parsed first: a `U+`-prefixed value is `value[2..]`
  parsed as a base-16 `u21` codepoint (parse error → `error.InvalidValue`);
  otherwise it is a literal string (UTF-8 validated — always valid here since
  the input is a Rust `&str`).
- Then the key is walked by `UnicodeRangeParser`, and `{ range, replacement }`
  is appended for each range (the assert holds because the parser guarantees
  `range[0] <= range[1]`). The map is **not** reset, so repeated keys
  accumulate.

`Replacement = union { codepoint: u21, string: []const u8 }`. Upstream's tests:
`"U+2500=U+002D"` → one entry `{[0x2500, 0x2500], codepoint 0x2D}`;
`"U+03A3=SUM"` → `{[0x3A3, 0x3A3], string "SUM"}`; `"U+2500-U+2503=|"` →
`{[0x2500, 0x2503], string "|"}`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// A `clipboard-codepoint-map` replacement (upstream `ClipboardCodepointMap.Replacement`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClipboardReplacement {
    /// Replace with another codepoint (`U+XXXX`).
    Codepoint(u32),
    /// Replace with a literal string.
    String(String),
}

/// One `clipboard-codepoint-map` entry (upstream `ClipboardCodepointMap.Entry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipboardCodepointMapEntry {
    pub range: [u32; 2],
    pub replacement: ClipboardReplacement,
}

/// An error parsing a `clipboard-codepoint-map` (upstream `parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardCodepointMapParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// No `=`, a bad range, or a bad codepoint replacement (upstream
    /// `error.InvalidValue`).
    InvalidValue,
}

/// The `clipboard-codepoint-map` config (upstream `Config.RepeatableClipboardCodepointMap`):
/// codepoint ranges mapped to a replacement. The `formatEntry` formatter is ported
/// later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatableClipboardCodepointMap {
    pub map: Vec<ClipboardCodepointMapEntry>,
}

impl RepeatableClipboardCodepointMap {
    /// Parse one `ranges=replacement` assignment (upstream `parseCLI`): split on the
    /// first `=`, trim, parse the replacement (a `U+XXXX` codepoint or a literal
    /// string), then append `{ range, replacement }` for each range the key yields.
    /// A missing value is `ValueRequired`; a missing `=`, a bad range, or a bad
    /// codepoint replacement is `InvalidValue`.
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), ClipboardCodepointMapParseError> {
        let input = input.ok_or(ClipboardCodepointMapParseError::ValueRequired)?;
        let eql = input
            .find('=')
            .ok_or(ClipboardCodepointMapParseError::InvalidValue)?;
        let trim = |s: &str| s.trim_matches(|c: char| c == ' ' || c == '\t');
        let key = trim(&input[..eql]);
        let value = trim(&input[eql + 1..]);

        let replacement = if let Some(cp_str) = value.strip_prefix("U+") {
            let cp =
                parse_u21_hex(cp_str).ok_or(ClipboardCodepointMapParseError::InvalidValue)?;
            ClipboardReplacement::Codepoint(cp)
        } else {
            // `value` is already valid UTF-8 (it is a `&str`).
            ClipboardReplacement::String(value.to_string())
        };

        let mut parser = unicode_range::UnicodeRangeParser::new(key.as_bytes());
        while let Some(range) = parser
            .next()
            .map_err(|_| ClipboardCodepointMapParseError::InvalidValue)?
        {
            self.map.push(ClipboardCodepointMapEntry {
                range,
                replacement: replacement.clone(),
            });
        }
        Ok(())
    }
}

/// Parse a base-16 `u21` (upstream `std.fmt.parseInt(u21, _, 16)`): an optional
/// `+`/`-` sign, then hex digits with interior-only `_` separators
/// (leading/trailing `_` rejected). `-0` is `0`; a negative nonzero, an overflow
/// beyond the `u21` max (`0x1FFFFF`), or any non-hex is `None`. (Mirrors the
/// `parse_u32_dec` base-10 helper, with base 16 and the `u21` bound — distinct from
/// `unicode_range`'s pure-hex `parse_hex_u21`, whose input is pre-scanned to hex.)
fn parse_u21_hex(buf: &str) -> Option<u32> {
    let (neg, rest): (bool, &str) = match buf.as_bytes().first() {
        Some(b'+') => (false, &buf[1..]),
        Some(b'-') => (true, &buf[1..]),
        _ => (false, buf),
    };
    let bytes = rest.as_bytes();
    if bytes.is_empty() || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return None;
    }
    let mut acc: i64 = 0;
    for &c in bytes {
        if c == b'_' {
            continue;
        }
        let digit = (c as char).to_digit(16)? as i64;
        if acc != 0 {
            acc = acc.checked_mul(16).filter(|&v| v <= 0x1FFFFF)?;
        } else if neg {
            acc = -digit;
            if acc < 0 {
                return None;
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=0x1FFFFF).contains(&acc) {
            return None;
        }
    }
    Some(acc as u32)
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the first-`=` split
with per-side `" \t"` trim, the replacement-first parse (`U+` codepoint via
`parse_u21_hex` or a literal string — UTF-8-valid by construction), and the
per-range append via the reused `UnicodeRangeParser`, with no reset between
calls. `parse_u21_hex` is a faithful base-16 `u21` `parseInt` (the same
sign/underscore/overflow shape as the Experiment 481 `parse_u32_dec`). `Clone` /
`PartialEq` / `Eq` are derived (they compare/copy the full entries, matching
upstream's `clone` / `equal`).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `RepeatableClipboardCodepointMap` (the `map`
  vector), `ClipboardCodepointMapEntry`, `ClipboardReplacement`, `parse_cli`
  (upstream `parseCLI`), and the base-16 `u21` helper, plus
  `ClipboardCodepointMapParseError`.
- **Faithful**: the `ValueRequired` guard; the first-`=` split (`InvalidValue`
  on none); the per-side `" \t"` trim; the replacement-first parse (`U+`
  codepoint via base-16 `u21`; else a literal string); the per-range append (no
  reset, later entries accumulating); the range parse via `UnicodeRangeParser` —
  exactly upstream's `parseCLI`. `add`'s plain append maps to `Vec::push` (the
  `range[0] <= range[1]` assert is guaranteed by the parser).
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; `std.mem.indexOf(=)`
  → `str::find('=')`; `parseInt(u21, _, 16)` → `parse_u21_hex`; the
  `Replacement` union → a `ClipboardReplacement` enum; the `MultiArrayList`
  storage → `Vec`; the UTF-8 validation is a no-op (a `&str` is always valid
  UTF-8), so the string branch never errors; the two upstream errors →
  `ClipboardCodepointMapParseError`.
- **Faithful re-use**: the range key is parsed by the Experiment 486
  `config::unicode_range::UnicodeRangeParser`.
- **Deferred**: `RepeatableClipboardCodepointMap.formatEntry` (renders each
  entry back to `U+XXXX[-U+YYYY]=…`; depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter. `clone` / `equal`
  are covered by the derives. (Consumed by later slices; this experiment lands
  the parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `ClipboardReplacement`,
   `ClipboardCodepointMapEntry`, `ClipboardCodepointMapParseError`, the
   `RepeatableClipboardCodepointMap` struct (`map: Vec<…>`,
   `derive(Debug, Clone, Default, PartialEq, Eq)`), `parse_cli`, and the private
   `parse_u21_hex` base-16 helper.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` tests: `"U+2500=U+002D"` → one entry
     `{[0x2500, 0x2500], Codepoint(0x2D)}`; `"U+03A3=SUM"` →
     `{[0x3A3, 0x3A3], String("SUM")}`; `"U+2500-U+2503=|"` →
     `{[0x2500, 0x2503], String("|")}`.
   - accumulation: two parses append (the map is not reset).
   - whitespace: `" U+2500 = U+002D "` → the same single codepoint entry.
   - the empty-string replacement: `"U+2500="` →
     `{[0x2500, 0x2500], String("")}`.
   - errors: `None` → `ValueRequired`; no `=` (`"U+2500"`) → `InvalidValue`; a
     bad range (`"X=A"`) → `InvalidValue`; a bad codepoint replacement
     (`"U+2500=U+ZZ"` and the empty `"U+2500=U+"`) → `InvalidValue`.
   - replacement-codepoint parser edges (design-review Low — the raw
     `parseInt(u21, _, 16)` path): `"U+2500=U++2D"` (leading `+`) →
     `Codepoint(0x2D)`; `"U+2500=U+-0"` (unsigned `-0`) → `Codepoint(0)`;
     `"U+2500=U+2_D"` (interior underscore) → `Codepoint(0x2D)`;
     `"U+2500=U+200000"` (`u21` overflow) → `InvalidValue`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty clipboard_codepoint_map
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `RepeatableClipboardCodepointMap::parse_cli` splits `ranges=replacement`,
  parses the replacement (codepoint or string), and appends one entry per range
  via the reused `UnicodeRangeParser`, returning `ValueRequired` /
  `InvalidValue` per upstream — faithful to upstream's `parseCLI`;
- the tests pass (the upstream cases; the accumulation, whitespace,
  empty-string, and error cases), and the existing tests still pass;
- `formatEntry` and the broader config parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if an entry is parsed wrong (wrong split/trim, wrong
replacement, wrong range append, a reset introduced), a missing value does not
error, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with no
Required/Recommended findings (one **Low**, folded in). It verified against the
vendored upstream that the design is faithful — the first-`=` split, the
space/tab trim, the replacement parsed before the range iteration, the exact
`U+` codepoint branch, the UTF-8 string branch, the per-range append with no
reset, and the order-sensitive full equality all match upstream
(`Config.zig:8250`, `ClipboardCodepointMap.zig:37`) — and that treating
`utf8ValidateSlice` as a no-op is appropriate for a Rust `&str` input boundary.

- **Low (folded in):** add replacement-codepoint parser edge tests for the raw
  `parseInt(u21, _, 16)` path (which, unlike `UnicodeRangeParser`'s pre-scanned
  hex run, sees the raw substring): a leading `+` (`U++2D`), an unsigned `-0`
  (`U+-0`), an interior underscore (`U+2_D`), and a `u21` overflow (`U+200000`),
  per `Config.zig:8258`. Added to the test plan.

Review artifacts:

- Prompt: `logs/codex-review/20260604-143718-d487-prompt.md` (design)
- Result: `logs/codex-review/20260604-143718-d487-last-message.md` (design)
