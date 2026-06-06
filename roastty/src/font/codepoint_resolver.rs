//! Resolves a codepoint to the font face that should render it.
//!
//! Faithful port of the core resolution chain of upstream
//! `font/CodepointResolver.zig`. The resolver sits on top of a [`Collection`]
//! and adds style-disabled fallback, presentation defaults, and the
//! regular-style fallback chain, resolves sprite codepoints to the
//! procedurally-drawn sprite face when sprite drawing is enabled, and defaults a
//! presentation-less codepoint via the UCD `Emoji_Presentation` property,
//! applies config codepoint overrides, and — when discovery is enabled — falls
//! back to a system-discovered face (including the CJK/`discoverFallback`
//! codepoint search) for a codepoint no loaded face covers. `get_index` is now a
//! complete port of upstream `getIndex`.

use std::collections::HashMap;

use crate::font::atlas::{Atlas, AtlasError};
use crate::font::codepoint_map::CodepointMap;
use crate::font::collection::{
    Collection, EntryError, Index, PresentationMode, SizeAdjustment, Special,
};
use crate::font::deferred_face::DeferredFace;
use crate::font::discovery::Descriptor;
use crate::font::face::coretext::{Face, RenderGlyphError, RenderOptions};
use crate::font::glyph::Glyph;
use crate::font::metrics::Metrics;
use crate::font::{Presentation, Style};

/// An all-zero [`Glyph`]: the upstream fallback when a resolved sprite index has
/// no draw function (defensive; should not occur for a properly-resolved index).
const BLANK_GLYPH: Glyph = Glyph {
    width: 0,
    height: 0,
    offset_x: 0,
    offset_y: 0,
    atlas_x: 0,
    atlas_y: 0,
};

/// An error rendering a resolved glyph through the [`CodepointResolver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolverRenderError {
    /// The index is a sprite glyph, but sprite drawing is disabled (no grid
    /// metrics were set via [`CodepointResolver::set_sprite_metrics`]).
    SpriteUnavailable,
    /// The index couldn't be resolved to a face.
    Entry(EntryError),
    /// The face failed to render the glyph.
    Render(RenderGlyphError),
    /// The sprite render failed to reserve atlas space.
    Atlas(AtlasError),
}

impl From<EntryError> for ResolverRenderError {
    fn from(err: EntryError) -> Self {
        ResolverRenderError::Entry(err)
    }
}

impl From<AtlasError> for ResolverRenderError {
    fn from(err: AtlasError) -> Self {
        ResolverRenderError::Atlas(err)
    }
}

impl From<RenderGlyphError> for ResolverRenderError {
    fn from(err: RenderGlyphError) -> Self {
        ResolverRenderError::Render(err)
    }
}

/// Resolves codepoints to face indices over a [`Collection`].
pub(crate) struct CodepointResolver {
    /// The collection of faces this resolver searches.
    collection: Collection,
    /// Whether each style is enabled, indexed by `Style as usize`. A disabled
    /// non-regular style falls back to regular. Faithful port of upstream's
    /// `StyleStatus` (`EnumArray(Style, bool)`).
    styles: [bool; 4],
    /// The grid metrics for sprite rendering. `None` disables sprite drawing —
    /// the analog of upstream's optional `sprite: ?SpriteFace`.
    sprite_metrics: Option<Metrics>,
    /// Whether the discovery-based fallback is enabled (the analog of upstream's
    /// optional `discover`). Disabled by default — discovery is opt-in.
    discover_enabled: bool,
    /// The codepoint → descriptor override map (the analog of upstream's optional
    /// `codepoint_map`). `None` disables overrides.
    codepoint_map: Option<CodepointMap>,
    /// Caches a discovered override descriptor's resolved index (or `None` when
    /// the descriptor found nothing) so repeat overrides don't re-discover or
    /// re-add a face. Keyed by [`Descriptor::hashcode`]. Faithful analog of
    /// upstream's `descriptor_cache`.
    descriptor_cache: HashMap<u64, Option<Index>>,
}

impl CodepointResolver {
    /// Create a resolver over `collection` with all styles enabled and sprite
    /// drawing and discovery disabled.
    pub(crate) fn new(collection: Collection) -> CodepointResolver {
        CodepointResolver {
            collection,
            styles: [true; 4],
            sprite_metrics: None,
            discover_enabled: false,
            codepoint_map: None,
            descriptor_cache: HashMap::new(),
        }
    }

    /// Set (or clear with `None`) the codepoint override map. Faithful analog of
    /// setting upstream's optional `codepoint_map`.
    pub(crate) fn set_codepoint_map(&mut self, map: Option<CodepointMap>) {
        self.codepoint_map = map;
    }

    /// Enable sprite drawing with the given grid `metrics`, or disable it with
    /// `None`. Faithful analog of setting upstream's `sprite: ?SpriteFace`.
    pub(crate) fn set_sprite_metrics(&mut self, metrics: Option<Metrics>) {
        self.sprite_metrics = metrics;
    }

    /// Enable or disable the discovery-based fallback (the analog of setting
    /// upstream's optional `discover`).
    pub(crate) fn set_discover_enabled(&mut self, enabled: bool) {
        self.discover_enabled = enabled;
    }

    /// The underlying collection.
    pub(crate) fn collection(&self) -> &Collection {
        &self.collection
    }

    /// The underlying collection, mutably (e.g. to add faces, complete styles).
    pub(crate) fn collection_mut(&mut self) -> &mut Collection {
        &mut self.collection
    }

    /// Enable or disable a style. A disabled non-regular style resolves as
    /// regular.
    pub(crate) fn set_style_enabled(&mut self, style: Style, enabled: bool) {
        self.styles[style as usize] = enabled;
    }

    /// Resolve a codepoint override: if `cp` is in the override map, discover the
    /// mapped font (caching the result), add it to the collection, and return its
    /// index if it actually has the glyph. Faithful port of upstream
    /// `getIndexCodepointOverride`. Requires discovery and a map.
    fn get_index_codepoint_override(&mut self, cp: u32) -> Option<Index> {
        if !self.discover_enabled {
            return None;
        }
        // Look up and clone the mapped descriptor (drops the map borrow before we
        // mutate the collection/cache).
        let desc = self.codepoint_map.as_ref()?.get(cp)?.clone();
        let key = desc.hashcode();

        // Fast path: the descriptor was already discovered (or known-absent).
        let cached = match self.descriptor_cache.get(&key).copied() {
            Some(v) => v,
            None => {
                // Slow path: discover the descriptor's font.
                let resolved = match desc.discover_faces().next() {
                    Some(face) => self
                        .collection
                        .add_with_adjustment(face, Style::Regular, false, SizeAdjustment::IcWidth)
                        .ok(),
                    None => None,
                };
                self.descriptor_cache.insert(key, resolved);
                resolved
            }
        };

        // A negative cache entry means the descriptor found nothing.
        let idx = cached?;

        // Discovery ignores presentation, so verify the glyph is actually there.
        if self
            .collection
            .has_codepoint(idx, cp, PresentationMode::Any)
        {
            Some(idx)
        } else {
            None
        }
    }

    /// Add the first loaded discovery fallback that covers `cp`.
    ///
    /// This preserves the CoreText `font_for_codepoint` path used by CJK and by
    /// descriptor searches that cannot be represented as deferred descriptors.
    fn add_loaded_discovery_fallback(
        &mut self,
        req: &Descriptor,
        cp: u32,
        p_mode: PresentationMode,
    ) -> Option<Index> {
        let faces = match self.collection.get_face(Index::new(Style::Regular, 0)) {
            Ok(original) => req.discover_fallback_faces(original),
            Err(_) => Vec::new(),
        };
        for face in faces {
            if fallback_face_has_codepoint(&face, cp, p_mode) {
                if let Ok(idx) = self.collection.add_with_adjustment(
                    face,
                    Style::Regular,
                    true,
                    SizeAdjustment::IcWidth,
                ) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Resolve `cp` (in `style`, with optional explicit presentation `p`) to a
    /// face [`Index`], or `None`. Faithful port of upstream `getIndex`'s core
    /// chain (codepoint overrides, the sprite check, the UCD presentation
    /// default, the regular-style retry, the discovery-based fallback when
    /// enabled, and the last-resort `Any`). Overrides and the discovery fallback
    /// **mutate** the collection (adding faces), hence `&mut self`.
    pub(crate) fn get_index(
        &mut self,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
    ) -> Option<Index> {
        // A disabled non-regular style falls back to regular.
        if style != Style::Regular && !self.styles[style as usize] {
            return self.get_index(cp, Style::Regular, p);
        }

        // Codepoint overrides: a config map can force a specific font for this
        // codepoint. Runs first (after the disabled-style normalization).
        if let Some(idx) = self.get_index_codepoint_override(cp) {
            return Some(idx);
        }

        // A sprite codepoint always resolves to the sprite face (when enabled).
        if let Some(m) = &self.sprite_metrics {
            if crate::font::sprite::draw::has_codepoint(cp, m) {
                return Some(Index::special(Special::Sprite));
            }
        }

        // Build the presentation mode (see `presentation_mode`).
        let p_mode = presentation_mode(cp, p);

        // Exact match in the requested style.
        if let Some(idx) = self.collection.get_index(cp, style, p_mode) {
            return Some(idx);
        }

        // For a non-regular style, retry as regular before giving up.
        if style != Style::Regular {
            if let Some(idx) = self.get_index(cp, Style::Regular, p) {
                return Some(idx);
            }
        }

        // Discovery-based fallback: for a regular request with discovery enabled,
        // search the system (via `discoverFallback` — the CJK gate and the
        // codepoint search included) for a font that has this codepoint, add the
        // first match (in the requested presentation) to the collection as a
        // fallback, and return it. Faithful port of upstream's regular-style
        // discovery fallback; codepoint overrides already ran earlier.
        if style == Style::Regular && self.discover_enabled {
            let req = Descriptor {
                codepoint: cp,
                monospace: false,
                ..Default::default()
            };
            if (0x4E00..=0x9FFF).contains(&cp) {
                if let Some(idx) = self.add_loaded_discovery_fallback(&req, cp, p_mode) {
                    return Some(idx);
                }
            }

            let mut tried_deferred = false;
            for face in req.discover_fallback_deferred_faces() {
                tried_deferred = true;
                if fallback_deferred_face_has_codepoint(&face, cp, p_mode) {
                    if let Ok(idx) = self.collection.add_deferred_with_adjustment(
                        face,
                        Style::Regular,
                        true,
                        SizeAdjustment::IcWidth,
                    ) {
                        return Some(idx);
                    }
                }
            }
            if !tried_deferred {
                if let Some(idx) = self.add_loaded_discovery_fallback(&req, cp, p_mode) {
                    return Some(idx);
                }
            }
        }

        // A regular request with `any` presentation has nothing more to try.
        // (Effectively unreachable: `p_mode` is always `Explicit` or `Default`.)
        if style == Style::Regular && p_mode == PresentationMode::Any {
            return None;
        }

        // Last resort: any regular face that has the codepoint in any
        // presentation.
        self.collection
            .get_index(cp, Style::Regular, PresentationMode::Any)
    }

    /// The presentation a glyph at `index` requires (which atlas to use).
    /// Faithful port of upstream `getPresentation`: a sprite index is text; a
    /// real face's glyph is emoji if it's a color glyph, else text.
    pub(crate) fn get_presentation(
        &mut self,
        index: Index,
        glyph: u16,
    ) -> Result<Presentation, EntryError> {
        // The only special kind is the sprite font, which is text presentation.
        if index.special_kind().is_some() {
            return Ok(Presentation::Text);
        }
        let face = self.collection.get_face(index)?;
        Ok(if face.is_color_glyph(glyph) {
            Presentation::Emoji
        } else {
            Presentation::Text
        })
    }

    /// The glyph id for codepoint `cp` in the face at `index` (its cmap lookup),
    /// or `None` if the face lacks the codepoint. Upstream `face.glyphIndex(cp)`,
    /// used by [`crate::font::shared_grid::SharedGrid::render_codepoint`].
    pub(crate) fn glyph_index(&mut self, index: Index, cp: u32) -> Result<Option<u16>, EntryError> {
        let face = self.collection.get_face(index)?;
        Ok(face.glyph_index(cp))
    }

    /// Render the glyph `glyph_index` at `index` into `atlas`, returning its
    /// [`Glyph`]. For a sprite index the `glyph_index` is the codepoint (hence
    /// `u32`, which holds the high sprite ranges); it renders via the sprite
    /// font when enabled, else returns
    /// [`ResolverRenderError::SpriteUnavailable`]. Faithful port of upstream
    /// `renderGlyph`.
    pub(crate) fn render_glyph(
        &mut self,
        atlas: &mut Atlas,
        index: Index,
        glyph_index: u32,
        opts: &RenderOptions,
    ) -> Result<Glyph, ResolverRenderError> {
        if index.special_kind().is_some() {
            // The sprite glyph index is its codepoint.
            let m = self
                .sprite_metrics
                .as_ref()
                .ok_or(ResolverRenderError::SpriteUnavailable)?;
            return Ok(crate::font::sprite::render_codepoint(
                glyph_index,
                m,
                opts.cell_width,
                atlas,
            )?
            .unwrap_or(BLANK_GLYPH));
        }
        let face = self.collection.get_face(index)?;
        // CoreText glyph ids fit in `u16`.
        Ok(face.render_glyph(atlas, glyph_index as u16, opts)?)
    }

    /// Whether the face at `idx` covers `cp` for the requested presentation
    /// (`None` ⇒ any presentation). Faithful analog of upstream
    /// `SharedGrid.hasCodepoint` — note `None` maps to `Any`, **not** the UCD
    /// default (so a grapheme component is satisfied by mere presence).
    pub(crate) fn has_codepoint(&self, idx: Index, cp: u32, p: Option<Presentation>) -> bool {
        self.collection
            .has_codepoint(idx, cp, has_codepoint_mode(p))
    }

    /// The font index that renders `primary_cp` and — when `graphemes` is
    /// non-empty — every codepoint of the grapheme: a single font covering them
    /// all. `primary_cp == 0` resolves the space cell. Emoji ZWJ/variation
    /// selectors (`U+200D`/`U+FE0E`/`U+FE0F`) are skipped. Faithful port of
    /// upstream `RunIterator.indexForCell` (the terminal-`Cell` extraction and the
    /// kitty placeholder check are the `RunIterator`'s, deferred).
    pub(crate) fn index_for_grapheme(
        &mut self,
        primary_cp: u32,
        graphemes: &[u32],
        style: Style,
        presentation: Option<Presentation>,
    ) -> Option<Index> {
        const ZWJ_VS: [u32; 3] = [0x200D, 0xFE0E, 0xFE0F];

        // An empty cell renders as a space.
        if primary_cp == 0 {
            return self.get_index(' ' as u32, style, presentation);
        }

        let primary = self.get_index(primary_cp, style, presentation)?;
        // Common case: a single codepoint resolves to its own index.
        if graphemes.is_empty() {
            return Some(primary);
        }

        // A grapheme: collect a font index for each component, then find one that
        // covers them all.
        let mut candidates = vec![primary];
        for &cp in graphemes {
            if ZWJ_VS.contains(&cp) {
                continue;
            }
            candidates.push(self.get_index(cp, style, None)?);
        }
        for &idx in &candidates {
            if !self.has_codepoint(idx, primary_cp, presentation) {
                continue;
            }
            if graphemes
                .iter()
                .filter(|cp| !ZWJ_VS.contains(cp))
                .all(|&cp| self.has_codepoint(idx, cp, None))
            {
                return Some(idx);
            }
        }
        None
    }
}

/// The `get_index` presentation mapping: an explicit presentation is used as-is;
/// `None` consults the Unicode Character Database (`Emoji_Presentation`) for the
/// default — emoji for codepoints that render as emoji without a variation
/// selector, text otherwise.
fn presentation_mode(cp: u32, p: Option<Presentation>) -> PresentationMode {
    match p {
        Some(v) => PresentationMode::Explicit(v),
        None => PresentationMode::Default(
            if crate::font::emoji_presentation::is_emoji_presentation(cp) {
                Presentation::Emoji
            } else {
                Presentation::Text
            },
        ),
    }
}

/// `hasCodepoint`'s presentation mapping: an explicit presentation is required,
/// but `None` accepts **any** presentation. Differs from [`presentation_mode`]
/// (which uses the UCD default for `None`) — faithful to upstream's
/// `SharedGrid.hasCodepoint`.
fn has_codepoint_mode(p: Option<Presentation>) -> PresentationMode {
    match p {
        Some(v) => PresentationMode::Explicit(v),
        None => PresentationMode::Any,
    }
}

/// Whether `face` (used as a **fallback** face) has the glyph for `cp` in the
/// requested presentation. Replicates a fallback `Entry`'s `has_codepoint`: a
/// fallback entry treats a `Default` presentation as `Explicit`, so the glyph
/// must be present **and** its color-ness must match (`Text ⇒ not a color glyph`,
/// `Emoji ⇒ a color glyph`); `Any` checks presence only.
fn fallback_face_has_codepoint(face: &Face, cp: u32, p_mode: PresentationMode) -> bool {
    let Some(glyph) = face.glyph_index(cp) else {
        return false;
    };
    match p_mode {
        PresentationMode::Any => true,
        PresentationMode::Explicit(p) | PresentationMode::Default(p) => match p {
            Presentation::Text => !face.is_color_glyph(glyph),
            Presentation::Emoji => face.is_color_glyph(glyph),
        },
    }
}

/// Deferred variant of [`fallback_face_has_codepoint`].
fn fallback_deferred_face_has_codepoint(
    face: &DeferredFace,
    cp: u32,
    p_mode: PresentationMode,
) -> bool {
    match p_mode {
        PresentationMode::Any => face.has_codepoint(cp, None),
        PresentationMode::Explicit(p) | PresentationMode::Default(p) => {
            face.has_codepoint(cp, Some(p))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::collection::SyntheticStyle;
    use crate::font::face::coretext::Face;

    const NO_SYNTHESIS: SyntheticStyle = SyntheticStyle {
        italic: false,
        bold: false,
        bold_italic: false,
    };

    fn menlo_resolver() -> CodepointResolver {
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        CodepointResolver::new(c)
    }

    #[test]
    fn index_for_grapheme_simple() {
        // A single codepoint resolves to its own index.
        let mut r = menlo_resolver();
        let want = r.get_index('A' as u32, Style::Regular, None);
        assert!(want.is_some());
        assert_eq!(
            r.index_for_grapheme('A' as u32, &[], Style::Regular, None),
            want
        );
    }

    #[test]
    fn index_for_grapheme_empty_is_space() {
        // An empty cell (codepoint 0) resolves as a space.
        let mut r = menlo_resolver();
        let space = r.get_index(' ' as u32, Style::Regular, None);
        assert!(space.is_some());
        assert_eq!(r.index_for_grapheme(0, &[], Style::Regular, None), space);
    }

    #[test]
    fn index_for_grapheme_multi() {
        // A synthetic two-codepoint grapheme (both in Menlo) exercises the
        // candidate collection and the common-font search: the regular face
        // covers both, so it is returned.
        let mut r = menlo_resolver();
        let want = r.get_index('A' as u32, Style::Regular, None);
        assert!(want.is_some());
        assert_eq!(
            r.index_for_grapheme('A' as u32, &['B' as u32], Style::Regular, None),
            want
        );
    }

    #[test]
    fn index_for_grapheme_skips_zwj() {
        // A ZWJ component is skipped (in both the candidate collection and the
        // coverage check), so the primary's index is returned despite the ZWJ not
        // having its own glyph.
        let mut r = menlo_resolver();
        let want = r.get_index('A' as u32, Style::Regular, None);
        assert_eq!(
            r.index_for_grapheme('A' as u32, &[0x200D], Style::Regular, None),
            want
        );
    }

    #[test]
    fn has_codepoint_basic() {
        // The regular face covers 'A' (any presentation); a C0 control it cannot
        // render is not covered.
        let mut r = menlo_resolver();
        let idx = r
            .get_index('A' as u32, Style::Regular, None)
            .expect("an index for 'A'");
        assert!(r.has_codepoint(idx, 'A' as u32, None));
        assert!(!r.has_codepoint(idx, 0x0007, None), "a BEL has no glyph");
    }

    #[test]
    fn get_index_default_presentation_emoji() {
        // U+2614 (umbrella with rain, Emoji_Presentation = Yes) exists in both a
        // monochrome text face (non-color) and the color emoji face. Both are
        // added as fallback faces, so presentation discriminates between them.
        // With no explicit presentation the UCD default is emoji, so it resolves
        // to the color face; with a forced text presentation it resolves to the
        // text face — proving `get_index` consults `is_emoji_presentation` for
        // the default (and that the exact-match step, not the last-resort `Any`,
        // makes the choice).
        let mut c = Collection::new();
        let text = c
            .add(Face::new("Menlo", 32.0), Style::Regular, true)
            .unwrap();
        let emoji = c
            .add(Face::new("Apple Color Emoji", 32.0), Style::Regular, true)
            .unwrap();
        let mut r = CodepointResolver::new(c);
        let umbrella = 0x2614;
        assert_eq!(
            r.get_index(umbrella, Style::Regular, None),
            Some(emoji),
            "the emoji default resolves the color umbrella"
        );
        assert_eq!(
            r.get_index(umbrella, Style::Regular, Some(Presentation::Text)),
            Some(text),
            "a forced text presentation resolves the monochrome umbrella"
        );
    }

    #[test]
    fn resolve_basic() {
        let mut r = menlo_resolver();
        let m = 'M' as u32;
        let at0 = Some(Index::new(Style::Regular, 0));
        assert_eq!(
            r.get_index(m, Style::Regular, Some(Presentation::Text)),
            at0
        );
        assert_eq!(r.get_index(m, Style::Regular, None), at0);
    }

    #[test]
    fn resolve_missing() {
        let mut r = menlo_resolver();
        // A Private-Use codepoint Menlo lacks; with discovery disabled, no
        // fallback is added.
        assert_eq!(
            r.get_index(0xE000, Style::Regular, Some(Presentation::Text)),
            None
        );
    }

    #[test]
    fn resolve_emoji_via_regular_any() {
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        c.add(Face::new("Apple Color Emoji", 32.0), Style::Regular, false)
            .unwrap();
        let mut r = CodepointResolver::new(c);
        let emoji = 0x1F600u32;
        // Explicit Text misses (Menlo lacks it; the emoji glyph is color, not
        // text), but the final regular/any fallback finds the emoji at idx 1.
        assert_eq!(
            r.get_index(emoji, Style::Regular, Some(Presentation::Text)),
            Some(Index::new(Style::Regular, 1))
        );
    }

    #[test]
    fn resolve_style_disabled_falls_back() {
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        // Alias the missing styles (Bold -> Regular).
        c.complete_styles(NO_SYNTHESIS).expect("complete");
        let mut r = CodepointResolver::new(c);
        r.set_style_enabled(Style::Bold, false);
        // Bold is disabled -> recurse as regular -> {Regular, 0}.
        assert_eq!(
            r.get_index('M' as u32, Style::Bold, Some(Presentation::Text)),
            Some(Index::new(Style::Regular, 0))
        );
    }

    fn none_opts(face: &Face) -> RenderOptions {
        RenderOptions {
            grid_metrics: crate::font::metrics::Metrics::calc(face.get_metrics()),
            cell_width: None,
            constraint: crate::font::face::constraint::Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        }
    }

    #[test]
    fn get_presentation_text() {
        let mut r = menlo_resolver();
        let glyph = r
            .collection_mut()
            .get_face(Index::new(Style::Regular, 0))
            .unwrap()
            .glyph_index('M' as u32)
            .unwrap();
        assert_eq!(
            r.get_presentation(Index::new(Style::Regular, 0), glyph),
            Ok(Presentation::Text)
        );
    }

    #[test]
    fn get_presentation_emoji() {
        let mut c = Collection::new();
        c.add(Face::new("Apple Color Emoji", 32.0), Style::Regular, false)
            .unwrap();
        let mut r = CodepointResolver::new(c);
        let glyph = r
            .collection_mut()
            .get_face(Index::new(Style::Regular, 0))
            .unwrap()
            .glyph_index(0x1F600)
            .unwrap();
        assert_eq!(
            r.get_presentation(Index::new(Style::Regular, 0), glyph),
            Ok(Presentation::Emoji)
        );
    }

    #[test]
    fn get_presentation_sprite() {
        let mut r = menlo_resolver();
        // A sprite index is text presentation and never loads a face.
        assert_eq!(
            r.get_presentation(Index::special(crate::font::collection::Special::Sprite), 0),
            Ok(Presentation::Text)
        );
    }

    #[test]
    fn render_glyph_via_resolver() {
        use crate::font::atlas::Format;
        let mut r = menlo_resolver();
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let idx = Index::new(Style::Regular, 0);
        let face_glyph = r.collection_mut().get_face(idx).unwrap();
        let glyph = face_glyph.glyph_index('M' as u32).unwrap();
        let opts = none_opts(r.collection_mut().get_face(idx).unwrap());
        let g = r
            .render_glyph(&mut atlas, idx, glyph as u32, &opts)
            .expect("render");
        assert!(g.width > 0 && g.height > 0);
    }

    #[test]
    fn render_glyph_sprite_unavailable() {
        use crate::font::atlas::Format;
        let mut r = menlo_resolver();
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let opts = none_opts(
            r.collection_mut()
                .get_face(Index::new(Style::Regular, 0))
                .unwrap(),
        );
        // Sprite drawing is disabled by default.
        assert_eq!(
            r.render_glyph(
                &mut atlas,
                Index::special(crate::font::collection::Special::Sprite),
                0x2500,
                &opts
            ),
            Err(ResolverRenderError::SpriteUnavailable)
        );
    }

    /// A resolver with sprite drawing enabled (grid metrics from the primary
    /// face).
    fn menlo_resolver_sprites() -> CodepointResolver {
        let mut r = menlo_resolver();
        r.collection_mut().update_metrics().expect("metrics");
        let m = r.collection().metrics().cloned();
        assert!(m.is_some(), "primary face yields grid metrics");
        r.set_sprite_metrics(m);
        r
    }

    #[test]
    fn get_index_sprite_enabled() {
        let mut r = menlo_resolver_sprites();
        // A box-drawing codepoint resolves to the sprite face.
        assert_eq!(
            r.get_index(0x2500, Style::Regular, None),
            Some(Index::special(Special::Sprite))
        );
    }

    #[test]
    fn get_index_sprite_disabled() {
        let mut r = menlo_resolver();
        // With sprites disabled, the box-drawing codepoint does not resolve to
        // the sprite face.
        assert_ne!(
            r.get_index(0x2500, Style::Regular, None),
            Some(Index::special(Special::Sprite))
        );
    }

    #[test]
    fn render_glyph_sprite_enabled() {
        use crate::font::atlas::Format;
        let mut r = menlo_resolver_sprites();
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let opts = none_opts(
            r.collection_mut()
                .get_face(Index::new(Style::Regular, 0))
                .unwrap(),
        );
        let g = r
            .render_glyph(&mut atlas, Index::special(Special::Sprite), 0x2500, &opts)
            .expect("sprite renders");
        assert!(g.width > 0 && g.height > 0, "box line glyph is non-empty");
    }

    #[test]
    fn render_glyph_sprite_high_codepoint() {
        use crate::font::atlas::Format;
        let mut r = menlo_resolver_sprites();
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let opts = none_opts(
            r.collection_mut()
                .get_face(Index::new(Style::Regular, 0))
                .unwrap(),
        );
        // 0x1FB00 (a sextant) is above u16: the u32 glyph index must not be
        // truncated. It resolves to and renders as a non-empty sprite.
        assert_eq!(
            r.get_index(0x1fb00, Style::Regular, None),
            Some(Index::special(Special::Sprite))
        );
        let g = r
            .render_glyph(&mut atlas, Index::special(Special::Sprite), 0x1fb00, &opts)
            .expect("high sprite renders");
        assert!(g.width > 0 && g.height > 0, "sextant glyph is non-empty");
    }

    #[test]
    fn discovery_fallback_finds_emoji() {
        // Menlo lacks the grinning-face emoji; with discovery enabled, the
        // resolver discovers a color font that has it and adds it as a fallback.
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        let before = r.collection().face_count(Style::Regular);
        let grin = 0x1F600;
        let idx = r
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .expect("discovery resolves the emoji");
        assert_eq!(
            r.collection().face_count(Style::Regular),
            before + 1,
            "the discovered fallback was added"
        );
        // A second lookup is satisfied by the fallback entry — no new face.
        let idx2 = r
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .expect("the fallback now resolves");
        assert_eq!(idx, idx2, "the same fallback index resolves");
        assert_eq!(
            r.collection().face_count(Style::Regular),
            before + 1,
            "no second fallback was added"
        );
    }

    #[test]
    fn discovery_fallback_stays_deferred_until_glyph_lookup() {
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        let grin = 0x1F600;
        let idx = r
            .get_index(grin, Style::Regular, Some(Presentation::Emoji))
            .expect("discovery resolves the emoji");
        assert!(
            r.collection().get_entry(idx).unwrap().is_deferred(),
            "index resolution and coverage checks leave the fallback deferred"
        );

        let glyph = r
            .glyph_index(idx, grin)
            .expect("fallback index remains valid")
            .expect("emoji glyph exists");
        assert!(glyph > 0);
        assert!(
            !r.collection().get_entry(idx).unwrap().is_deferred(),
            "glyph lookup loads the deferred fallback in place"
        );
    }

    #[test]
    fn discovery_fallback_cjk_uses_loaded_codepoint_search() {
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        let idx = r
            .get_index(0x4E00, Style::Regular, Some(Presentation::Text))
            .expect("discovery resolves a CJK ideograph");
        assert!(
            !r.collection().get_entry(idx).unwrap().is_deferred(),
            "the locale-aware CJK codepoint-search fallback is loaded"
        );
    }

    fn helvetica_a_to_z_map() -> CodepointMap {
        let mut map = CodepointMap::default();
        map.add(
            ['A' as u32, 'Z' as u32],
            Descriptor {
                family: Some("Helvetica".into()),
                ..Default::default()
            },
        );
        map
    }

    #[test]
    fn codepoint_override_forces_font() {
        // 'A' is in Menlo, but the override map forces Helvetica — so `get_index`
        // returns the override's added face, not the Menlo primary.
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        r.set_codepoint_map(Some(helvetica_a_to_z_map()));
        let idx = r
            .get_index('A' as u32, Style::Regular, Some(Presentation::Text))
            .expect("the override resolves 'A'");
        assert_ne!(
            idx,
            Index::new(Style::Regular, 0),
            "the override is not the Menlo primary"
        );

        // Without the map, 'A' resolves to the Menlo primary.
        let mut plain = menlo_resolver();
        plain.set_discover_enabled(true);
        assert_eq!(
            plain.get_index('A' as u32, Style::Regular, Some(Presentation::Text)),
            Some(Index::new(Style::Regular, 0))
        );
    }

    #[test]
    fn codepoint_override_caches() {
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        r.set_codepoint_map(Some(helvetica_a_to_z_map()));
        let before = r.collection().face_count(Style::Regular);
        let a = r.get_index('A' as u32, Style::Regular, Some(Presentation::Text));
        let after_a = r.collection().face_count(Style::Regular);
        // 'B' maps to the same descriptor → a cache hit, no new face.
        let b = r.get_index('B' as u32, Style::Regular, Some(Presentation::Text));
        let after_b = r.collection().face_count(Style::Regular);
        assert_eq!(a, b, "the same override descriptor resolves the same index");
        assert_eq!(after_a, before + 1, "the override added one face");
        assert_eq!(after_b, after_a, "the cache hit added no face");
    }

    #[test]
    fn codepoint_override_unmapped() {
        // '0' is outside the mapped range → not overridden (resolves to Menlo).
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        r.set_codepoint_map(Some(helvetica_a_to_z_map()));
        assert_eq!(
            r.get_index('0' as u32, Style::Regular, Some(Presentation::Text)),
            Some(Index::new(Style::Regular, 0))
        );
    }

    #[test]
    fn discovery_fallback_resolves_cjk() {
        // A CJK ideograph Menlo lacks now resolves via the `discoverFallback`
        // CJK gate (the codepoint search), which the plain general match did not
        // reliably reach.
        let mut r = menlo_resolver();
        r.set_discover_enabled(true);
        let han = 0x4E00;
        assert!(
            r.get_index(han, Style::Regular, Some(Presentation::Text))
                .is_some(),
            "the CJK gate resolves U+4E00"
        );
    }

    #[test]
    fn discovery_fallback_disabled() {
        // Without discovery enabled, an emoji Menlo lacks resolves to nothing.
        let mut r = menlo_resolver();
        assert_eq!(
            r.get_index(0x1F600, Style::Regular, Some(Presentation::Emoji)),
            None,
            "no discovery, no fallback"
        );
    }

    #[test]
    fn fallback_presentation_check() {
        use crate::font::face::coretext::Face;
        let emoji = Face::new("Apple Color Emoji", 32.0);
        let grin = 0x1F600;
        // The grinning face is a color glyph: matches Emoji, not Text.
        assert!(fallback_face_has_codepoint(
            &emoji,
            grin,
            PresentationMode::Explicit(Presentation::Emoji)
        ));
        assert!(!fallback_face_has_codepoint(
            &emoji,
            grin,
            PresentationMode::Explicit(Presentation::Text)
        ));
        // `Any` matches on presence alone.
        assert!(fallback_face_has_codepoint(
            &emoji,
            grin,
            PresentationMode::Any
        ));
        // A codepoint the face lacks never matches (a CJK ideograph the emoji
        // font does not cover).
        assert!(!fallback_face_has_codepoint(
            &emoji,
            0x4E00,
            PresentationMode::Any
        ));
    }
}
