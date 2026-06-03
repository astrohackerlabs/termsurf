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

# Experiment 346: the score's variation-axis derivation

## Description

`Descriptor::score` refines its bold/italic guess from the candidate font's
`head` (`macStyle`) and `OS/2` (`fsSelection`) tables, but the
**variation-axis** refinement — deriving bold/italic from a _variable_ font's
`wght`/`ital`/`slnt` instance values — is deferred (see the comment in `score`:
"The variation-axis derivation, which overwrites these for variable fonts, is a
later experiment"). This experiment ports that block (upstream `Score.score`
lines 722–776): when the candidate is a variable font, its current axis values
**overwrite** the table-derived bold/italic, since the instance is
authoritative.

## Upstream behavior (`discovery.zig` `Score.score`)

```zig
if (font.copyAttribute(.variation_axes)) |axes| variations: {
    const values = font.copyAttribute(.variation) orelse break :variations;
    var ital_seen = false;
    for (0..axes.getCount()) |i| {
        const dict = axes.getValueAtIndex(Dictionary, i);
        const cf_id   = dict.getValue(…, Key.identifier.key()).?;     // CFNumber
        const cf_name = dict.getValue(…, Key.name.key()).?;            // CFString
        const cf_def  = dict.getValue(…, Key.default_value.key()).?;   // CFNumber
        const name_str = cf_name.cstring(&buf, .utf8) orelse "";
        var def: f64 = 0; _ = cf_def.getValue(.double, &def);
        var val: f64 = def;
        if (values.getValue(Number, cf_id)) |cf_val| _ = cf_val.getValue(.double, &val);

        if (mem.eql(u8, "wght", name_str)) { is_bold = val > 600; continue; }
        if (mem.eql(u8, "ital", name_str)) { is_italic = val > 0.5; ital_seen = true; continue; }
        if (!ital_seen and mem.eql(u8, "slnt", name_str)) { is_italic = val <= -5.0; continue; }
    }
}
```

For each variation axis: read its **name**, its **default value**, and the
instance's **value** (from the `kCTFontVariationAttribute` dictionary keyed by
the axis identifier, falling back to the default). Then:

- **`wght`** → `is_bold = value > 600` (a subjective bold threshold);
- **`ital`** → `is_italic = value > 0.5`, and mark `ital_seen` (an explicit
  italic axis wins over slant);
- **`slnt`** (only if no `ital` axis was seen) → `is_italic = value <= -5.0`
  (more than a 5° clockwise slant counts as italic).

These **overwrite** (not OR) the table-derived flags — the variable instance is
authoritative. The axis name from `kCTFontVariationAxesAttribute` is _not
localized_, so the raw tag strings (`"wght"`/`"ital"`/`"slnt"`) match.

## Rust mapping (`roastty/src/font/discovery.rs`)

- Extract the threshold logic as a pure, testable free function:
  ```rust
  /// Refine `(is_bold, is_italic)` from a variable font's ordered axis
  /// `(name, value)` pairs. Faithful port of upstream's `wght`/`ital`/`slnt`
  /// rules: each match overwrites the flag; an explicit `ital` axis suppresses a
  /// later `slnt`.
  fn derive_style_from_axes(
      mut is_bold: bool,
      mut is_italic: bool,
      axes: &[(String, f64)],
  ) -> (bool, bool) {
      let mut ital_seen = false;
      for (name, val) in axes {
          match name.as_str() {
              "wght" => is_bold = *val > 600.0,
              "ital" => {
                  is_italic = *val > 0.5;
                  ital_seen = true;
              }
              "slnt" if !ital_seen => is_italic = *val <= -5.0,
              _ => {}
          }
      }
      (is_bold, is_italic)
  }
  ```
- Add `variation_axis_pairs(font: &CTFont) -> Vec<(String, f64)>` that reads the
  `kCTFontVariationAxesAttribute` array and the `kCTFontVariationAttribute`
  values dictionary, and for each axis dictionary collects
  `(name, value-or-default)` — using the existing `dict.value(key_ptr)` →
  `*const c_void` idiom (as in `symbolic_traits`), `CFString::to_string` for the
  name, and `CFNumber::as_f64` for the default and instance values (the instance
  value is looked up in the values dict keyed by the axis identifier
  `CFNumber`). Returns empty when the font has no variation axes or no variation
  values (upstream's two `break :variations`).
- In `score`, after the `OS/2` block and before `s.bold`/`s.italic`:
  ```rust
  let pairs = variation_axis_pairs(&font);
  if !pairs.is_empty() {
      (is_bold, is_italic) = derive_style_from_axes(is_bold, is_italic, &pairs);
  }
  ```
  and drop the "later experiment" note from the doc comment.

## Scope / faithfulness notes

- **Ported**: the variation-axis bold/italic derivation in `Score.score` — the
  `wght`/`ital`/`slnt` thresholds, the `ital_seen` suppression of `slnt`, and
  the **overwrite** (the variable instance is authoritative over the
  `head`/`OS/2` tables).
- **Faithful**: the axis name is read from `kCTFontVariationAxesAttribute` (the
  non-localized variant, so the raw `"wght"`/`"ital"`/`"slnt"` tags match, as
  upstream relies on); the instance value falls back to the axis default when
  the variation dictionary lacks an entry.
- **Deferred** (unchanged): the special-font fast path, the `Shaper` struct +
  `RunIterator`. With this, the discovery `score()` is complete relative to
  upstream.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/discovery.rs`: import the variation attribute/axis keys;
   add `derive_style_from_axes` and `variation_axis_pairs`; call them in `score`
   between the `OS/2` derivation and the `s.bold`/`s.italic` assignment; update
   the `score` doc comment.
2. Tests (in `discovery.rs`):
   - `derive_style_from_axes_thresholds`: a focused unit test of the pure helper
     — `wght = 700` → bold; `wght = 400` **overwrites** a true input to
     not-bold; `ital = 1.0` → italic and suppresses a following `slnt = -3.0`
     (stays italic); `slnt = -10.0` (no `ital`) → italic; `slnt = -3.0` → not
     italic; an `ital = 0.0` after a `slnt = -10.0` **overwrites** back to
     not-italic; an unknown axis is ignored.
   - `score_non_variable_unchanged` (smoke): scoring a non-variable font (Menlo)
     still yields the table/symbolic-trait result — `variation_axis_pairs`
     returns empty (no axes), so the derivation is skipped. (Asserts the
     existing bold/italic scoring is unchanged; the variable-font runtime path
     needs a variable font, a documented limitation as in Experiment 345.)
   - The existing `score`-based tests still pass unchanged.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty derive_style
cargo test -p roastty score
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `score` reads the candidate's variation axes and overwrites the bold/italic
  guess via `derive_style_from_axes` for variable fonts, faithful to upstream's
  `wght`/`ital`/`slnt` thresholds and `ital_seen` suppression;
- the threshold unit test and the non-variable smoke test pass, and the existing
  `score` tests still pass unchanged;
- the special-font path and the `Shaper`/`RunIterator` stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if no reliably-present variable font is available
to prove a runtime axis-derived flip (the threshold logic is fully covered by
the unit test, and the CF reading is verified by faithfulness to upstream).

The experiment **fails** if the thresholds, the `ital_seen` suppression, the
overwrite semantics, or the axis/value reading diverge from upstream, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the helper logic is faithful to the upstream block
(`wght > 600.0`, `ital > 0.5`, `slnt <= -5.0`, each matching axis
**overwriting** rather than OR-ing); the `ital_seen` behavior is correct (an
`ital` axis suppresses only _later_ `slnt` axes, while a preceding `slnt` can be
overwritten by a later `ital`), and the proposed order-sensitive vectors cover
those cases. Reading the axis **name** from `kCTFontVariationAxesAttribute` is
faithful (upstream compares the name against `"wght"`/`"ital"`/`"slnt"`, and
that attribute's names are non-localized); the value lookup in
`kCTFontVariationAttribute` by the identifier `CFNumber` with fallback to the
axis default matches upstream's control flow; and returning empty when axes or
values are absent correctly preserves the table-derived path. Placement after
`head`/`OS/2` and before assigning `s.bold`/`s.italic` is right (the variable
instance becomes authoritative only after the table-derived guess is built).
Residual note (accepted): the CoreText attribute-reading path is smoke-covered
only, absent a stable variable font — the documented limitation, as in
Experiment 345.

Review artifacts:

- Prompt: `logs/codex-review/20260603-140008-295893-prompt.md` (design)
- Result: `logs/codex-review/20260603-140008-295893-last-message.md` (design)
