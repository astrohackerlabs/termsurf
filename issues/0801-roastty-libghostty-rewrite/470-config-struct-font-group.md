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

# Experiment 470: grow the Config struct with the font group

## Description

Continuing the incremental growth of the aggregating `Config` struct
(Experiments 461–469), this experiment adds the **font** group: the four
`font-style*` keys (`font_style`, `font_style_bold`, `font_style_italic`,
`font_style_bold_italic`, each `FontStyle`) and `font_shaping_break`
(`FontShapingBreak`). It is the first group to put the `String`-backed
`FontStyle` value type (Experiment 440) into `Config`, exercising the struct's
non-`Copy` `Clone`/`PartialEq` derive. It adds the five fields and their
upstream `Config`-field defaults to `Config` and its `Default`. The parser and
the rest of upstream `Config` stay deferred.

## Upstream behavior

In `config/Config.zig`, the font group's field defaults:

```zig
@"font-style": FontStyle = .{ .default = {} },
@"font-style-bold": FontStyle = .{ .default = {} },
@"font-style-italic": FontStyle = .{ .default = {} },
@"font-style-bold-italic": FontStyle = .{ .default = {} },
@"font-shaping-break": FontShapingBreak = .{},
```

Each `font-style*` key defaults to `.{ .default = {} }` (the `FontStyle`
`default` variant — use the font discovery's default style);
`font-shaping-break` defaults to `.{}` (the struct's own field defaults:
`cursor = true`).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
pub(crate) struct Config {
    // ... clipboard (461) … macOS-window (469) ...
    /// `font-style`.
    pub font_style: FontStyle,
    /// `font-style-bold`.
    pub font_style_bold: FontStyle,
    /// `font-style-italic`.
    pub font_style_italic: FontStyle,
    /// `font-style-bold-italic`.
    pub font_style_bold_italic: FontStyle,
    /// `font-shaping-break`.
    pub font_shaping_break: FontShapingBreak,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // ... earlier groups ...
            font_style: FontStyle::Default,
            font_style_bold: FontStyle::Default,
            font_style_italic: FontStyle::Default,
            font_style_bold_italic: FontStyle::Default,
            font_shaping_break: FontShapingBreak::default(),
        }
    }
}
```

The defaults are upstream's Config-field defaults: each `font-style*` is
`FontStyle::Default` (the `.{ .default = {} }` literal), and
`font-shaping-break` is `FontShapingBreak::default()` (the `.{}` literal —
`cursor = true`).

## Scope / faithfulness notes

- **Ported (bridged)**: the font field group of the aggregating `Config` struct
  (upstream `config.Config`) — the five fields and their `Default`.
- **Faithful**: the four `font-style*` fields use the already-ported `FontStyle`
  type (Experiment 440) and the `font-shaping-break` field uses
  `FontShapingBreak` (Experiment 437); their `Default` values match upstream's
  Config-field defaults (each `FontStyle::Default`;
  `FontShapingBreak::default()`).
- **Faithful adaptation**: the `.{ .default = {} }` literal maps to
  `FontStyle::Default`, and `.{}` to `FontShapingBreak::default()` (its own
  field defaults, `cursor = true`). `FontStyle` is `String`-backed and not
  `Copy`, so `Config` continues to derive `Clone`/`PartialEq` (not `Copy`) —
  this is the first group to require that non-`Copy` property. The struct
  continues to grow one coherent field group per experiment.
- **Deferred**: the rest of upstream `Config`'s fields (added group by group in
  later slices), the parser, the `changeConfig` machinery, and the
  conditional-config system. (Consumed by later slices; this experiment grows
  the struct with the font group.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add the five fields `font_style: FontStyle`, `font_style_bold: FontStyle`,
     `font_style_italic: FontStyle`, `font_style_bold_italic: FontStyle`,
     `font_shaping_break: FontShapingBreak` to `Config`, and their defaults
     (each `FontStyle::Default`; `FontShapingBreak::default()`) to the `Default`
     impl.
2. Tests (in `config/mod.rs`):
   - extend the `Config::default()` assertion for the new fields: the four
     `font_style*` are `FontStyle::Default` and `font_shaping_break` is
     `FontShapingBreak::default()`; the existing group defaults still hold.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_default
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config` gains the five font fields, and `Config::default()` sets their
  upstream defaults (each `font-style*` `FontStyle::Default`;
  `font-shaping-break` `FontShapingBreak::default()`) while the earlier group
  defaults still hold — a faithful partial of upstream's `Config`;
- the tests pass (the new defaults; the existing defaults), and the existing
  tests still pass;
- the rest of upstream `Config` and the parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a default is wrong, a field uses the wrong type, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the four `font-style*`
defaults are all `.{ .default = {} }`, so `FontStyle::Default` is the correct
Rust value (`Config.zig:186`);
`font_shaping_break = FontShapingBreak::default()` correctly maps `.{}` because
the struct's own default is `cursor = true` (`Config.zig:374` / `:8563`);
`FontStyle` is correctly non-`Copy` (upstream's string-backed `name` payload,
`Config.zig:8431`), validating `Config` as `Clone` but not `Copy`; the font
group is coherent (four style selectors plus the shaping-break config); and the
test plan is adequate (assert all five new defaults and keep the existing groups
covered as `Default` grows).

Review artifacts:

- Prompt: `logs/codex-review/20260604-123828-d470-prompt.md` (design)
- Result: `logs/codex-review/20260604-123828-d470-last-message.md` (design)

## Result

**Result:** Pass

The `Config` struct now carries the font field group.

- `roastty/src/config/mod.rs`: `Config` gains `font_style`, `font_style_bold`,
  `font_style_italic`, `font_style_bold_italic` (each `FontStyle`), and
  `font_shaping_break: FontShapingBreak`; `Config::default()` sets their
  upstream Config-field defaults — each `font_style*` is `FontStyle::Default`,
  and `font_shaping_break` is `FontShapingBreak::default()` (`cursor = true`).

Test (in `config/mod.rs`): `config_default_clipboard_group` extended to assert
the four `font_style*` are `FontStyle::Default` and `font_shaping_break` is
`FontShapingBreak::default()` alongside the nine prior groups' defaults; the
modified-config inequality and the `Clone`/`PartialEq` round-trip remain.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2952 passed, 0 failed (no regressions; the existing
  `config_default` test was extended).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The aggregating `Config` struct now holds ten field groups — clipboard (461)
through macOS-window (469), plus font — thirty-seven fields total, and the first
`String`-backed field (`FontStyle`), which validates the `Config`
`Clone`-not-`Copy` derive (and complements the float field added in Experiment
466 for the `PartialEq`-not-`Eq` derive). The parser, the `changeConfig`
machinery, the conditional-config system, and the remaining upstream `Config`
fields stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the four `font_style*` fields default to
`FontStyle::Default` (faithfully mapping upstream `.{ .default = {} }`);
`font_shaping_break` defaults through `FontShapingBreak::default()`, preserving
`cursor = true`; adding `FontStyle` validates the aggregate's `Clone`-not-`Copy`
shape; and extending the existing `Config::default()` test is adequate and keeps
all prior groups covered. No public C ABI/header impact; nothing needed to
change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-124028-r470-prompt.md` (result)
- Result: `logs/codex-review/20260604-124028-r470-last-message.md` (result)
