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

# Experiment 479: the config ColorList CLI parser (ColorList::parse_cli)

## Description

Continuing the config color value types, this experiment ports `ColorList`
(upstream `Config.ColorList`) — a comma-separated list of colors (used by e.g.
`background-image-*` tint lists). Its parser splits the input on commas, trims
and parses each entry via `Color::parse_cli`, caps the list at 64 entries, and
resets the list on each parse. The C mirror (`colors_c` / the
`ghostty_config_color_list_s` extern struct) and the `formatEntry` formatter
stay deferred.

## Upstream behavior

In `config/Config.zig`, `ColorList.parseCLI`:

```zig
pub const ColorList = struct {
    colors: std.ArrayListUnmanaged(Color) = .{},
    colors_c: std.ArrayListUnmanaged(Color.C) = .{},

    pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void {
        const input = input_ orelse return error.ValueRequired;
        if (input.len == 0) return error.ValueRequired;

        // Always reset on parse
        self.* = .{};

        // Split the input by commas and parse each color
        var it = std.mem.tokenizeScalar(u8, input, ',');
        var count: usize = 0;
        while (it.next()) |raw| {
            count += 1;
            if (count > 64) return error.InvalidValue;

            // Trim whitespace from each color value
            const trimmed = std.mem.trim(u8, raw, " \t");
            const color = try Color.parseCLI(trimmed);
            try self.colors.append(alloc, color);
            try self.colors_c.append(alloc, color.cval());
        }

        // If no colors were parsed, we need to return an error
        if (self.colors.items.len == 0) return error.InvalidValue;

        assert(self.colors.items.len == self.colors_c.items.len);
    }
    // ...
};
```

- A missing or empty value is `error.ValueRequired`.
- The list is reset (`self.* = .{}`) before parsing — each parse replaces the
  previous list.
- The input is split on `,` with `tokenizeScalar`, which **skips empty tokens**
  (so `"a,,b"` and `",a,"` yield only the non-empty segments).
- Each non-empty token is counted; the **65th** token (`count > 64`) is
  `error.InvalidValue`.
- Each token is whitespace-trimmed (`" \t"`) and parsed by `Color.parseCLI`,
  whose error propagates (so a token that trims to empty — e.g. a lone `" "` —
  is `error.InvalidValue` via `Color.parseCLI`'s hex fallback).
- If no colors were parsed (the list is empty — i.e. the input was only commas),
  it is `error.InvalidValue`.
- `colors_c` is the C mirror kept in lock-step for FFI (`cval`).

Upstream's tests: `"black,white"` → 2 colors; whitespace tolerance
(`"black, white"`, `"black , white"`, `" black , white "`) → 2 colors; `null` →
`error.ValueRequired`; `" "` → `error.InvalidValue`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The config `ColorList` (upstream `Config.ColorList`): a comma-separated list
/// of colors (1..=64). The `colors_c` C mirror and the `formatEntry` formatter
/// are ported in later slices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ColorList {
    pub colors: Vec<Color>,
}

impl ColorList {
    /// Parse a comma-separated color list (upstream `ColorList.parseCLI`): a
    /// missing or empty value is `ColorParseError::ValueRequired`; the list is
    /// reset, then each comma-separated token (empties skipped) is trimmed and
    /// parsed via [`Color::parse_cli`]; more than 64 colors, or an all-empty
    /// input, is `Invalid`.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        if input.is_empty() {
            return Err(ColorParseError::ValueRequired);
        }

        // Always reset on parse.
        self.colors.clear();

        let mut count: usize = 0;
        for raw in input.split(',').filter(|tok| !tok.is_empty()) {
            count += 1;
            if count > 64 {
                return Err(ColorParseError::Invalid);
            }
            let trimmed = raw.trim_matches(|c: char| c == ' ' || c == '\t');
            let color = Color::parse_cli(Some(trimmed))?;
            self.colors.push(color);
        }

        if self.colors.is_empty() {
            return Err(ColorParseError::Invalid);
        }
        Ok(())
    }
}
```

`parse_cli` mirrors upstream: the missing/empty `ValueRequired` guard, the
reset, the comma split skipping empty tokens (Zig's `tokenizeScalar`), the
`count > 64` cap, the per-token `" \t"` trim + `Color::parse_cli`, and the
empty-result `Invalid`. The error type is the shared `ColorParseError`
(`ValueRequired` for the missing/empty input; `Invalid` for the over-64 /
all-empty / bad-color cases, the last propagated from `Color::parse_cli`) —
upstream uses the same two error names (`error.ValueRequired` /
`error.InvalidValue`).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `ColorList` struct (the `colors` vector) and
  `ColorList::parse_cli` (upstream `ColorList.parseCLI`).
- **Faithful**: the missing/empty `ValueRequired` guard; the reset-on-parse; the
  comma tokenization that skips empty tokens; the 65th-token `Invalid` cap; the
  per-token `" \t"` trim and `Color::parse_cli`; the all-empty-input `Invalid` —
  exactly upstream's `parseCLI`. Mid-list a bad color returns `Invalid` and
  leaves the partially built list (as upstream returns mid-loop after its
  reset).
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; the allocator-backed
  `std.ArrayListUnmanaged(Color)` → `Vec<Color>`; `std.mem.tokenizeScalar(',')`
  → `split(',').filter(non-empty)`; the two upstream errors → `ColorParseError`
  (`ValueRequired` / `Invalid`).
- **Deferred**: the `colors_c` C mirror and `ColorList.cval` /
  `ghostty_config_color_list_s` extern struct (FFI), and `ColorList.formatEntry`
  (joins `Color.formatBuf` with commas; depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter. (Consumed by later
  slices; this experiment lands the value parser.)
- No C ABI/header/ABI-inventory change (internal Rust; the C mirror is
  deferred).

## Changes

1. `roastty/src/config/mod.rs`:
   - add the config `ColorList` struct (`colors: Vec<Color>`,
     `derive(Debug, Clone, Default, PartialEq, Eq)`).
   - add
     `ColorList::parse_cli(&mut self, input: Option<&str>) -> Result<(), ColorParseError>`.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` test: `"black,white"` → 2 colors (`{0,0,0}`,
     `{255,255,255}`); the whitespace cases (`"black, white"`,
     `"black , white"`, `" black , white "`) → 2 colors; `None` →
     `Err(ValueRequired)`, `""` → `Err(ValueRequired)`, `" "` → `Err(Invalid)`;
     plus: reset-on-parse (a second parse replaces the list, not appends); empty
     tokens skipped (`"black,,white"` and `",black,white,"` → 2 colors); the cap
     (64 colors OK, 65 → `Err(Invalid)`); and a bad color mid-list
     (`"black,nope"` → `Err(Invalid)`).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty color_list
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `ColorList::parse_cli` resets then parses the comma-separated,
  whitespace-trimmed colors (empties skipped, capped at 64), returning
  `ColorParseError::ValueRequired` on a missing/empty value and `Invalid` on
  over-64 / all-empty / bad-color — faithful to upstream's `parseCLI`;
- the tests pass (the upstream cases; the reset, empty-token, cap, and bad-color
  cases), and the existing tests still pass;
- the `colors_c` C mirror / `cval` / `formatEntry` and the broader config
  parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a list is parsed wrong (wrong split/trim, wrong cap,
empties not skipped, no reset), a missing/empty value does not error, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding and no Required/Recommended findings. It verified against the
vendored upstream: `None` and empty input return `ValueRequired` before the
reset (`Config.zig:5697`); resetting after the value-required guards is correct,
and `Vec::clear()` is a reasonable adaptation of `self.* = .{}` for the semantic
state; `split(',').filter(|tok| !tok.is_empty())` matches `tokenizeScalar`'s
empty-token skipping; the per-token space/tab-only trim and `Color::parse_cli`
delegation match upstream (`:5710`); reusing `ColorParseError` is acceptable
since the surfaced errors are exactly `ValueRequired` / `InvalidValue`; and
deferring `colors_c` / `cval` / `formatEntry` is the right scope.

- **Low (already in the test plan):** cover the two loop semantics not in
  upstream's minimal test set — empty-token skipping (`"black,,white"`,
  `",black,"`, `"black,"` parse only the non-empty tokens) and the `count > 64`
  cap returning `Invalid` on the 65th token (`Config.zig:5703`, `:5707`). The
  Changes/Tests section above already lists both; they are implemented.

Review artifacts:

- Prompt: `logs/codex-review/20260604-133103-d479-prompt.md` (design)
- Result: `logs/codex-review/20260604-133103-d479-last-message.md` (design)

## Result

**Result:** Pass

`ColorList::parse_cli` was added to `roastty/src/config/mod.rs` exactly as
designed — the missing/empty `ValueRequired` guard, the reset, the comma split
skipping empty tokens, the `count > 64` cap, the per-token `" \t"` trim +
`Color::parse_cli`, and the all-empty `Invalid`, reusing `ColorParseError`. The
new test `color_list_parse_cli_parses_comma_separated_colors` asserts the
upstream cases (`"black,white"`, the whitespace variants), plus the two
design-review-Low behaviors (empty-token skipping; the 64-item cap with the 65th
`Invalid`), the reset-on-parse, the missing/empty/whitespace/all-empty/bad-color
errors.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2959 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no findings**
(the design Low is resolved): `ColorList::parse_cli` preserves the upstream
ordering (missing/empty → `ValueRequired` before reset, then reset,
comma-tokenize skipping empties, per-token space/tab trim, `Color::parse_cli`,
cap at 64, reject all-empty); `Vec::clear()` is an appropriate semantic reset;
reusing `ColorParseError` is fine for the exposed `ValueRequired` / `Invalid`;
the expanded test covers the upstream cases plus empty-token skipping, reset,
empty/all-empty inputs, bad colors, and the 64-item cap; and deferring
`colors_c` / `cval` / `formatEntry` / the broader config parser remains properly
scoped. "Approved for the result commit."

Review artifacts:

- Prompt: `logs/codex-review/20260604-133348-r479-prompt.md` (result)
- Result: `logs/codex-review/20260604-133348-r479-last-message.md` (result)

## Conclusion

The `ColorList` config now parses: a comma-separated, whitespace-tolerant,
empty-skipping, 64-capped list of colors reusing `Color::parse_cli`. With
`Color`, `TerminalColor`, `BoldColor`, `Palette`, and `ColorList` all parsing,
the config color value types are largely covered on the parse side. The next
slice can move to a non-color config value type's `parseCLI` (e.g. `Duration`,
`WindowPadding`, or `MouseScrollMultiplier`) or port the color formatters once
the config `EntryFormatter` lands, continuing toward the per-field parser
dispatch and the full config loader.
