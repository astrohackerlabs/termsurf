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

# Experiment 260: Color glyph detection — ColorState (sbix)

## Description

The remaining `renderGlyph` branch is the **color/sbix** path (emoji). Before
the colored render can be written, the face must know whether a glyph is
colored. This experiment ports upstream's `ColorState` and the `hasColor` /
`isColorGlyph` queries (`font/face/coretext.zig` lines 258–267, 892–968), scoped
to **sbix** detection — which covers Apple Color Emoji, the macOS color font.
The colored **render** (depth-4 P3 RGBA atlas) is the next experiment; SVG-table
detection is deferred.

### Upstream behavior (`font/face/coretext.zig`)

- `Face.color: ?ColorState`, built in `initFont` when `traits.color_glyphs` is
  set (line 103).
- `ColorState.init` (lines 905–945):
  `sbix = copyTable("sbix") exists && length > 0`; plus an SVG table parse
  (deferred here).
- `ColorState.isColorGlyph` (lines 951–967): cast the glyph id to `u16` (else
  `false`); if `sbix` return `true`; else check SVG `hasGlyph`; else `false`.
- `Face.hasColor` (lines 258–261): `color != null`.
- `Face.isColorGlyph` (lines 263–267): `color orelse false; c.isColorGlyph(id)`.

### Faithful adaptation (the symbolic-traits gate)

Upstream gates `ColorState` creation on the `color_glyphs` **symbolic trait**.
The objc2-core-text 0.3.2 binding for `CTFontGetSymbolicTraits` is unusable (the
`CTFontSymbolicTraits` type name is mangled to a placeholder in the generated
bindings). The trait, however, is only an **optimization** to avoid copying the
sbix/SVG tables for non-color fonts — `isColorGlyph`'s actual logic is
sbix/SVG-based. So this port builds `ColorState` directly from **sbix table
presence**: a font with a non-empty `sbix` table is a color font. This is
behaviorally equivalent for sbix fonts (Apple Color Emoji has an `sbix` table;
text fonts like Menlo do not) at the cost of one extra table copy per face
construction. The deviation is documented here per the issue's hybrid policy.

### Rust mapping (`roastty/src/font/face/coretext.rs`)

1. `ColorState { sbix: bool }` with
   `fn is_color_glyph(&self, _glyph: u16) -> bool { self.sbix }` (the `u16` cast
   is implicit — our glyph ids are already `u16`; SVG `hasGlyph` is deferred, so
   non-sbix returns `false`).
2. `Face` gains `color: Option<ColorState>`. `Face::new` detects the `sbix`
   table (via the existing `copy_table(b"sbix")` pattern, treating a present
   non-empty table as `sbix = true`) and sets
   `color = Some(ColorState { sbix: true })` when present, else `None`.
   `new_synthetic_bold` inherits this through `new`.
3. `Face::has_color(&self) -> bool` (`self.color.is_some()`) and
   `Face::is_color_glyph(&self, glyph: u16) -> bool`
   (`self.color.map_or(false, |c| c.is_color_glyph(glyph))`).

### Scope / faithfulness notes

- **Deferred** (later experiments): SVG-table detection (`opentype::SVG` +
  `hasGlyph`, so SVG-only color fonts aren't yet detected) and the colored
  **render** (the depth-4 P3 RGBA bitmap context, the
  `byte_order_32_little | premultiplied_first` info, the RGBA atlas write, and
  the sbix whole-pixel quantization). This experiment is detection only —
  `render_glyph` is unchanged.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/face/coretext.rs`:
   - Add `ColorState { sbix: bool }` + `is_color_glyph`.
   - Add `color: Option<ColorState>` to `Face`; detect the `sbix` table in
     `new`.
   - Add `Face::has_color` and `Face::is_color_glyph`.
2. New tests (live CoreText, macOS):
   - `text_font_has_no_color`: `Face::new("Menlo", 32.0).has_color()` is
     `false`, and `'M'` is not a color glyph.
   - `emoji_font_has_color`: `Face::new("Apple Color Emoji", 32.0).has_color()`
     is `true`, and the glyph for an emoji code point (e.g. `😀` `U+1F600`) is a
     color glyph. (Guard: assert the emoji glyph id resolved to non-zero so a
     fallback didn't silently swap fonts; if the system lacks the font this is a
     real environment problem, not a flaky test.)
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty face
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Face` carries `color: Option<ColorState>`, detected from the `sbix` table in
  `new`, and `has_color`/`is_color_glyph` report it faithfully;
- an sbix color font (Apple Color Emoji) reports `has_color() == true` and its
  emoji glyphs as color glyphs; a text font (Menlo) reports `false`;
- `render_glyph` and the existing tests are unchanged (detection only);
- the symbolic-traits → sbix-table adaptation is documented;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if Apple Color Emoji cannot be loaded by name in
the test environment (the detection logic would still be correct, but the live
assertion couldn't run — it would fall back to asserting the `sbix` table is
detected for a font known to carry one).

The experiment **fails** if color detection is wrong for sbix fonts or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-212517-649674-prompt.md`
- Result: `logs/codex-review/20260602-212517-649674-last-message.md`

Codex confirmed the sbix-table-presence adaptation is acceptable and
behaviorally equivalent for sbix fonts (non-empty `sbix` is the operative signal
once `ColorState` exists, and `isColorGlyph` treats any glyph as color when
`sbix` is true), that deferring SVG detection is a clearly-scoped gap rather
than a correctness issue for Apple Color Emoji, and that the `u16` glyph-id
shape matches the Rust CoreText path. It flagged one implementation note: the
emoji glyph must be resolved from its **UTF-16 surrogate pair** (`U+1F600` is
outside the BMP), so the test encodes the code point to two `u16` code units and
takes the first resolved glyph (guarded non-zero).

## Result

**Result:** Pass

`ColorState { sbix: bool }` (with `is_color_glyph` returning `self.sbix`) and
`Face.color: Option<ColorState>` landed. `Face::new` detects color via
`detect_color`, which marks the font as color when the `sbix` table is present
and non-empty. `Face::has_color` and `Face::is_color_glyph` expose it.
`new_synthetic_bold` inherits the detection through `new`.

Tests (live CoreText):

- `text_font_has_no_color` — Menlo reports `has_color() == false` and `'M'` is
  not a color glyph.
- `emoji_font_has_color` — Apple Color Emoji reports `has_color() == true`; the
  `U+1F600` glyph (resolved from its UTF-16 surrogate pair, guarded non-zero) is
  a color glyph.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty face` → 28 passed, 0 failed.
- `cargo test -p roastty` → 2379 passed, 0 failed (no regressions; +2).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

Color detection works for sbix fonts. The next experiment writes the colored
**render** path in `render_glyph`: when `is_color_glyph(glyph)`, use a depth-4
P3 RGBA bitmap context (`CGColorSpace::new_with_name(displayP3)`,
`byte_order_32_little | premultiplied_first`), an RGBA atlas (the `Atlas`
already supports `Bgra`), the sbix whole-pixel position/size quantization, and
the synthetic-bold/thicken suppression for sbix. That render is the larger half
of the color path; SVG-table detection (for SVG-only color fonts) and the
`opentype::SVG` parser remain deferred. Beyond `renderGlyph`: the
Collection/CodepointResolver, the shaper, and the Nerd Font attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-212746-889079-prompt.md`
- Result: `logs/codex-review/20260602-212746-889079-last-message.md`

Codex confirmed `copy_table(b"sbix").is_some_and(|d| !d.is_empty())` matches
upstream's `length > 0` check, that the two-step `Face` init is fine (the
`CTFont` is retained before `detect_color` runs), that `has_color` /
`is_color_glyph` match the scoped upstream behavior for sbix fonts, that
`new_synthetic_bold` preserves color by starting from `Face::new`, and that the
tests cover both the Menlo negative and the Apple Color Emoji positive (with
proper surrogate-pair glyph resolution).
