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

# Experiment 480: the config Duration CLI parser (Duration::parse_cli)

## Description

Moving off the color value types, this experiment ports the first non-color
config value: `Duration` (upstream `Config.Duration`) — a time span stored in
nanoseconds and written as a sequence of `number+unit` segments (`"1s"`,
`"500ms"`, `"1h30m"`, `"1d 12h"`). Its parser walks the input segment by
segment, greedily reading a number and then the longest matching unit, summing
the nanosecond products with saturating math. The `format`/`formatEntry`,
`asMilliseconds`/`cval`, and `round`/`lte` helpers stay deferred (no consumer
yet).

## Upstream behavior

In `config/Config.zig`, `Config.Duration`:

```zig
pub const Duration = struct {
    /// Duration in nanoseconds
    duration: u64 = 0,

    const units = [_]struct { name: []const u8, factor: u64 }{
        // The order is important as the first factor that matches will be the
        // default unit that is used for formatting.
        .{ .name = "y",  .factor = 365 * std.time.ns_per_day },
        .{ .name = "w",  .factor = std.time.ns_per_week },
        .{ .name = "d",  .factor = std.time.ns_per_day },
        .{ .name = "h",  .factor = std.time.ns_per_hour },
        .{ .name = "m",  .factor = std.time.ns_per_min },
        .{ .name = "s",  .factor = std.time.ns_per_s },
        .{ .name = "ms", .factor = std.time.ns_per_ms },
        .{ .name = "µs", .factor = std.time.ns_per_us },
        .{ .name = "us", .factor = std.time.ns_per_us },
        .{ .name = "ns", .factor = 1 },
    };

    pub fn parseCLI(input: ?[]const u8) !Duration {
        var remaining = input orelse return error.ValueRequired;

        var value: ?u64 = null;
        while (remaining.len > 0) {
            // Skip over whitespace before the number
            while (remaining.len > 0 and std.ascii.isWhitespace(remaining[0])) {
                remaining = remaining[1..];
            }
            // There was whitespace at the end, that's OK
            if (remaining.len == 0) break;

            // Find the longest number (greedy parseUnsigned over growing prefixes)
            const number: u64 = number: { ... } orelse return error.InvalidValue;

            // A number without a unit is invalid unless the number is exactly
            // zero. In that case, the unit is unambiguous since it's all the same.
            if (remaining.len == 0) {
                if (number == 0) { value = 0; break; }
                return error.InvalidValue;
            }

            // Find the longest matching unit (to distinguish 'm' from 'ms').
            const factor = factor: { ... } orelse return error.InvalidValue;

            // Add our time value to the total. Avoid overflow with saturating math.
            const diff = std.math.mul(u64, number, factor) catch std.math.maxInt(u64);
            value = (value orelse 0) +| diff;
        }

        return if (value) |v| .{ .duration = v } else error.ValueRequired;
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- The loop, per segment: skips leading ASCII whitespace; if only trailing
  whitespace remained, breaks; greedily reads the **longest number** (see
  below); a number with no following unit is `error.InvalidValue` **unless** the
  number is exactly `0` (then `value = 0` and the loop ends); reads the
  **longest matching unit** (so `ms` wins over `m`); and adds `number * factor`
  to the running total with **saturating** multiply and add.
- An empty or whitespace-only input leaves `value` null and returns
  `error.ValueRequired`.

The unit factors are nanoseconds: `ns_per_us = 1_000`, `ns_per_ms = 1_000_000`,
`ns_per_s = 1_000_000_000`, `ns_per_min = 60 * ns_per_s`,
`ns_per_hour = 60 * ns_per_min`, `ns_per_day = 24 * ns_per_hour`,
`ns_per_week = 7 * ns_per_day`, and `y = 365 * ns_per_day`. `µs` is the
multi-byte (UTF-8) micro sign; `us` is its ASCII alias.

### The greedy number scan, and its simplification

Upstream finds the number by trying
`std.fmt.parseUnsigned(u64, remaining[0..index], 10)` for `index = 1, 2, 3, …`,
keeping the last success and **breaking at the first failing prefix** (then
reverting `remaining` to just after the last success). For base-10 unsigned
parsing the only ways a prefix fails are: a non-digit byte, a trailing `_`
(`"1_"` fails), a leading sign (`"+"`/`"-"` fails), or `u64` overflow. Because
the loop breaks at the _first_ failure, this is exactly equivalent to: **consume
the maximal run of leading ASCII digits, stopping before any digit that would
overflow `u64`** (a `checked_mul(10)` / `checked_add(digit)` that, on overflow,
leaves that digit in `remaining`). A sign or `_` is a non-digit and ends the run
immediately, matching the upstream break. If no digit is consumed the number is
absent → `error.InvalidValue`. The port uses this equivalent digit scan
(documented here and flagged for review), avoiding a full `parseUnsigned`
re-implementation.

The port also operates on **bytes** (`&[u8]`), like upstream's `[]const u8`, so
the `remaining[0..index]` unit slicing never splits the multi-byte `µs` on a
char boundary.

Upstream's whitespace set is Zig's `std.ascii.isWhitespace`: space, `\t`, `\n`,
`\r`, vertical tab `0x0B`, form feed `0x0C` (note `0x0B` is **not** in Rust's
`u8::is_ascii_whitespace`, so the port uses an explicit set).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing a `Duration` config value (upstream `Duration.parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurationParseError {
    /// No value, or an all-whitespace value (upstream `error.ValueRequired`).
    ValueRequired,
    /// A malformed segment (upstream `error.InvalidValue`).
    InvalidValue,
}

/// A `Duration` config value (upstream `Config.Duration`): a time span in
/// nanoseconds. `format` / `asMilliseconds` / `round` / `lte` are ported later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Duration {
    pub duration: u64,
}

/// `(name, factor-in-ns)`, in upstream order (first match is the formatting unit;
/// longest match wins when parsing, so `ms` beats `m`).
const DURATION_UNITS: &[(&[u8], u64)] = &[
    (b"y", 365 * NS_PER_DAY),
    (b"w", NS_PER_WEEK),
    (b"d", NS_PER_DAY),
    (b"h", NS_PER_HOUR),
    (b"m", NS_PER_MIN),
    (b"s", NS_PER_S),
    (b"ms", NS_PER_MS),
    ("µs".as_bytes(), NS_PER_US),
    (b"us", NS_PER_US),
    (b"ns", 1),
];

const NS_PER_US: u64 = 1_000;
const NS_PER_MS: u64 = 1_000_000;
const NS_PER_S: u64 = 1_000_000_000;
const NS_PER_MIN: u64 = 60 * NS_PER_S;
const NS_PER_HOUR: u64 = 60 * NS_PER_MIN;
const NS_PER_DAY: u64 = 24 * NS_PER_HOUR;
const NS_PER_WEEK: u64 = 7 * NS_PER_DAY;

impl Duration {
    /// Parse a duration (upstream `Duration.parseCLI`): a sequence of
    /// `number+unit` segments summed in nanoseconds with saturating math. A
    /// missing/all-whitespace value is `ValueRequired`; a bad number/unit, or a
    /// nonzero number with no unit, is `InvalidValue`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<Duration, DurationParseError> {
        let mut remaining = input.ok_or(DurationParseError::ValueRequired)?.as_bytes();

        let mut value: Option<u64> = None;
        while !remaining.is_empty() {
            while let [first, rest @ ..] = remaining {
                if !is_ascii_ws_zig(*first) {
                    break;
                }
                remaining = rest;
            }
            if remaining.is_empty() {
                break; // trailing whitespace is fine
            }

            // Longest number: consume leading digits, stopping before u64 overflow.
            let mut number: Option<u64> = None;
            while let [d @ b'0'..=b'9', rest @ ..] = remaining {
                match number
                    .unwrap_or(0)
                    .checked_mul(10)
                    .and_then(|n| n.checked_add((d - b'0') as u64))
                {
                    Some(n) => {
                        number = Some(n);
                        remaining = rest;
                    }
                    None => break, // this digit would overflow; leave it
                }
            }
            let number = number.ok_or(DurationParseError::InvalidValue)?;

            if remaining.is_empty() {
                if number == 0 {
                    value = Some(0);
                    break;
                }
                return Err(DurationParseError::InvalidValue);
            }

            // Longest matching unit (so "ms" wins over "m").
            let mut factor: Option<u64> = None;
            let mut unit_len = 0usize;
            for index in 1..=remaining.len() {
                if let Some(&(_, f)) =
                    DURATION_UNITS.iter().find(|(name, _)| *name == &remaining[..index])
                {
                    factor = Some(f);
                    unit_len = index;
                }
            }
            let factor = factor.ok_or(DurationParseError::InvalidValue)?;
            remaining = &remaining[unit_len..];

            let diff = number.saturating_mul(factor);
            value = Some(value.unwrap_or(0).saturating_add(diff));
        }

        value
            .map(|duration| Duration { duration })
            .ok_or(DurationParseError::ValueRequired)
    }
}

/// Zig's `std.ascii.isWhitespace` set: space, `\t`, `\n`, `\r`, vertical tab, and
/// form feed (note vertical tab `0x0B` is not in Rust's `is_ascii_whitespace`).
fn is_ascii_ws_zig(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the per-segment
whitespace skip with the trailing-whitespace break, the greedy
(overflow-stopping) number scan, the zero-without-unit special case, the
longest-unit match, the saturating `number * factor` accumulation, and the final
`ValueRequired` for an empty/ whitespace-only input.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `Duration` struct (`duration: u64`), its unit
  table, and `Duration::parse_cli` (upstream `Duration.parseCLI`), plus
  `DurationParseError`.
- **Faithful**: the `ValueRequired` guard; the whitespace skip + trailing-ws
  break; the greedy number (equivalent to upstream's
  longest-`parseUnsigned`-prefix, see above); the nonzero-number-without-unit
  `InvalidValue` and the zero-without-unit acceptance; the longest-unit match
  (`ms` over `m`); the saturating multiply/add; the empty/whitespace-only
  `ValueRequired` — exactly upstream's `parseCLI`. The unit set, factors, and
  order match (incl. the multi-byte `µs` and its `us` alias).
- **Faithful adaptation**: `?[]const u8` → `Option<&str>` (parsed over
  `as_bytes()`, like upstream's `[]const u8`);
  `std.fmt.parseUnsigned`-per-prefix → the overflow-stopping digit scan
  (behaviorally identical, justified above); `std.math.mul … catch maxInt` →
  `saturating_mul`; `+|` → `saturating_add`; `std.ascii.isWhitespace` →
  `is_ascii_ws_zig` (explicit set incl. `0x0B`); the two upstream errors →
  `DurationParseError`.
- **Deferred**: `Duration.format` / `formatEntry` (the inverse formatter;
  depends on the not-yet-ported config `EntryFormatter`), `asMilliseconds` /
  `cval` (FFI), and `round` / `lte` (no consumer yet). `clone` / `equal` are
  covered by the derives. (Consumed by later slices; this experiment lands the
  parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `DurationParseError { ValueRequired, InvalidValue }`, the `Duration`
     struct (`duration: u64`,
     `derive(Debug, Clone, Copy, Default, PartialEq, Eq)`), the `DURATION_UNITS`
     table + `NS_PER_*` consts, `Duration::parse_cli`, and the private
     `is_ascii_ws_zig` helper.
2. Tests (in `config/mod.rs`):
   - single units: `"1s"` → `1_000_000_000`, `"500ms"` → `500_000_000`, `"1ns"`
     → `1`, `"1us"` → `1_000`, `"1µs"` → `1_000` (multi-byte unit), `"1m"` →
     `60_000_000_000` vs `"1ms"` → `1_000_000` (longest-unit distinction),
     `"1h"`, `"1d"`, `"1w"`, `"1y"` → their factors.
   - multi-segment: `"1h30m"` → `1h + 30m`, `"1m30s"` → `90_000_000_000`,
     `"1d 12h"` (inner whitespace) → `1d + 12h`.
   - zero: `"0"` → `0`; whitespace tolerance `" 1s "` → `1_000_000_000`.
   - saturating: `"99999999y"` (product overflows `u64`) → `u64::MAX`.
   - errors: `None` → `ValueRequired`; `""` → `ValueRequired`; `"   "` →
     `ValueRequired`; `"5"` (nonzero, no unit) → `InvalidValue`; `"5x"` (bad
     unit) → `InvalidValue`; `"abc"` (no number) → `InvalidValue`.
   - trailing-whitespace subtlety (folded in from the design review): `"1 "` →
     `InvalidValue` and `"0 "` → `InvalidValue` — a number followed by
     whitespace is _not_ a complete segment, because the unit match runs on the
     `" "` (which matches no unit) before the next loop's whitespace skip; this
     is distinct from `"1s "` (a complete segment, then trailing whitespace) →
     valid.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty duration
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Duration::parse_cli` sums `number+unit` segments in nanoseconds with the
  greedy number scan, longest-unit match, zero-without-unit handling, and
  saturating math, returning `ValueRequired` on a missing/all-whitespace value
  and `InvalidValue` on a bad segment — faithful to upstream's `parseCLI`;
- the tests pass (the single-unit, multi-segment, zero, whitespace, saturating,
  and error cases), and the existing tests still pass;
- `format` / `asMilliseconds` / `round` / `lte` and the broader config
  parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a duration is parsed wrong (wrong number/unit/sum,
wrong longest-unit match, non-saturating overflow, wrong whitespace handling), a
missing/all-whitespace value does not error, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding and no Required/Recommended findings. It verified against the
vendored upstream: the digit-scan equivalence to the greedy `parseUnsigned` loop
is correct for this parser (signs fail immediately, underscores stop the number
and then fail as unit text, non-digits stop the number, overflow leaves the
overflowing digit in `remaining` — `Config.zig:10005`); byte-oriented parsing is
the right call for the multi-byte `µs`, and longest byte-slice unit matching
correctly handles `m` vs `ms` and `µs` (`:9958`); the whitespace set matches
`std.ascii.isWhitespace` incl. vertical tab / form feed; zero-without-a-unit
only succeeds when the number consumes the entire remaining input (`:10020`);
`saturating_mul` / `saturating_add` correctly maps `mul catch maxInt` and `+|`
(`:10057`); and deferring the formatting and helper methods is appropriate.

- **Low (folded in):** add the trailing-whitespace-after-a-bare-number case —
  `"1 "` is `InvalidValue` (the unit match runs on `" "` before the next loop's
  whitespace skip), distinct from `"1s "` (valid). Upstream tests this
  (`Config.zig:10300`). `"0 "` was added too, to lock the exact
  zero-without-unit behavior. Added to the test plan above.

Review artifacts:

- Prompt: `logs/codex-review/20260604-133918-d480-prompt.md` (design)
- Result: `logs/codex-review/20260604-133918-d480-last-message.md` (design)
