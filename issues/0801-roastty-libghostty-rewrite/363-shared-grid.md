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

# Experiment 363: the shared font grid (render path)

## Description

The font subsystem can resolve and shape glyphs (Experiments 339–362) and has
the lower-level rasterization primitives — `CodepointResolver::render_glyph` /
`get_presentation` and the `Atlas` — but nothing ties them into the central
object the renderer holds. This experiment adds the **`SharedGrid`**: it owns
the two glyph atlases (grayscale for text, BGRA for color), the
`CodepointResolver`, and the active grid `Metrics`, and provides
`render_glyph(index, glyph_index, opts)` — the call that rasterizes a shaped
glyph index into the correct atlas. This is upstream's `font/SharedGrid.zig`
render path. The glyph cache is deferred to a follow-up (its key needs a
hashable `RenderOptions`).

## Upstream behavior

Upstream `SharedGrid.renderGlyph(index, glyph_index, opts)`:

1. looks up a glyph cache keyed by `{index, glyph, opts}` (fast path);
2. on miss, gets the glyph's presentation via
   `resolver.getPresentation(index, glyph_index)` and selects the atlas —
   `.text → atlas_grayscale`, `.emoji → atlas_color`;
3. for emoji, overrides the constraint to `size = .cover`, centered, with a
   small pad (so the emoji scales to fill its cells without touching the edges);
4. calls `resolver.renderGlyph(atlas, index, glyph_index, render_opts)`, and on
   `error.AtlasFull` grows the atlas (`size * 2`) and retries once;
5. caches and returns the result.

roastty already has steps 2 and 4's primitives
(`CodepointResolver::get_presentation` / `render_glyph`, `Atlas::grow`); this
experiment is the `SharedGrid` that owns the atlases and performs the
presentation→atlas selection, the emoji constraint, and the grow-and-retry. Step
1 (the cache) is deferred.

## Rust mapping (`roastty/src/font/shared_grid.rs`, new)

```rust
use crate::font::atlas::{Atlas, AtlasError, Format};
use crate::font::codepoint_resolver::{CodepointResolver, ResolverRenderError};
use crate::font::collection::Index;
use crate::font::face::constraint::{Align, Constraint, Size};
use crate::font::face::coretext::{RenderGlyphError, RenderOptions};
use crate::font::glyph::Glyph;
use crate::font::metrics::Metrics;
use crate::font::Presentation;

/// Initial atlas edge length in pixels. Matches upstream `SharedGrid.init`.
const ATLAS_INITIAL_SIZE: u32 = 512;

/// The shared font grid: the two glyph atlases (grayscale for text, BGRA for
/// color), the codepoint resolver, and the active grid metrics. Renders a glyph
/// index into the correct atlas. Faithful port of upstream `font/SharedGrid.zig`'s
/// render path (the glyph cache is a later experiment).
pub(crate) struct SharedGrid {
    pub atlas_grayscale: Atlas,
    pub atlas_color: Atlas,
    pub resolver: CodepointResolver,
    pub metrics: Metrics,
}

impl SharedGrid {
    /// Create a grid over `resolver` with the given grid `metrics`, allocating the
    /// two initial atlases. Always configures the sprite font on the resolver
    /// (terminal rendering needs box-drawing/sprite glyphs), matching upstream
    /// `SharedGrid.init`.
    pub(crate) fn new(mut resolver: CodepointResolver, metrics: Metrics) -> SharedGrid {
        // The shared grid always enables sprite rendering; otherwise a sprite
        // index would render as `SpriteUnavailable`.
        resolver.set_sprite_metrics(Some(metrics));
        SharedGrid {
            atlas_grayscale: Atlas::new(ATLAS_INITIAL_SIZE, Format::Grayscale),
            atlas_color: Atlas::new(ATLAS_INITIAL_SIZE, Format::Bgra),
            resolver,
            metrics,
        }
    }

    /// Render `glyph_index` at `index` into the correct atlas — grayscale for text,
    /// color for emoji — returning its [`Glyph`]. Emoji get upstream's
    /// cover/center constraint. On `AtlasFull`, grows the atlas (`size * 2`) and
    /// retries once. Faithful port of upstream `SharedGrid.renderGlyph` (sans the
    /// glyph cache).
    pub(crate) fn render_glyph(
        &mut self,
        index: Index,
        glyph_index: u32,
        opts: &RenderOptions,
    ) -> Result<Glyph, ResolverRenderError> {
        // CoreText glyph ids fit `u16`; a sprite index ignores the glyph here.
        let presentation = self.resolver.get_presentation(index, glyph_index as u16)?;
        match presentation {
            Presentation::Emoji => {
                let render_opts = RenderOptions {
                    // Scale emoji to cover their cells, centered, with a little pad.
                    constraint: Constraint {
                        size: Size::Cover,
                        align_horizontal: Align::Center,
                        align_vertical: Align::Center,
                        pad_left: 0.025,
                        pad_right: 0.025,
                        ..Constraint::default()
                    },
                    ..*opts
                };
                render_into(
                    &mut self.atlas_color,
                    &self.resolver,
                    index,
                    glyph_index,
                    &render_opts,
                )
            }
            Presentation::Text => render_into(
                &mut self.atlas_grayscale,
                &self.resolver,
                index,
                glyph_index,
                opts,
            ),
        }
    }
}

/// Render into `atlas`, growing it (`size * 2`) and retrying once on `AtlasFull`.
/// A free function taking the atlas and resolver as separate borrows so the two
/// disjoint `SharedGrid` fields can be borrowed at once.
fn render_into(
    atlas: &mut Atlas,
    resolver: &CodepointResolver,
    index: Index,
    glyph_index: u32,
    opts: &RenderOptions,
) -> Result<Glyph, ResolverRenderError> {
    match resolver.render_glyph(atlas, index, glyph_index, opts) {
        Err(e) if is_atlas_full(&e) => {
            atlas.grow(atlas.size() * 2);
            resolver.render_glyph(atlas, index, glyph_index, opts)
        }
        other => other,
    }
}

/// Whether a resolver render error is an atlas-full condition (from either the
/// face render path or the sprite reservation).
fn is_atlas_full(err: &ResolverRenderError) -> bool {
    matches!(
        err,
        ResolverRenderError::Render(RenderGlyphError::AtlasFull)
            | ResolverRenderError::Atlas(AtlasError::AtlasFull)
    )
}
```

This also adds a small `Atlas::size(&self) -> u32` accessor (the field is
private) so the grow-and-retry can double the size, and declares the module in
`font/mod.rs`.

## Scope / faithfulness notes

- **Ported (bridged)**: the `SharedGrid` struct (two atlases + resolver +
  metrics) and `render_glyph`'s presentation→atlas selection, emoji constraint
  override, and atlas grow-and-retry — upstream `SharedGrid.renderGlyph` minus
  the cache.
- **Faithful**: text → grayscale atlas, emoji → color atlas (via
  `get_presentation`); the emoji constraint is `cover`/centered with `0.025`
  left/right pad, matching upstream; `AtlasFull` grows to `size * 2` and retries
  once (both the face-render and sprite-reserve atlas-full conditions); the two
  atlases start at 512 px (`Grayscale` / `Bgra`), as upstream; `new` always
  configures the sprite font (`set_sprite_metrics(Some(metrics))`), matching
  upstream `SharedGrid.init`'s "always set up the sprite font" — so a sprite
  (box-drawing) index renders rather than returning `SpriteUnavailable`.
- **Faithful adaptation**: `render_into` is a free function taking the atlas and
  resolver as separate parameters so the disjoint `SharedGrid` fields are
  borrowed independently (a borrow-checker shape, not a behavior change); the
  emoji `RenderOptions` is built by struct-update (`..*opts`) since all fields
  are `Copy` — no clone needed; `SharedGrid::new` takes the `metrics` explicitly
  (upstream derives them from the collection during `init`; deriving/reloading
  metrics is out of scope here).
- **Deferred**: the **glyph cache** (upstream's
  `glyphs: HashMap<GlyphKey, Render>` fast path — its key includes
  `RenderOptions`, which needs a hashable form, a separate experiment), metrics
  reload on font change, and the Metal draw-path consumer that reads the
  atlases. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/shared_grid.rs` (new): the `SharedGrid` struct, `new`,
   `render_glyph`, and the `render_into` / `is_atlas_full` helpers.
2. `roastty/src/font/atlas.rs`: add `pub(crate) fn size(&self) -> u32`.
3. `roastty/src/font/mod.rs`: declare `pub(crate) mod shared_grid;`.
4. Tests (in `shared_grid.rs`): build a Menlo `SharedGrid` and assert:
   - **text**: `render_glyph(Index::default(), glyph('M'), opts)` returns a
     `Glyph` with `width > 0`, `height > 0`; the reserved region fits inside the
     (un-grown, 512) grayscale atlas; and the success itself proves the
     text→grayscale selection — a monochrome glyph routed to the BGRA color
     atlas would have failed `InvalidAtlasFormat`.
   - **sprite**: `render_glyph(Index::special(Special::Sprite), 0x2500, opts)`
     (box-drawing horizontal line) returns a `Glyph` with `width > 0` — proving
     `new` configured the sprite font (without it this would be
     `SpriteUnavailable`).
5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty shared_grid
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `SharedGrid::render_glyph` selects the atlas by presentation, applies the
  emoji constraint, grows-and-retries on `AtlasFull`, and returns the rendered
  `Glyph` — faithful to upstream `SharedGrid.renderGlyph` (sans cache);
- the render test passes, and the existing tests still pass;
- the glyph cache, metrics reload, and Metal consumer stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the atlas selection is wrong (text into color or
vice versa), the emoji constraint diverges, the grow-and-retry misbehaves, or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Required** finding, now addressed:

- **Required (addressed):** `SharedGrid::new` must enable sprite rendering on
  the resolver. `CodepointResolver::new` starts with `sprite_metrics: None`, so
  a special (sprite) index renders as `SpriteUnavailable` until metrics are set;
  upstream `SharedGrid.init` explicitly always configures the sprite font
  because terminal rendering needs box-drawing glyphs.
  `new(mut resolver, metrics)` now calls
  `resolver.set_sprite_metrics(Some(metrics))` before storing, and a sprite
  render test (`Index::special(Special::Sprite)`, `0x2500`) guards against
  regression.

Codex confirmed the rest is sound: the presentation→atlas selection, the emoji
cover/center/`0.025`-pad constraint override, and the single grow-and-retry on
atlas-full match upstream's `renderGlyph`; matching both
`Render(RenderGlyphError::AtlasFull)` and `Atlas(AtlasError::AtlasFull)` is
correct (face rendering and sprite reservation report atlas-full through
different variants); deferring the glyph cache is an acceptable scope boundary
(each call still returns a valid placement — but it should be a near follow-up,
since repeated glyphs reserve duplicate atlas regions until the cache exists);
the `..*opts` Copy struct-update and the free `render_into` helper (disjoint
`&mut atlas_*` + `&resolver` borrows) are the right Rust shapes; and retrying
once (propagating a second failure) is faithful.

Review artifacts:

- Prompt: `logs/codex-review/20260603-173414-725440-prompt.md` (design)
- Result: `logs/codex-review/20260603-173414-725440-last-message.md` (design)

## Result

**Result:** Pass

The shared font grid's render path is in place — the central font object the
renderer holds.

- `roastty/src/font/shared_grid.rs` (new): `SharedGrid` owns the two glyph
  atlases (grayscale for text, BGRA for color), the `CodepointResolver`, and the
  grid `Metrics`. `new(resolver, metrics)` configures the sprite font on the
  resolver (`set_sprite_metrics(Some(metrics))`) and allocates the two 512 px
  atlases. `render_glyph(index, glyph_index, opts)` gets the glyph's
  presentation (`resolver.get_presentation`), selects the atlas (text →
  grayscale, emoji → color), applies upstream's `cover`/centered/`0.025`-pad
  constraint for emoji (via `..*opts` struct-update), and renders through the
  free helper `render_into` — which grows the atlas (`size * 2`) and retries
  once on `AtlasFull` (matching both `Render(RenderGlyphError::AtlasFull)` and
  `Atlas(AtlasError::AtlasFull)`).
- `roastty/src/font/atlas.rs`: added `pub(crate) fn size(&self) -> u32` (for the
  grow-and-retry doubling; the field is private).
- `roastty/src/font/mod.rs`: declared `pub(crate) mod shared_grid;`.

Tests (in `shared_grid.rs`):

- `render_glyph_text_places_glyph_in_grayscale_atlas` — renders `'M'` at
  `Index::default()`, asserts `width`/`height > 0`, the grayscale atlas is still
  512 px (no grow), and the region fits; the success itself proves text →
  grayscale (a monochrome glyph into the BGRA atlas would fail
  `InvalidAtlasFormat`).
- `render_glyph_sprite_uses_configured_sprite_font` — renders the box-drawing
  `U+2500` at `Index::special(Special::Sprite)` and asserts
  `width`/`height > 0`, proving `new` configured the sprite font (else
  `SpriteUnavailable`).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2811 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The font subsystem now has its central object: `SharedGrid` ties the resolver
and the two atlases together and rasterizes any shaped glyph index into the
right atlas. Combined with the shaping path (Experiments 358–362), the renderer
has everything it needs from `font/` — shape a viewport into `ShapedRun`s, then
`render_glyph` each glyph index into an atlas region.

The remaining `font/`-side work is the **glyph cache** (upstream's
`glyphs: HashMap<GlyphKey, Render>` fast path — its key includes
`RenderOptions`, which needs a hashable form, so it is its own experiment) and
metrics reload on font change. Beyond `font/`, the **Metal draw path** consumes
the atlases and the `ShapedRun`s to fill the GPU cell buffer.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the Required sprite fix is correctly applied
(`new(mut resolver, metrics)` calls `set_sprite_metrics(Some(metrics))` before
storing, matching upstream's "always configures the sprite font"); that
`render_glyph` is faithful to the approved design (presentation selects
grayscale vs BGRA, emoji gets the cover/center/`0.025`-pad constraint, and
`render_into` grows once on either face-path or sprite-path atlas-full before
retrying); that the disjoint-field borrow via the free helper is sound and
`..*opts` is valid because the carried fields are copyable; and that the two
tests cover the intended risks (text → grayscale routing, and `new` configuring
sprite metrics). It noted the deferred cache is a performance/atlas-pressure
concern, not a correctness blocker. Nothing needed to change before the result
commit.

Review artifacts:

- Result review: `logs/codex-review/20260603-173803-232783-last-message.md`
