//! The shared font grid — the central object the renderer holds.
//!
//! Faithful port of upstream `font/SharedGrid.zig`'s render path: it owns the two
//! glyph atlases (grayscale for text, BGRA for color), the [`CodepointResolver`],
//! and the active grid [`Metrics`], and renders a glyph index into the correct
//! atlas. The glyph cache (upstream's `glyphs` map, keyed by `RenderOptions`) is a
//! later sub-area.

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

    /// Render `glyph_index` at `index` into the correct atlas — grayscale for
    /// text, color for emoji — returning its [`Glyph`]. Emoji get upstream's
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
/// disjoint [`SharedGrid`] fields can be borrowed at once.
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
    fn render_glyph_text_places_glyph_in_grayscale_atlas() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();
        let glyph = Face::new("Menlo", 32.0).glyphs_for_characters(&[b'M' as u16])[0];

        let g = grid
            .render_glyph(Index::default(), u32::from(glyph), &opts)
            .expect("'M' renders");

        // It rasterized something.
        assert!(g.width > 0);
        assert!(g.height > 0);
        // The reserved region fits inside the un-grown 512px grayscale atlas. A
        // monochrome glyph routed to the BGRA color atlas would have failed
        // `InvalidAtlasFormat`, so a successful render proves text → grayscale.
        assert_eq!(grid.atlas_grayscale.size(), 512);
        assert!((g.atlas_x + g.width) as usize <= 512);
        assert!((g.atlas_y + g.height) as usize <= 512);
    }

    #[test]
    fn render_glyph_sprite_uses_configured_sprite_font() {
        let mut grid = menlo_grid();
        let opts = menlo_opts();

        // A box-drawing horizontal line (U+2500) renders via the sprite font,
        // which `new` configured. Without `set_sprite_metrics`, this would be
        // `SpriteUnavailable`.
        let g = grid
            .render_glyph(Index::special(Special::Sprite), 0x2500, &opts)
            .expect("the box-drawing sprite renders");
        assert!(g.width > 0);
        assert!(g.height > 0);
    }
}
