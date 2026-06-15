//! The shared font grid — the central object the renderer holds.
//!
//! Faithful port of upstream `font/SharedGrid.zig`'s render path: it owns the two
//! glyph atlases (grayscale for text, BGRA for color), the [`CodepointResolver`],
//! the active grid [`Metrics`], and the glyph cache, and renders a glyph index
//! into the correct atlas — rasterizing each distinct glyph once. Cache
//! invalidation on metrics/font reload is a later sub-area.

use std::collections::HashMap;

use crate::font::atlas::{Atlas, AtlasError, Format};
use crate::font::codepoint_resolver::{CodepointResolver, ResolverRenderError};
use crate::font::collection::{EntryError, Index};
use crate::font::face::constraint::{Align, Constraint, Size};
use crate::font::face::coretext::{RenderGlyphError, RenderOptions};
use crate::font::glyph::Glyph;
use crate::font::metrics::Metrics;
use crate::font::shaper_cache::ShaperCache;
use crate::font::{Presentation, Style};
use crate::renderer::size::CellSize;

/// Initial atlas edge length in pixels. Matches upstream `SharedGrid.init`.
const ATLAS_INITIAL_SIZE: u32 = 512;

/// A rendered glyph paired with the presentation that decided its atlas. Faithful
/// port of upstream `SharedGrid.Render`: the draw path uses `presentation` to
/// sample the right atlas (`Emoji` → color, `Text` → grayscale) and `glyph` for
/// the atlas placement, size, and bearings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Render {
    pub glyph: Glyph,
    pub presentation: Presentation,
}

/// The glyph cache key. Mirrors upstream `GlyphKey.Packed`: the packed font
/// index, the glyph id, and the **integer** render options. The float-bearing
/// `grid_metrics`/`constraint` are excluded — `grid_metrics` is constant per grid
/// and the `constraint` is derived from the glyph's presentation, so neither
/// varies independently of these fields.
///
/// Invariant: this is correct only on the grid/renderer path. It is **not** a
/// general "same glyph, arbitrary constraint" key — a caller rendering the same
/// `(index, glyph, integer-opts)` with a deliberately different
/// `constraint`/`grid_metrics` would wrongly hit the cache. The grid never does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    index: u16,
    glyph: u32,
    cell_width: u8,
    thicken: bool,
    thicken_strength: u8,
    constraint_width: u8,
}

impl GlyphKey {
    fn new(index: Index, glyph: u32, opts: &RenderOptions) -> GlyphKey {
        GlyphKey {
            index: index.int(),
            glyph,
            // Upstream's `cell_width orelse 0`.
            cell_width: opts.cell_width.unwrap_or(0),
            thicken: opts.thicken,
            thicken_strength: opts.thicken_strength,
            constraint_width: opts.constraint_width,
        }
    }
}

/// The codepoint cache key. Mirrors upstream `CodepointKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CodepointKey {
    style: u8,
    codepoint: u32,
    presentation: Option<u8>,
}

impl CodepointKey {
    fn new(codepoint: u32, style: Style, presentation: Option<Presentation>) -> CodepointKey {
        CodepointKey {
            style: style as u8,
            codepoint,
            presentation: presentation.map(|p| p as u8),
        }
    }
}

/// The shared font grid: the two glyph atlases (grayscale for text, BGRA for
/// color), the codepoint resolver, the active grid metrics, and the glyph cache.
/// Renders a glyph index into the correct atlas, rasterizing each distinct glyph
/// once. Faithful port of upstream `font/SharedGrid.zig`'s render path.
pub(crate) struct SharedGrid {
    pub atlas_grayscale: Atlas,
    pub atlas_color: Atlas,
    pub resolver: CodepointResolver,
    pub metrics: Metrics,
    pub shaper_cache: ShaperCache,
    /// The codepoint cache: each lookup stores the resolved font index or a
    /// negative result. Real-face indexes are inserted only after preload
    /// succeeds, matching upstream's no-poison rollback contract.
    codepoints: HashMap<CodepointKey, Option<Index>>,
    /// The glyph cache: each distinct glyph is rasterized into an atlas once.
    glyphs: HashMap<GlyphKey, Render>,
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
            shaper_cache: ShaperCache::new(),
            codepoints: HashMap::new(),
            glyphs: HashMap::new(),
        }
    }

    /// The grid cell size. Faithful port of upstream `SharedGrid.cellSize`.
    pub(crate) fn cell_size(&self) -> CellSize {
        CellSize {
            width: self.metrics.cell_width,
            height: self.metrics.cell_height,
        }
    }

    /// Get the font index for `cp`, caching positive and negative results.
    ///
    /// Positive real-font results are preloaded before insertion so a preload
    /// failure cannot leave a stale index in the cache. Special sprite indexes do
    /// not have a real face and are cached without preloading.
    pub(crate) fn get_index(
        &mut self,
        cp: u32,
        style: Style,
        presentation: Option<Presentation>,
    ) -> Result<Option<Index>, EntryError> {
        let key = CodepointKey::new(cp, style, presentation);
        if let Some(&cached) = self.codepoints.get(&key) {
            return Ok(cached);
        }

        let value = self.resolver.get_index(cp, style, presentation);
        if let Some(index) = value {
            if index.special_kind().is_none() {
                self.resolver.collection_mut().get_face(index)?;
            }
        }
        self.codepoints.insert(key, value);
        Ok(value)
    }

    /// Returns true if the given font index has the codepoint and presentation.
    /// `None` presentation accepts any presentation.
    pub(crate) fn has_codepoint(
        &self,
        index: Index,
        cp: u32,
        presentation: Option<Presentation>,
    ) -> bool {
        self.resolver.has_codepoint(index, cp, presentation)
    }

    /// Render `glyph_index` at `index` into the correct atlas — grayscale for
    /// text, color for emoji — returning a [`Render`] (the glyph plus the
    /// presentation that chose its atlas). Returns the cached `Render` on a hit;
    /// otherwise emoji get upstream's cover/center constraint, and on `AtlasFull`
    /// the atlas grows (`size * 2`) and the render retries once. Faithful port of
    /// upstream `SharedGrid.renderGlyph`.
    pub(crate) fn render_glyph(
        &mut self,
        index: Index,
        glyph_index: u32,
        opts: &RenderOptions,
    ) -> Result<Render, ResolverRenderError> {
        let key = GlyphKey::new(index, glyph_index, opts);
        if let Some(&render) = self.glyphs.get(&key) {
            // Cache hit: no re-rasterization, no atlas reservation. Carries the
            // glyph and the presentation that selected its atlas.
            return Ok(render);
        }

        // CoreText glyph ids fit `u16`; a sprite index ignores the glyph here.
        let presentation = self.resolver.get_presentation(index, glyph_index as u16)?;
        let glyph = match presentation {
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
                    &mut self.resolver,
                    index,
                    glyph_index,
                    &render_opts,
                )
            }
            Presentation::Text => render_into(
                &mut self.atlas_grayscale,
                &mut self.resolver,
                index,
                glyph_index,
                opts,
            ),
        }?; // a render error is propagated WITHOUT caching

        let render = Render {
            glyph,
            presentation,
        };
        self.glyphs.insert(key, render);
        Ok(render)
    }

    /// Render a Unicode codepoint as a real font glyph: resolve `cp` to a face
    /// ([`CodepointResolver::get_index`]), look up its glyph id
    /// ([`CodepointResolver::glyph_index`]), and render it ([`Self::render_glyph`]).
    /// Returns `None` if no font has the codepoint or the resolved face lacks it.
    /// Faithful port of upstream `SharedGrid.renderCodepoint` — used for the lock
    /// cursor and preedit text (which render real codepoints, not sprites).
    pub(crate) fn render_codepoint(
        &mut self,
        cp: u32,
        style: Style,
        presentation: Option<Presentation>,
        opts: &RenderOptions,
    ) -> Result<Option<Render>, ResolverRenderError> {
        let Some(index) = self.get_index(cp, style, presentation)? else {
            return Ok(None);
        };
        let Some(glyph_index) = self.resolver.glyph_index(index, cp)? else {
            return Ok(None);
        };
        Ok(Some(self.render_glyph(
            index,
            u32::from(glyph_index),
            opts,
        )?))
    }
}

/// Render into `atlas`, growing it (`size * 2`) and retrying once on `AtlasFull`.
/// A free function taking the atlas and resolver as separate borrows so the two
/// disjoint [`SharedGrid`] fields can be borrowed at once.
fn render_into(
    atlas: &mut Atlas,
    resolver: &mut CodepointResolver,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::collection::{Collection, Special};
    use crate::font::face::coretext::Face;
    use crate::font::Style;

    fn menlo_grid() -> SharedGrid {
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        c.update_metrics().unwrap();
        let metrics = *c.metrics().unwrap();
        SharedGrid::new(CodepointResolver::new(c), metrics)
    }

    fn menlo_opts() -> RenderOptions {
        let face = Face::new("Menlo", 32.0);
        RenderOptions {
            grid_metrics: Metrics::calc(face.get_metrics()),
            cell_width: None,
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        }
    }

    #[test]
    fn cell_size_reports_metrics_dimensions() {
        let grid = menlo_grid();

        assert_eq!(
            grid.cell_size(),
            CellSize {
                width: grid.metrics.cell_width,
                height: grid.metrics.cell_height,
            }
        );
    }

    #[test]
    fn get_index_caches_visible_ascii_and_has_codepoint_any() {
        let mut grid = menlo_grid();

        for cp in 32..127 {
            let idx = grid
                .get_index(cp, Style::Regular, None)
                .unwrap()
                .expect("ASCII resolves");
            assert_eq!(idx, Index::default());
            assert!(grid.has_codepoint(idx, cp, None));
        }

        assert_eq!(grid.codepoints.len(), 95);

        for cp in 32..127 {
            let idx = grid
                .get_index(cp, Style::Regular, None)
                .unwrap()
                .expect("ASCII cache hit");
            assert_eq!(idx, Index::default());
        }

        assert_eq!(grid.codepoints.len(), 95);
    }

    #[test]
    fn get_index_caches_missing_codepoint_as_none() {
        let mut grid = menlo_grid();
        let missing = 0xFDD0;

        assert_eq!(
            grid.get_index(missing, Style::Regular, Some(Presentation::Text))
                .unwrap(),
            None
        );
        assert_eq!(grid.codepoints.len(), 1);
        assert_eq!(
            grid.get_index(missing, Style::Regular, Some(Presentation::Text))
                .unwrap(),
            None
        );
        assert_eq!(grid.codepoints.len(), 1);
    }

    #[test]
    fn get_index_discovery_fallback_caches_without_duplicates() {
        let mut grid = menlo_grid();
        grid.resolver.set_discover_enabled(true);
        let grin = 0x1F600;

        let first = grid
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .unwrap()
            .expect("emoji fallback resolves");
        let face_count = grid.resolver.collection().face_count(Style::Regular);
        assert_eq!(face_count, 2);

        let second = grid
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .unwrap()
            .expect("emoji fallback cache hit");
        assert_eq!(second, first);
        assert_eq!(
            grid.resolver.collection().face_count(Style::Regular),
            face_count
        );
        assert_eq!(grid.codepoints.len(), 1);
    }

    #[test]
    fn get_index_deferred_discovery_preloads_face_before_caching() {
        let mut grid = menlo_grid();
        grid.resolver.set_discover_enabled(true);
        let grin = 0x1F600;

        let idx = grid
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .unwrap()
            .expect("emoji fallback resolves");

        assert!(!grid
            .resolver
            .collection()
            .get_entry(idx)
            .expect("fallback entry exists")
            .is_deferred());
        assert_eq!(
            grid.get_index(grin, Style::Regular, Some(Presentation::Emoji))
                .unwrap(),
            Some(idx)
        );
    }

    #[test]
    fn get_index_sprite_caches_special_without_real_face_preload() {
        let mut grid = menlo_grid();
        let cp = 0x2500;

        let idx = grid
            .get_index(cp, Style::Regular, None)
            .unwrap()
            .expect("sprite resolves");

        assert_eq!(idx, Index::special(Special::Sprite));
        assert_eq!(grid.resolver.collection().face_count(Style::Regular), 1);
        assert_eq!(
            grid.get_index(cp, Style::Regular, None).unwrap(),
            Some(Index::special(Special::Sprite))
        );
        assert_eq!(grid.codepoints.len(), 1);
    }

    #[test]
    fn render_glyph_text_places_glyph_in_grayscale_atlas() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();
        let glyph = Face::new("Menlo", 32.0).glyphs_for_characters(&[b'M' as u16])[0];

        let r = grid
            .render_glyph(Index::default(), u32::from(glyph), &opts)
            .expect("'M' renders");

        // It rasterized something, and reports the text presentation that routed
        // it to the grayscale atlas.
        assert_eq!(r.presentation, Presentation::Text);
        assert!(r.glyph.width > 0);
        assert!(r.glyph.height > 0);
        // The reserved region fits inside the un-grown 512px grayscale atlas. A
        // monochrome glyph routed to the BGRA color atlas would have failed
        // `InvalidAtlasFormat`, so a successful render proves text → grayscale.
        assert_eq!(grid.atlas_grayscale.size(), 512);
        assert!((r.glyph.atlas_x + r.glyph.width) as usize <= 512);
        assert!((r.glyph.atlas_y + r.glyph.height) as usize <= 512);
    }

    #[test]
    fn render_glyph_sprite_uses_configured_sprite_font() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();

        // A box-drawing horizontal line (U+2500) renders via the sprite font,
        // which `new` configured. Without `set_sprite_metrics`, this would be
        // `SpriteUnavailable`.
        let r = grid
            .render_glyph(Index::special(Special::Sprite), 0x2500, &opts)
            .expect("the box-drawing sprite renders");
        // A sprite is text presentation (it draws into the grayscale atlas).
        assert_eq!(r.presentation, Presentation::Text);
        assert!(r.glyph.width > 0);
        assert!(r.glyph.height > 0);
    }

    #[test]
    fn render_glyph_caches_by_key() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();
        let face = Face::new("Menlo", 32.0);
        let m = u32::from(face.glyphs_for_characters(&[b'M' as u16])[0]);
        let n = u32::from(face.glyphs_for_characters(&[b'N' as u16])[0]);

        // Rendering 'M' twice returns the identical glyph and caches one entry —
        // the second call was a hit (no second rasterization).
        let first = grid.render_glyph(Index::default(), m, &opts).expect("'M'");
        let second = grid.render_glyph(Index::default(), m, &opts).expect("'M'");
        assert_eq!(first, second);
        assert_eq!(grid.glyphs.len(), 1);

        // A distinct glyph is a distinct key — the cache grows to two entries.
        grid.render_glyph(Index::default(), n, &opts).expect("'N'");
        assert_eq!(grid.glyphs.len(), 2);

        // The same glyph with thickening enabled is a distinct render key.
        let thick_opts = RenderOptions {
            thicken: true,
            ..opts
        };
        grid.render_glyph(Index::default(), m, &thick_opts)
            .expect("thick 'M'");
        assert_eq!(grid.glyphs.len(), 3);

        // Strength is also part of upstream's packed key, so changing it cannot
        // reuse the prior thickened rasterization.
        let dim_thick_opts = RenderOptions {
            thicken_strength: 128,
            ..thick_opts
        };
        grid.render_glyph(Index::default(), m, &dim_thick_opts)
            .expect("dim thick 'M'");
        assert_eq!(grid.glyphs.len(), 4);
    }

    #[test]
    fn render_codepoint_renders_a_present_glyph() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();

        // 'M' is in Menlo: the codepoint resolves to a face, its glyph id is
        // looked up, and the glyph renders into the grayscale (text) atlas.
        let r = grid
            .render_codepoint('M' as u32, Style::Regular, Some(Presentation::Text), &opts)
            .expect("render ok")
            .expect("'M' is present");
        assert_eq!(r.presentation, Presentation::Text);
        assert!(r.glyph.width > 0);
        assert!(r.glyph.height > 0);

        // It renders the same glyph as resolving 'M' directly (the path looked up
        // the correct cmap glyph id, not the codepoint).
        let gid = u32::from(Face::new("Menlo", 32.0).glyphs_for_characters(&[b'M' as u16])[0]);
        let direct = grid
            .render_glyph(Index::default(), gid, &opts)
            .expect("'M' direct");
        assert_eq!(r, direct);
    }

    #[test]
    fn render_codepoint_missing_codepoint_is_none() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();

        // A Private-Use codepoint Menlo lacks, with discovery disabled (the
        // default), resolves to no font → `None`.
        let r = grid
            .render_codepoint(0xE000, Style::Regular, Some(Presentation::Text), &opts)
            .expect("render ok");
        assert_eq!(r, None);
    }
}
