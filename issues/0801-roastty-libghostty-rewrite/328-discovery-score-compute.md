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

# Experiment 328: font discovery — computing the score

## Description

With the `Score` value type in place (Experiment 327), this experiment ports the
function that **computes** a score for a candidate descriptor: upstream
`CoreText.Score.score(desc, ct_desc)` loads the candidate font and fills in the
ranking fields. This slice ports the **font-loaded and symbolic-trait** fields —
`glyph_count`, `codepoint` (does the font have the requested codepoint), and
`monospace`/`bold`/`italic` from the descriptor's symbolic traits. The richer
bold/italic derivation (the `head`/`OS/2`/variation tables) and the style
exact/fuzzy match are the next experiments.

## Upstream behavior (`discovery.zig` `Score.score`)

```zig
fn score(desc: *const Descriptor, ct_desc: *FontDescriptor) Score {
    var self: Score = .{};
    const font = Font.createWithFontDescriptor(ct_desc, 12) catch return self; // 0 if unloadable
    self.glyph_count = cast(u16, font.getGlyphCount()) orelse maxInt(u16);      // clamp
    if (desc.codepoint > 0) {
        // UTF-32 → UTF-16, then getGlyphsForCharacters → present?
        self.codepoint = font.getGlyphsForCharacters(unichars, glyphs);
    }
    const symbolic_traits = ct_desc.copyAttribute(.traits) … kCTFontSymbolicTrait …;
    self.monospace = symbolic_traits.monospace;
    var is_bold = symbolic_traits.bold; var is_italic = symbolic_traits.italic;
    // … head/OS2/variation refinement … (deferred)
    self.bold = desc.bold == is_bold;
    self.italic = desc.italic == is_italic;
    // … style exact/fuzzy … (deferred)
    return self;
}
```

A font that fails to load scores all-zero (we never want a font we can't load).
`glyph_count` is clamped to `u16::MAX`. `codepoint` is only computed when the
descriptor seeks one. `monospace` comes straight from the symbolic traits;
`bold`/`italic` are whether the font's bold/italic-ness **matches the request**
(`desc.bold == is_bold`), where `is_bold`/`is_italic` start from the symbolic
traits (and are refined later — deferred here).

## Rust mapping (`roastty/src/font/discovery.rs`)

- `pub(crate) fn score(&self, ct_desc: &CTFontDescriptor) -> Score` on
  `Descriptor`:
  - Load the font: `CTFont::with_font_descriptor(ct_desc, 12.0, null)`. (The
    binding's constructor does not fail; an unloadable descriptor is not modeled
    as a fallible load here — a faithfulness note, since `with_font_descriptor`
    is non-`Option` in objc2 0.3.)
  - `glyph_count`: `font.glyph_count()` (`CFIndex`/`isize`), clamped to `u16`
    (`>= u16::MAX → u16::MAX`, negative → `0`).
  - `codepoint`: when `self.codepoint > 0`, UTF-16-encode the scalar and call
    `font.glyphs_for_characters` (the existing `Face::glyph_index`
    surrogate-pair pattern: `false` from CoreText ⇒ not present).
  - symbolic traits: read `ct_desc.attribute(kCTFontTraitsAttribute)` →
    `CFDictionary` → `kCTFontSymbolicTrait` `CFNumber` → `CTFontSymbolicTraits`
    (absent ⇒ empty traits). `monospace = traits.contains(TraitMonoSpace)`;
    `is_bold = traits.contains(TraitBold)`;
    `is_italic = traits.contains(TraitItalic)`.
  - `bold = self.bold == is_bold`; `italic = self.italic == is_italic`.
  - `fuzzy_style`/`exact_style` stay `0`/`false` (the style match is deferred).
  - A small `symbolic_traits(ct_desc) -> CTFontSymbolicTraits` helper factors
    the descriptor read.

## Scope / faithfulness notes

- **Ported**: `Score.score`'s `glyph_count`, `codepoint`, `monospace`, and the
  symbolic-traits-based `bold`/`italic` fields.
- **Deferred**: the `head`/`OS/2`/variation bold-italic **refinement** (the next
  experiment — roastty already has the `Head`/`Os2` parsers), the **style**
  `exact_style`/`fuzzy_style` match, and `sortMatchingDescriptors` (wiring the
  ranking into `discover_descriptors`). With the refinement deferred, `bold`/
  `italic` use the symbolic traits only — a faithful partial that the refinement
  will sharpen.
- The unloadable-font → all-zero score path is not modeled (the objc2
  `with_font_descriptor` is non-fallible); noted as a deviation.
- No C ABI/header/ABI-inventory change (`Score`/`Descriptor` are internal Rust).

## Changes

1. `roastty/src/font/discovery.rs`: add `Descriptor::score` and the
   `symbolic_traits` helper.
2. Tests (in `discovery.rs`) — all score a **resolved candidate** descriptor
   obtained from `discover_descriptors` (a matched font, not the query
   descriptor; scoring the query descriptor is wrong because it lacks resolved
   traits and a character-set query can resolve a codepoint-supporting
   fallback):
   - A shared helper resolves a Menlo candidate:
     `let c = Descriptor { family: Some("Menlo"), .. }.discover_descriptors()`
     then the first candidate whose family reads back `"Menlo"` (assert
     non-empty).
   - `score_menlo_is_monospace`: `Descriptor::default().score(&c)` yields
     `monospace == true` (every Menlo face is monospace) and `glyph_count > 0`.
   - `score_codepoint_present_absent`:
     `Descriptor { codepoint: 'M' as u32, .. } .score(&c)` scores
     `codepoint == true`; `Descriptor { codepoint: 0x1F600, .. }.score(&c)` (an
     emoji the resolved Menlo lacks) scores `codepoint == false`;
     `Descriptor::default().score(&c)` scores `codepoint == false` (none
     sought).
   - `score_bold_italic_match_flips`: scoring the **same** candidate with
     `desc.bold = false` vs `desc.bold = true` gives **opposite** `bold` fields
     (`self.bold == is_bold`, so flipping the request flips the match) —
     deterministic regardless of the candidate's actual boldness; likewise for
     `italic`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty score
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `score` reproduces upstream's `glyph_count`, `codepoint`, `monospace`, and
  symbolic-traits `bold`/`italic` computation (font load, clamp, codepoint
  lookup, symbolic-trait reads, the `desc.x == is_x` comparisons);
- the monospace, codepoint, and bold/italic-match tests pass;
- the `head`/`OS/2`/variation refinement, the style match, and the sort stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a system-font-dependent assertion is
non-deterministic on the test host (the field computation is still exercised).

The experiment **fails** if the score computation for these fields diverges from
upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised **one Required
finding**: the test plan must **not** score the query descriptor directly.
Upstream `Score.score(desc, ct_desc)` scores a **matched candidate** descriptor
(from `createMatchingFontDescriptors`), not the original query — a query
descriptor can lack resolved traits, and if it carries
`kCTFontCharacterSetAttribute` for an "absent" codepoint, CoreText may resolve a
fallback that supports it, making the
`monospace`/`bold`/`italic`/absent-codepoint assertions non-faithful or flaky.
Fixed: the tests now resolve a Menlo candidate via `discover_descriptors` and
score request descriptors against **that** candidate; the bold/italic test
asserts the match-field **flips** when the request's boldness flips
(deterministic regardless of the candidate's actual traits).

Codex confirmed the rest of the slice is sound: the `glyph_count` clamp, the
codepoint lookup only when requested, the symbolic-trait reads, and the
`desc.bold == is_bold` / `desc.italic == is_italic` comparisons match upstream;
deferring the table/variation/style refinement is a valid partial (symbolic-only
bold/italic); and the non-fallible font-load deviation is acceptable as
documented.

Review artifacts:

- Prompt: `logs/codex-review/20260603-114657-068605-prompt.md`
- Result: `logs/codex-review/20260603-114657-068605-last-message.md`

## Result

**Result:** Pass

The score computation lands (the font-loaded + symbolic-trait fields).

- `roastty/src/font/discovery.rs`:
  `Descriptor::score(&self, ct_desc: &CTFontDescriptor) -> Score` loads the
  candidate font (`CTFont::with_font_descriptor`, size 12), clamps `glyph_count`
  to `u16`, computes `codepoint` coverage when sought (`font_has_codepoint` —
  the `Face::glyph_index` UTF-16 surrogate-pair pattern via
  `glyphs_for_characters`), reads the descriptor's symbolic traits
  (`symbolic_traits` — `kCTFontTraitsAttribute` → `kCTFontSymbolicTrait`
  `CFNumber` → `CTFontSymbolicTraits`, absent ⇒ empty), and sets `monospace`
  plus `bold = self.bold == is_bold` / `italic = self.italic == is_italic`.
  `fuzzy_style`/`exact_style` stay `0`/`false` (deferred).

Tests (scoring a **resolved** Menlo candidate from `discover_descriptors`, not
the query): `score_menlo_is_monospace` (`monospace`, `glyph_count > 0`),
`score_codepoint_present_absent` (`'M'` true, `0x1F600` false, none-sought
false), `score_bold_italic_match_flips` (flipping the bold/italic request flips
the match field — deterministic).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2710 passed, 0 failed (+3, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

A candidate font now scores its glyph count, requested-codepoint coverage, and
symbolic monospace/bold/italic match against the request — the core of upstream
`Score.score`.

The next discovery experiment is the **bold/italic refinement** — sharpening
`is_bold`/`is_italic` beyond the symbolic traits using the `head` (`macStyle`),
`OS/2` (`fsSelection`), and variation-axis (`wght`/`ital`/`slnt`) data (roastty
already has the `Head`/`Os2` parsers) — then the **style** `exact_style`/
`fuzzy_style` match, then `sortMatchingDescriptors` (wiring the ranking into
`discover_descriptors`), the `DiscoverIterator`/`DeferredFace`,
`discoverFallback`, and finally the resolver's discovery fallback and codepoint
overrides.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It confirmed `score` matches the intended upstream
slice (candidate load, `glyph_count` clamp, codepoint-only-when-requested via
`glyphs_for_characters` with surrogate-pair handling, symbolic-trait reads from
`kCTFontTraitsAttribute → kCTFontSymbolicTrait`, and the `self.bold == is_bold`
/ `self.italic == is_italic` match semantics), that leaving `fuzzy_style`/
`exact_style` at defaults is correct for the deferred style slice, that the
prior Required finding is resolved (the tests score a resolved candidate, not
the query), and that there are no pointer/lifetime hazards (`font_has_codepoint`
uses live stack buffers for the CoreText call, and `symbolic_traits` reads the
raw dictionary value while the retained dictionary is still alive). No Optional
findings.

Review artifacts:

- Result review: `logs/codex-review/20260603-115032-753141-last-message.md`
