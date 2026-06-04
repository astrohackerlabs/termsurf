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

# Experiment 496: the Duration config formatter (Duration::format_entry)

## Description

Continuing the config **formatter** layer (Experiments 491–495), this experiment
ports `Duration.format` / `Duration.formatEntry` (upstream `Config.Duration`) —
the inverse of the Experiment 480 parser. It decomposes the nanosecond value
into the largest matching units (`1m 30s`, `500ms`, …) and writes it as one
string entry. It reuses the `DURATION_UNITS` table from Experiment 480, grounded
by the `EntryFormatter` from Experiment 491.

## Upstream behavior

In `config/Config.zig`, `Config.Duration`:

```zig
pub fn formatEntry(self: Duration, formatter: formatterpkg.EntryFormatter) !void {
    var buf: [64]u8 = undefined;
    var writer: std.Io.Writer = .fixed(&buf);
    try self.format(&writer);
    try formatter.formatEntry([]const u8, writer.buffered());
}

pub fn format(self: Duration, writer: *std.Io.Writer) !void {
    var value = self.duration;
    var i: usize = 0;
    for (units) |unit| {
        if (value >= unit.factor) {
            if (i > 0) writer.writeAll(" ") catch unreachable;
            const remainder = value % unit.factor;
            const quotient = (value - remainder) / unit.factor;
            writer.print("{d}{s}", .{ quotient, unit.name }) catch unreachable;
            value = remainder;
            i += 1;
        }
    }
}
```

- `format` walks the units (largest first: `y`, `w`, `d`, `h`, `m`, `s`, `ms`,
  `µs`, `us`, `ns`). For each unit whose factor `<= value`, it writes
  `{quotient}{unit-name}` (where `quotient = value / factor`) and reduces
  `value` to the remainder; segments are separated by a single space. So
  `90_000_000_000` ns formats to `1m 30s`. Because `µs` precedes `us` (both
  factor `1000`), a microsecond component is written as `µs`. A value of `0`
  produces the empty string.
- `formatEntry` writes that formatted string as a single string entry
  (`name = …\n`).

(Each `quotient` is `>= 1` since the unit is only used when `value >= factor`,
so no `0`-prefixed segment is produced.)

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl Duration {
    /// Decompose into the largest matching units (upstream `Duration.format`):
    /// `{quotient}{unit}` segments, space-separated (e.g. `1m 30s`). `0` → empty.
    fn format_value(self) -> String {
        use std::fmt::Write as _;
        let mut value = self.duration;
        let mut out = String::new();
        for &(name, factor) in DURATION_UNITS {
            if value >= factor {
                if !out.is_empty() {
                    out.push(' ');
                }
                let remainder = value % factor;
                let quotient = value / factor;
                // `name` is valid UTF-8 (a unit-name byte literal, incl. `µs`).
                let _ = write!(out, "{}{}", quotient, std::str::from_utf8(name).unwrap());
                value = remainder;
            }
        }
        out
    }

    /// Format as a config entry (upstream `Duration.formatEntry`): the decomposed
    /// duration string.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(&self.format_value());
    }
}
```

`format_value` mirrors `format`: it walks `DURATION_UNITS` largest-first,
appending `{quotient}{unit}` for each unit `<=` the running value (a space
before all but the first), reducing to the remainder.
`quotient = value / factor` and `remainder = value % factor` (upstream computes
the same via `(value - remainder) / factor`). `format_entry` writes the result
as a string entry. The empty-string for a `0` value yields a `name = \n` line
(via `entry_str("")`), matching upstream. The fixed-buffer writer /
`unreachable` error path has no Rust analog (a `String` build cannot fail).
`format_value` / `format_entry` take `self` by value (`Duration` is `Copy`).

## Scope / faithfulness notes

- **Ported (bridged)**: `Duration::format_value` (upstream `Duration.format`)
  and `Duration::format_entry` (upstream `Duration.formatEntry`).
- **Faithful**: the largest-first unit decomposition; the `{quotient}{unit}`
  segment shape; the single-space separator between segments; the `µs`-over-`us`
  preference (`µs` precedes `us` in the table); the empty string for `0` —
  exactly upstream's `format` / `formatEntry`.
- **Faithful adaptation**: the writer-into-buffer `format` → a
  `String`-returning `format_value`; `writer.print("{d}{s}")` →
  `write!("{}{}")`; `formatEntry([]const u8, …)` → `entry_str`; the multi-byte
  `µs` unit name is decoded from its byte literal via `from_utf8` (always
  valid). The `unreachable` / `OutOfMemory` paths have no Rust analog.
- **Deferred**: the remaining types' `formatEntry` (ported in later slices), the
  generic field-dispatch `formatEntry`, the `Duration` `asMilliseconds` / `cval`
  (FFI) / `round` / `lte` helpers, and the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `Duration::format_value` and
   `Duration::format_entry` (in the existing `impl Duration`).
2. Tests (in `config/mod.rs`):
   - `Duration { 1_000_000_000 }` (1s) → `"a = 1s\n"`; `{ 500_000_000 }` (500ms)
     → `"a = 500ms\n"`; `{ 90_000_000_000 }` (1m30s) → `"a = 1m 30s\n"`;
     `{ 1_000 }` (1µs) → `"a = 1µs\n"`; `{ 3_600_000_000_000 }` (1h) →
     `"a = 1h\n"`; `{ 0 }` → `"a = \n"`.
   - a multi-segment value spanning several units (e.g.
     `1d + 2h + 3m + 4s = 93_784_000_000_000` → `"a = 1d 2h 3m 4s\n"`).
   - the upstream max-value case (design-review Low): `u64::MAX` →
     `"a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n"` (exercises the whole
     largest-first table, including the `µs`-over-`us` preference).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty duration_format_entry
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Duration::format_value` / `format_entry` decompose the value into
  largest-first `{quotient}{unit}` space-separated segments (`µs` over `us`,
  empty for `0`) and write the string entry — faithful to upstream's `format` /
  `formatEntry`;
- the tests pass (the single-unit, multi-segment, `µs`, and zero cases), and the
  existing tests still pass;
- the other types' `formatEntry`, the generic field-dispatch, and the deferred
  `Duration` helpers stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a formatted duration differs from upstream (wrong
unit decomposition, wrong separator, `us` instead of `µs`, wrong zero handling),
an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with no
Required/Recommended findings (one **Low**, folded in). It confirmed the
formatter logic faithful: the largest-first decomposition, the single-space
separators, the quotient/remainder update, the `µs`-over-`us` preference (`µs`
appears first in the table), and the empty-string entry for zero
(`Config.zig:9958`/`:10065`); and that `from_utf8(name).unwrap()` is safe for
the fixed unit names.

- **Low (folded in):** add the upstream max-value `formatEntry` test —
  `u64::MAX` → `"a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n"` — which
  exercises the whole largest-first unit table (`Config.zig:10320`). Added to
  the test plan.

Review artifacts:

- Prompt: `logs/codex-review/20260604-153214-d496-prompt.md` (design)
- Result: `logs/codex-review/20260604-153214-d496-last-message.md` (design)

## Result

**Result:** Pass

`Duration::format_value` and `Duration::format_entry` were added to the existing
`impl Duration` exactly as designed — the largest-first unit decomposition over
`DURATION_UNITS` (`{quotient}{unit}` segments, single-space-separated, `µs` over
`us`, empty for `0`), written as a string entry. The new test
`duration_format_entry_decomposes_units` covers the single-unit, multi-segment,
`µs`, and zero cases, plus the folded-in `u64::MAX` full-table case
(`a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n`).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2981 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `Duration.format` /
`formatEntry` (largest-first unit decomposition, one-space separators, `µs`
preferred over `us`, zero as an empty entry, and the exact `u64::MAX` output —
`Config.zig:10065`/`:10320`); the folded-in max-value test covers the tricky
full-table decomposition; gates are clean. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-153456-r496-prompt.md` (result)
- Result: `logs/codex-review/20260604-153456-r496-last-message.md` (result)

## Conclusion

`Duration` now round-trips: `parse_cli` (Experiment 480) reads `1m30s` and
`format_value` / `format_entry` write it back as `1m 30s`, reusing the same
`DURATION_UNITS` table. The config formatter side now covers ten types. The next
slices can port the remaining types' `formatEntry` (`SelectionWordChars` — which
re-encodes codepoints to UTF-8 — `QuickTerminalSize`, the codepoint maps), then
the generic field-dispatch `formatEntry`, continuing toward the full config
formatter and loader.
