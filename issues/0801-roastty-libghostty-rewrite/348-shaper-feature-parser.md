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

# Experiment 348: the feature-string parser

## Description

Experiment 347 added the `Feature` type and applied the hardcoded
`default_features`. User-configured features arrive as **strings** (HarfBuzz
syntax: `"liga"`, `"-calt"`, `"cv01=2"`, `"kern off"`, …). This experiment ports
upstream's `Feature.fromString`/`FeatureList.fromString` — the error-tolerant
state-machine parser in `shaper/feature.zig` — turning feature strings into
`Feature`s. Threading the parsed features through shaping (`Options` →
`shape_run`) is a follow-up; this experiment is the **parser** in isolation
(self-contained pure logic, exhaustively unit-tested).

## Upstream behavior (`shaper/feature.zig`)

`Feature.fromReader` is a byte state machine (a subset of HarfBuzz's
`hb_feature_from_string`), reading until end-of-stream or `,` (EOF treated as
`,`), tolerant of bad input (on error, fast-forward to the next `,` and yield
`null`):

- **start**: skip ` `/`\t`; `,` → `null` (empty); `+` → value `1`, → tag; `-` →
  value `0`, → tag; `"`/`'` → → tag; else → first tag byte, → tag.
- **tag**: `,` → `null` (interrupted); `"`/`'` → ignore; else → append; at 4
  bytes → space.
- **space**: ` `/`\t`/`"`/`'` → ignore; `=` → error if a `+`/`-` value already
  set, else ignore; `,` → if no value, value `1`; done; `0`–`9` → error if value
  set, else start int; `o`/`O` → error if value set, else bool; else → error.
- **int**: `,` → done; `0`–`9` → `value = value*10 + d` (overflow → error); else
  → error.
- **bool** (`on`/`off`): `,` → `null`; `n`/`N` → (value must be unset) value
  `1`, done; `f`/`F` → first `f` sets value `0`, second `f` → done; else →
  error.
- **done**: skip ` `/`\t` until `,`; anything else → error.
- **error**: skip to the next `,`, return `null`.

A parsed feature requires a complete 4-byte tag and a resolved value.
`FeatureList.fromString` loops `fromReader` over a comma-separated string,
appending each parsed feature and dropping the invalid ones.

Upstream's own tests (mirrored below) pin the behavior, e.g.
`"kern"`/`"kern on"`/ `"+kern"`/`"\"kern\" = 1"` → `kern=1`;
`"kern off"`/`"-'kern'"`/`"\"kern\"=0"` → `kern=0`; `"aalt=2"`/`"'aalt' 2"` →
`aalt=2`; and the invalid `"aalt=2x"`, `"toolong"`, `"sht"`, `"-kern 1"`,
`"-kern on"`, `"aalt=o"`, `"aalt=ofn"` → `null`.

## Rust mapping (`roastty/src/font/shape.rs`)

- Implement the state machine over a byte cursor `(&[u8], &mut usize)`,
  mirroring upstream's `readByte() catch ','` (EOF yields `,` without advancing)
  and `skipUntilDelimiterOrEof(',')`:

  ```rust
  impl Feature {
      /// Parse a single feature from `s` (HarfBuzz-subset syntax), or `None`.
      pub(crate) fn from_str(s: &str) -> Option<Feature> {
          let mut pos = 0;
          Feature::parse_one(s.as_bytes(), &mut pos)
      }

      /// Parse one feature starting at `*pos`, advancing past it (and its
      /// trailing `,`). `None` on invalid syntax (advancing to the next `,`).
      fn parse_one(bytes: &[u8], pos: &mut usize) -> Option<Feature> { … }
  }

  /// Parse a comma-separated feature list, dropping invalid entries. Faithful
  /// port of `FeatureList.fromString`.
  pub(crate) fn parse_features(s: &str) -> Vec<Feature> {
      let bytes = s.as_bytes();
      let mut pos = 0;
      let mut out = Vec::new();
      while pos < bytes.len() {
          if let Some(f) = Feature::parse_one(bytes, &mut pos) {
              out.push(f);
          }
      }
      out
  }
  ```

  The state machine uses an enum of states (`Start`/`Tag`/`Space`/`Int`/`Bool`/
  `Done`/`Err`) and the same transitions, with `read_byte` (advance, EOF → `,`)
  and `skip_to_boundary` helpers. A `value: Option<u32>` and a `tag: [u8; 4]`
  with a length count reproduce upstream's locals; success requires
  `tag_len == 4` and `value` set.

## Scope / faithfulness notes

- **Ported**: the `Feature.fromString` state machine and
  `FeatureList.fromString` list parser — the HarfBuzz-subset feature-string
  syntax with upstream's exact tolerance and error recovery.
- **Faithful**: EOF is treated as `,` (so a trailing feature with no comma still
  parses); error recovery advances through the next `,`; the `on`/`off` keyword
  handling; quote-mark skipping; the `+`/`-`-then-value-conflict errors.
- **Faithful improvement (int overflow)**: upstream guards only the multiply
  (`std.math.mul`) and leaves the digit add unchecked (which would wrap in
  release or panic in debug). roastty uses a single checked accumulation
  (`value.checked_mul(10).and_then(|v| v.checked_add(d))`), treating **either**
  overflow as invalid (`None`) — never wrapping or panicking. This only differs
  from upstream for inputs that overflow on the final add (e.g.
  `"aalt=4294967296"`), which upstream does not test; roastty rejects them.
- **Deferred** (follow-up): threading the parsed features (a user
  `Options.features` string → `parse_features` → merged with `default_features`)
  through `shape_run`; the `features_no_default` variant; the special-font path;
  the `Shaper` struct + `RunIterator`. (The parser is consumed by tests now; the
  font module's `#![allow(dead_code)]` covers the not-yet-wired path, matching
  the existing ported-ahead-of-consumer primitives.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/shape.rs`: add `Feature::from_str`, `Feature::parse_one`
   (the state machine), and `parse_features`.
2. Tests (in `shape.rs`), mirroring upstream's `Feature.fromString` and
   `FeatureList.fromString` tests:
   - `feature_from_string_boolean_on`: `"kern"`, `"kern, "`, `"kern on"`,
     `"kern on, "`, `"+kern"`, `"+kern, "`, `"\"kern\" = 1"`, `"\"kern\" = 1, "`
     → `kern = 1`.
   - `feature_from_string_boolean_off`: `"kern off"`, `"-'kern'"`,
     `"\"kern\"=0"` (and trailing-comma variants) → `kern = 0`.
   - `feature_from_string_numeric`: `"aalt=2"`, `"'aalt' 2"` (and variants) →
     `aalt = 2`.
   - `feature_from_string_invalid`: `"aalt=2x"`, `"toolong"`, `"sht"`,
     `"-kern 1"`, `"-kern on"`, `"aalt=o,"`, `"aalt=ofn,"` → `None`.
   - `feature_list_from_string`: upstream's combined string parses to the exact
     `[kern=1 ×4, kern=0 ×3, aalt=2 ×2, last=1]` (invalid entries dropped, the
     final no-comma element included).
   - `feature_from_string_overflow`: `"aalt=4294967295"` → `aalt = u32::MAX`;
     `"aalt=4294967296"` → `None` (the checked accumulation rejects overflow).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty feature
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Feature::from_str` and `parse_features` reproduce upstream's state machine
  and list parsing, with the exact tolerance and error recovery;
- the boolean/numeric/invalid and list tests (mirroring upstream's) pass, and
  the existing tests still pass;
- the `Options` threading, the special-font path, and the `Shaper`/`RunIterator`
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the parser diverges from upstream on any of the
mirrored test vectors (wrong tolerance, wrong value resolution, missing error
recovery), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the cursor model is faithful given the invariants:
`read_byte` returns `b','` at EOF without advancing (preserving the
trailing-item behavior, so `"last"` parses and leaves `pos == len`);
`parse_features` looping `while pos < bytes.len()` matches upstream's loop,
dropping invalid entries and continuing; error recovery should advance through
the next `,` (or to EOF) — which the `skip_to_boundary` helper does; and the
`tag_len == 4 && value.is_some()` postcondition holds (short tags hit `,`/EOF in
`Tag` → `None`; a complete tag with no explicit value defaults to `1` only in
`Space` on `,`/EOF). It verified the transitions match upstream, including the
`=`-after-`+`/`-` conflict, `on`/`off` handling, the two-`f` consumption for
`off`, ambiguous bool forms → `None`, and `Done` rejecting non-whitespace before
the delimiter.

Two guidance points, both folded in:

- **Integer overflow:** use one checked expression
  (`checked_mul(10).and_then(checked_add(d))`) so both multiply and add overflow
  become invalid — adopted (a faithful improvement over upstream's mul-only
  guard; see the scope note). The `"aalt=4294967295"`/`"aalt=4294967296"` test
  vectors were added to pin it.
- **Error recovery** consuming the next `,` (not just stopping before it) is
  closer to upstream — the design's `skip_to_boundary` does this.

Review artifacts:

- Prompt: `logs/codex-review/20260603-142244-237266-prompt.md` (design)
- Result: `logs/codex-review/20260603-142244-237266-last-message.md` (design)

## Result

**Result:** Pass

The feature-string parser is ported.

- `roastty/src/font/shape.rs`: `Feature::from_str` / `Feature::parse_one`
  implement the HarfBuzz-subset state machine (`FeatureState` enum
  `Start`/`Tag`/`Space`/ `Int`/`Bool`/`Done`/`Err`, a labeled outer loop with
  per-state read loops, the `feature_read_byte` EOF-as-`,` helper, and
  `feature_skip_to_boundary`). The integer value uses a single checked
  accumulation (`checked_mul(10).and_then(checked_add(d))`). `parse_features`
  loops `parse_one` over a comma-separated string, dropping invalid entries.
  Success requires `tag_len == 4` and a resolved value.

Tests (mirroring upstream's `Feature.fromString`/`FeatureList.fromString`):
`feature_from_string_boolean_on` (`kern`/`kern on`/`+kern`/`"kern" = 1` →
`kern=1`), `feature_from_string_boolean_off` (`kern off`/`-'kern'`/`"kern"=0` →
`kern=0`), `feature_from_string_numeric` (`aalt=2`/`'aalt' 2` → `aalt=2`),
`feature_from_string_invalid` (`aalt=2x`, `toolong`, `sht`, `-kern 1`,
`-kern on`, `aalt=o`, `aalt=ofn` → `None`), `feature_from_string_overflow`
(`4294967295` → `u32::MAX`, `4294967296` → `None`), and
`feature_list_from_string` (the combined string →
`[kern=1 ×4, kern=0 ×3, aalt=2 ×2, last=1]`).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2768 passed, 0 failed (+6, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

roastty can now parse HarfBuzz-syntax font-feature strings into `Feature`s,
matching upstream's tolerant state machine on every mirrored test vector. The
`Feature` type (Exp 347) and its parser (Exp 348) are both in place.

The follow-up: thread the parsed features through shaping — a user
`Options.features` string → `parse_features` → merged with `default_features`
(plus the `features_no_default` variant) into `shape_run`. Then the special-font
fast path and the `Shaper` struct + `RunIterator` remain.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It verified the state transitions match upstream's
`Feature.fromReader`/`FeatureList.fromString` — including EOF-as-`,` without
advancing, error recovery consuming through the next `,`, the `+`/`-` conflicts,
`on`/`off` with the two-`f` handling, `Done`'s whitespace-only behavior, and
dropping invalid entries while continuing the list. It confirmed the
`tag[tag_len]` write is safe under the state invariant (`Tag` is entered with
`tag_len <= 1` and exits to `Space` at `tag_len == 4`, so no write occurs at
`tag_len == 4`); that the checked-both overflow is the documented deliberate
improvement (rejecting final-add overflow rather than wrapping/panicking); and
that the `tag_len == 4 && value` postcondition is correct. It also ran
`cargo test -p roastty feature` (9 passed, 0 failed).

Review artifacts:

- Result review: `logs/codex-review/20260603-142738-035676-last-message.md`
