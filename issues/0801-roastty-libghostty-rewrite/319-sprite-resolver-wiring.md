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

# Experiment 319: wiring the sprite font into the resolver

## Description

The sprite render pipeline is complete (`has_codepoint`, `render_codepoint` →
`Glyph`, Experiments 316–318), but the `CodepointResolver` still **defers** it:
`get_index` has a sprite-check placeholder, and `render_glyph` returns
`SpriteUnavailable` for sprite indices. This experiment wires it in — filling
the explicit `SpriteUnavailable` arm — so a sprite codepoint resolves to the
sprite face and renders through `render_codepoint`. Faithful port of upstream
`CodepointResolver.getIndex`'s sprite check and `renderGlyph`'s sprite branch.

## Upstream behavior (`CodepointResolver.zig`)

- The resolver holds an optional `sprite: ?SpriteFace` (with grid metrics); a
  `null` sprite means sprite drawing is disabled.
- `getIndex(cp, p)`: after the style/presentation handling, **before** the
  collection lookup,
  `if (self.sprite) |sprite| if (sprite.hasCodepoint(cp, p)) return .initSpecial(.sprite)`
  — a sprite codepoint always resolves to the sprite face.
- `renderGlyph(atlas, index, glyph_index: u32, opts)`: if `index.special()` is
  `.sprite`, `return self.sprite.?.renderGlyph(alloc, atlas, glyph_index, opts)`
  (the `glyph_index` **is** the codepoint for sprites — hence `u32`, which holds
  the high sprite ranges like `U+1FB00`/`U+1CD00`); else render via the face.

## Rust mapping (`roastty/src/font/codepoint_resolver.rs`)

- `ResolverRenderError`: add an `Atlas(AtlasError)` variant + `From<AtlasError>`
  (the sprite render can fail to reserve atlas space).
- `CodepointResolver`: add a `sprite_metrics: Option<Metrics>` field (the grid
  metrics for sprite rendering; `None` disables sprites — the analog of
  upstream's `?SpriteFace`). `new` initializes it to `None`; add
  `pub(crate) fn set_sprite_metrics(&mut self, metrics: Option<Metrics>)`.
- `get_index`: at the placeholder (before the collection lookup), add
  `if let Some(m) = &self.sprite_metrics { if sprite::draw::has_codepoint(cp, m) { return Some(Index::special(Special::Sprite)); } }`.
- `render_glyph`: change the glyph parameter from `glyph: u16` to
  `glyph_index: u32` (matching upstream — sprite codepoints exceed `u16`). The
  sprite arm:
  `if index.special_kind().is_some() { let m = self.sprite_metrics.as_ref().ok_or(SpriteUnavailable)?; return Ok(sprite::render_codepoint(glyph_index, m, atlas)?.unwrap_or(BLANK_GLYPH)); }`
  (`BLANK_GLYPH` = an all-zero `Glyph`, the upstream fallback when a resolved
  sprite has no draw fn — should not occur for a properly-resolved index). The
  face arm passes `glyph_index as u16` (CoreText glyph ids fit `u16`).

## Scope / faithfulness notes

- **Ported**: the resolver's sprite `get_index` check and `render_glyph` sprite
  branch (filling `SpriteUnavailable`), driven by a `sprite_metrics` toggle.
- **Deferred**: the wide-glyph `cell_width` factoring, the sprite-kind special
  glyphs (underlines/cursors), the collection's sprite-coverage in
  `has_codepoint` (the resolver-level check suffices for `get_index`), and a
  range-only `has_codepoint` fast path.
- No C ABI/header/ABI-inventory change (the resolver/`Glyph`/`Atlas` types are
  internal Rust).

## Changes

1. `roastty/src/font/codepoint_resolver.rs`: add the `Atlas` error variant; the
   `sprite_metrics` field + `set_sprite_metrics`; the `get_index` sprite check;
   the `render_glyph` `u32` widening + sprite branch.
2. Update the existing `render_glyph` test call sites to pass `u32`.
3. Tests:
   - `get_index_sprite_enabled`: with `sprite_metrics` set (from
     `collection().metrics()` after `update_metrics`), `get_index(0x2500, …)`
     returns the `Sprite` special index.
   - `get_index_sprite_disabled`: with `sprite_metrics` `None` (default),
     `get_index(0x2500, …)` does **not** return the sprite index (falls through
     to the face chain).
   - `render_glyph_sprite_enabled`: with sprites enabled,
     `render_glyph(atlas, Sprite, 0x2500, opts)` returns a non-empty `Glyph`.
   - `render_glyph_sprite_high_codepoint`: with sprites enabled,
     `render_glyph(atlas, Sprite, 0x1FB00, opts)` (a sextant codepoint **above
     `u16`**) returns a real `Glyph`, not the blank fallback — proving the `u32`
     glyph index is not truncated to `u16` (the point of the widening); and
     `get_index(0x1FB00, …)` returns the `Sprite` index (per the design review).
   - `render_glyph_sprite_unavailable`: with sprites disabled (default), the
     sprite index returns `Err(SpriteUnavailable)` (the unchanged behavior).
   - `render_glyph_via_resolver` (existing): updated to pass the face glyph id
     as `u32`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- with sprite metrics set, `get_index` resolves a sprite codepoint to the sprite
  index and `render_glyph` renders it via `render_codepoint`; with sprites
  disabled, the sprite index still returns `SpriteUnavailable` and `get_index`
  ignores sprites;
- the enabled/disabled `get_index` and `render_glyph` tests confirm both paths;
- the wide-glyph factoring, the sprite-kind special glyphs, and the collection
  coverage stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the sprite branch needs the wide-glyph
factoring the single-cell path does not cover.

The experiment **fails** if the resolver's sprite resolution or render diverges
from z2d, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Required**
finding: add a sprite-enabled render test using a codepoint **above `u16`**
(e.g. `0x1FB00`), since the whole behavioral reason to widen `render_glyph` from
`u16` to `u32` is that sprite glyph indices are codepoints that can exceed
`0xFFFF` — the `0x2500` tests do not prove the sprite arm avoids truncation.
Fixed: added `render_glyph_sprite_high_codepoint` (`0x1FB00` renders a real
`Glyph`, not the blank fallback, and `get_index(0x1FB00)` returns `Sprite`). One
**Optional** suggestion — widen `get_presentation`'s glyph parameter to `u32`
for API symmetry — noted but deferred (the sprite branch ignores the glyph id
and returns text presentation, so it is not needed for correctness). Codex
confirmed the rest is faithful: `sprite_metrics: Option<Metrics>` is a
reasonable `?SpriteFace` analog; the sprite check belongs before the collection
lookup (sprite codepoints win); `None → blank Glyph` in the sprite render branch
matches upstream's defensive fallback; and ignoring presentation is correct
(upstream passes `p` but sprite `hasCodepoint` ignores it).

Review artifacts:

- Prompt: `logs/codex-review/20260603-101141-360109-prompt.md`
- Result: `logs/codex-review/20260603-101141-360109-last-message.md`
