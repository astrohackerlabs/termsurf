//! A CoreText-backed font face (`CTFont`).
//!
//! Faithful (macOS) port of the `CTFont` plumbing in upstream
//! `font/face/coretext.zig`. This module provides face construction, raw
//! OpenType table access (`CTFontCopyTable`), face-metric extraction,
//! glyph measurement, shaping, glyph rasterization, and atlas-backed glyph
//! rendering.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{
    CFArray, CFAttributedString, CFIndex, CFMutableDictionary, CFNumber, CFRange, CFRetained,
    CFString, CFType, CGAffineTransform, CGPoint, CGSize,
};
use objc2_core_graphics::{
    kCGColorSpaceDisplayP3, CGBitmapContextCreate, CGColorSpace, CGContext, CGGlyph,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGTextDrawingMode,
};
use objc2_core_text::{
    kCTFontAttributeName, kCTFontFeatureSettingsAttribute, kCTFontOpenTypeFeatureTag,
    kCTFontOpenTypeFeatureValue, CTFont, CTFontDescriptor, CTFontOrientation, CTFontTableOptions,
    CTLine, CTRun, CTRunStatus,
};

use super::constraint::{Constraint, GlyphSize, Size};
use crate::font::atlas::{Atlas, AtlasError, Format};
use crate::font::discovery::Variation;
use crate::font::glyph::Glyph;
use crate::font::metrics::{FaceMetrics, Metrics};
use crate::font::opentype::{head::Head, hhea::Hhea, os2::Os2, post::Post, svg::Svg};
use crate::font::shape;
use crate::os::cf_release_thread::CfReleasePool;

/// The horizontal-shear transform used to synthesize an italic (oblique) face.
/// `c ≈ tan(15°)` slants the glyphs to the right. Faithful to upstream's
/// `italic_skew`.
const ITALIC_SKEW: CGAffineTransform = CGAffineTransform {
    a: 1.0,
    b: 0.0,
    c: 0.267949,
    d: 1.0,
    tx: 0.0,
    ty: 0.0,
};

/// Color-font state for a face. Faithful port of upstream `ColorState`: a face
/// is colored if it has a non-empty `sbix` table or a parseable `SVG ` table.
#[derive(Debug)]
pub(crate) struct ColorState {
    /// True if the font has a non-empty `sbix` table. Upstream assumes the mere
    /// presence of `sbix` means the font's glyphs are colored.
    sbix: bool,
    /// The parsed `SVG ` table, if the font has one. Used to check whether an
    /// individual glyph has an SVG document.
    svg: Option<Svg>,
}

impl ColorState {
    /// Returns true if the given glyph id is colored. For sbix fonts every glyph
    /// is treated as colored; otherwise the glyph must be present in the `SVG `
    /// table. Faithful port of upstream `ColorState.isColorGlyph`.
    fn is_color_glyph(&self, glyph: u16) -> bool {
        if self.sbix {
            return true;
        }
        self.svg.as_ref().is_some_and(|s| s.has_glyph(glyph))
    }
}

/// A font face backed by a CoreText `CTFont`. `CFRetained` manages the
/// underlying CoreFoundation retain/release.
pub(crate) struct Face {
    font: CFRetained<CTFont>,
    /// When set, the synthetic-bold line width (faux bold for fonts without a
    /// real bold variant).
    synthetic_bold: Option<f64>,
    /// Color-font state (`Some` for color fonts such as Apple Color Emoji).
    color: Option<ColorState>,
}

/// A rasterized glyph: a grayscale coverage bitmap (`width * height` bytes, one
/// byte per pixel) plus its whole-pixel bottom-left bearings. The bitmap is
/// written into the atlas row-for-row (no vertical flip — the texture
/// orientation is the renderer's concern, matching upstream `renderGlyph`).
pub(crate) struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub bitmap: Vec<u8>,
    /// Whole-pixel left bearing (`floor(origin.x)`).
    pub bearing_x: i32,
    /// Whole-pixel bottom bearing (`floor(origin.y)`).
    pub bearing_y: i32,
}

/// An error rendering a glyph into the atlas. Faithful to upstream
/// `renderGlyph`, which propagates both atlas reservation and bitmap-context
/// creation failures rather than panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderGlyphError {
    /// The atlas cannot fit the glyph; it must be enlarged.
    AtlasFull,
    /// CoreGraphics could not create the rasterization bitmap context.
    ContextCreationFailed,
    /// The atlas pixel format doesn't match the glyph's color depth (a color
    /// glyph needs a `Bgra` atlas; a monochrome glyph needs a `Grayscale` one).
    InvalidAtlasFormat,
}

impl From<AtlasError> for RenderGlyphError {
    fn from(err: AtlasError) -> Self {
        match err {
            AtlasError::AtlasFull => RenderGlyphError::AtlasFull,
        }
    }
}

/// Options controlling how a glyph is rendered into the atlas. Faithful subset
/// of upstream `RenderOptions`: the grid metrics defining the cell layout, the
/// sizing/alignment [`Constraint`], the number of cells the glyph spans, and the
/// constraint cell span. (Upstream's `thicken` and `thicken_strength` are
/// deferred with the thicken/color branches but exist here as fields.)
pub(crate) struct RenderOptions {
    /// The metrics defining the grid layout (usually the primary face's).
    pub grid_metrics: Metrics,
    /// The number of cells the glyph spans horizontally, used by the sprite font
    /// to widen its canvas (`None`/`Some(0)`/`Some(1)` ⇒ a single cell). Faithful
    /// analog of upstream's `cell_width: ?u8`.
    pub cell_width: Option<u8>,
    /// The sizing and alignment constraint for the glyph.
    pub constraint: Constraint,
    /// The number of cells horizontally the glyph may take up when constrained.
    pub constraint_width: u8,
    /// Draw the glyph with font smoothing (a heavier, thicker stroke).
    pub thicken: bool,
    /// Strength of the thickening, `0..=255` (only meaningful when `thicken`).
    /// `255` is the default (white fill); lower values gray the fill.
    pub thicken_strength: u8,
}

impl Face {
    /// Create a face for the named system font at the given point size. CoreText
    /// returns a fallback font if the exact name is unavailable, so this never
    /// fails.
    pub(crate) fn new(name: &str, size: f64) -> Face {
        let cf_name = CFString::from_str(name);
        // SAFETY: `cf_name` is a valid `CFString` that lives through the call,
        // and a null `matrix` pointer is documented as valid (no transform).
        let font = unsafe { CTFont::with_name(&cf_name, size, std::ptr::null()) };
        Face::from_ct_font(font)
    }

    /// Build a face wrapping an existing `CTFont`, detecting its color state.
    /// `synthetic_bold` starts unset.
    pub(crate) fn from_ct_font(font: CFRetained<CTFont>) -> Face {
        let mut face = Face {
            font,
            synthetic_bold: None,
            color: None,
        };
        face.color = face.detect_color();
        face
    }

    /// Synthesize a bold face from this one — a size-preserving copy marked with
    /// the synthetic-bold line width. Faithful port of upstream `syntheticBold`.
    pub(crate) fn synthetic_bold(&self) -> Face {
        // SAFETY: `self.font` is a live `CTFont`; size `0.0` preserves the source
        // size and a null matrix/attributes leaves the font otherwise unchanged.
        let copy = unsafe { self.font.copy_with_attributes(0.0, std::ptr::null(), None) };
        let mut face = Face::from_ct_font(copy);
        face.synthetic_bold = Some((self.size() / 14.0).max(1.0));
        face
    }

    /// Resize this face to a new point size in place, replacing its `CTFont`
    /// with a copy at the new size (color is re-detected). Faithful port of
    /// upstream `setSize`. If the face was synthetic-bold, the bold flag is
    /// preserved and its line width is **recomputed** for the new size (an
    /// improvement over upstream, whose `setSize` drops `synthetic_bold` — it
    /// never resizes synthetic faces, so the case never arises there; the
    /// size-derived `max(size / 14, 1)` width would otherwise go stale).
    pub(crate) fn set_size(&mut self, points: f64) {
        let was_synthetic_bold = self.synthetic_bold.is_some();
        // SAFETY: `self.font` is a live `CTFont`; a null matrix/attributes copies
        // it at the new size.
        let copy = unsafe {
            self.font
                .copy_with_attributes(points, std::ptr::null(), None)
        };
        *self = Face::from_ct_font(copy);
        if was_synthetic_bold {
            self.synthetic_bold = Some((points / 14.0).max(1.0));
        }
    }

    /// Apply variation-axis settings to this face in place, rebuilding its
    /// `CTFont` from a font descriptor copied with each axis set. Faithful port of
    /// upstream `setVariations`: a no-op for an empty list; otherwise each
    /// variation's packed tag id becomes a `CFNumber` folded into the descriptor
    /// via `CTFontDescriptorCreateCopyWithVariation`, and the font is rebuilt at
    /// the preserved size. The face is reconstructed (re-deriving color), with
    /// `synthetic_bold` carried across.
    pub(crate) fn set_variations(&mut self, vs: &[Variation]) {
        if vs.is_empty() {
            return;
        }
        // SAFETY: `self.font` is a live `CTFont`.
        let mut desc = unsafe { self.font.font_descriptor() };
        for v in vs {
            let id = CFNumber::new_i32(v.id as i32);
            // SAFETY: `desc` and `id` are live; the call returns a retained
            // descriptor copy with the axis set.
            desc = unsafe { desc.copy_with_variation(&id, v.value) };
        }
        // SAFETY: `self.font` and `desc` are live; a null matrix is valid; size
        // `0.0` preserves the current size.
        let font = unsafe {
            self.font
                .copy_with_attributes(0.0, std::ptr::null(), Some(&desc))
        };
        let synthetic_bold = self.synthetic_bold;
        *self = Face::from_ct_font(font);
        self.synthetic_bold = synthetic_bold;
    }

    /// Synthesize an italic (oblique) face from this one — a copy sheared by the
    /// [`ITALIC_SKEW`] matrix. Faithful port of upstream `syntheticItalic`.
    pub(crate) fn synthetic_italic(&self) -> Face {
        // SAFETY: `self.font` is a live `CTFont`; size `0.0` preserves the source
        // size and `&ITALIC_SKEW` is a valid transform that lives through the
        // call.
        let copy = unsafe { self.font.copy_with_attributes(0.0, &ITALIC_SKEW, None) };
        Face::from_ct_font(copy)
    }

    /// Detect color-font state from the font's tables. A font is a color font if
    /// it has a non-empty `sbix` table or a parseable `SVG ` table. Faithful port
    /// of upstream `ColorState.init`.
    fn detect_color(&self) -> Option<ColorState> {
        let sbix = self.copy_table(b"sbix").is_some_and(|d| !d.is_empty());
        let svg = self
            .copy_table(b"SVG ")
            .and_then(|d| Svg::from_bytes(&d).ok());
        if sbix || svg.is_some() {
            Some(ColorState { sbix, svg })
        } else {
            None
        }
    }

    /// True if this is a color font (e.g. Apple Color Emoji).
    pub(crate) fn has_color(&self) -> bool {
        self.color.is_some()
    }

    /// The synthetic-bold line width, if this is a synthetic-bold face.
    pub(crate) fn synthetic_bold_width(&self) -> Option<f64> {
        self.synthetic_bold
    }

    /// True if this face has an oblique (sheared) transform — i.e. a synthetic
    /// italic. Checks the font's transform matrix for a non-zero shear.
    pub(crate) fn is_skewed(&self) -> bool {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.matrix() }.c != 0.0
    }

    /// True if the given glyph id is colored.
    pub(crate) fn is_color_glyph(&self, glyph: u16) -> bool {
        self.color.as_ref().is_some_and(|c| c.is_color_glyph(glyph))
    }

    /// Map a Unicode codepoint to its glyph id, or `None` if this face has no
    /// glyph for it (or `cp` is not a Unicode scalar value). Faithful port of
    /// upstream `glyphIndex`: the codepoint is converted to UTF-16 (a surrogate
    /// pair for non-BMP) and looked up via `CTFontGetGlyphsForCharacters`.
    pub(crate) fn glyph_index(&self, cp: u32) -> Option<u16> {
        let c = char::from_u32(cp)?;
        let mut units = [0u16; 2];
        let units = c.encode_utf16(&mut units);
        let mut glyphs = [0u16; 2];
        let chars_ptr = NonNull::new(units.as_ptr() as *mut u16).unwrap();
        let glyphs_ptr = NonNull::new(glyphs.as_mut_ptr()).unwrap();
        // SAFETY: `units`/`glyphs` are length-`len` buffers; CoreText reads the
        // UTF-16 units and writes one glyph per unit, returning `false` if any
        // input has no glyph.
        let ok = unsafe {
            self.font
                .glyphs_for_characters(chars_ptr, glyphs_ptr, units.len() as isize)
        };
        if !ok {
            return None;
        }
        // For a surrogate pair the trailing unit decodes to `0`; the glyph is in
        // slot 0.
        Some(glyphs[0])
    }

    /// Shape a run of bare Unicode codepoints, assigning each scalar its own
    /// sequential cluster (cell). A convenience wrapper over [`Self::shape_run`]
    /// for the common case where no grapheme grouping is supplied; equivalent to
    /// calling `shape_run` with `cluster = i` for each scalar `i`.
    pub(crate) fn shape_codepoints(&self, codepoints: &[u32]) -> Vec<shape::Cell> {
        let run: Vec<shape::Codepoint> = codepoints
            .iter()
            .enumerate()
            .map(|(i, &codepoint)| shape::Codepoint {
                codepoint,
                cluster: i as u32,
            })
            .collect();
        self.shape_run(&run)
    }

    /// Shape a clustered run, applying the default OpenType features. A thin
    /// delegate over [`Self::shape_run_with_features`].
    pub(crate) fn shape_run(&self, run: &[shape::Codepoint]) -> Vec<shape::Cell> {
        self.shape_run_with_features(run, &shape::default_features())
    }

    /// Shape a clustered run, applying the features configured by `options` (the
    /// [`default_features`](shape::default_features) merged with the parsed user
    /// features — see [`shape::Options::merged_features`]).
    pub(crate) fn shape_run_options(
        &self,
        run: &[shape::Codepoint],
        options: &shape::Options,
    ) -> Vec<shape::Cell> {
        self.shape_run_with_features(run, &options.merged_features())
    }

    /// Shape a clustered run of codepoints into positioned glyphs with this face,
    /// applying the given OpenType `features`, via CoreText (`CFAttributedString`
    /// → `CTLine` → `CTRun`). Faithful port of the core of upstream `Shaper.shape`:
    /// the input is a `(codepoint, cluster)` stream (upstream's
    /// `RunState.codepoints`, fed by `addCodepoint`), where the caller supplies
    /// each codepoint's cluster (the terminal cell, grouping a grapheme's
    /// codepoints). Each [`shape::Cell`]'s `x` is the glyph's cluster, and
    /// `x_offset`/`y_offset` are the glyph's nudge from the cell origin. The
    /// special/sprite fast path is handled outside CoreText by
    /// [`shape::shape_special`] and the sprite renderer path.
    pub(crate) fn shape_run_with_features(
        &self,
        run: &[shape::Codepoint],
        features: &[shape::Feature],
    ) -> Vec<shape::Cell> {
        if run.is_empty() {
            return Vec::new();
        }
        // Build the run's string and, alongside it, a reverse lookup from each
        // UTF-16 unit back to its cluster (the source cell). The CoreText string
        // indices index into the UTF-16 storage a `CFString` from this `String`
        // preserves; pushing each codepoint's supplied cluster once per UTF-16
        // unit keeps `clusters` aligned with those indices and lets both halves of
        // a surrogate pair share one cluster (mirroring upstream's padding). The
        // parallel `pads` marks which units are padding (codepoint `0`): the first
        // unit carries the real codepoint, any extra unit (a surrogate low half)
        // is padding — and a real `U+0000` is padding too, matching upstream's
        // `codepoint == 0` skip in the `is_first` walk.
        let mut text = String::new();
        let mut clusters: Vec<u32> = Vec::new();
        let mut pads: Vec<bool> = Vec::new();
        for cp in run {
            let Some(ch) = char::from_u32(cp.codepoint) else {
                continue;
            };
            text.push(ch);
            for u in 0..ch.len_utf16() {
                clusters.push(cp.cluster);
                pads.push(if u == 0 { cp.codepoint == 0 } else { true });
            }
        }
        if text.is_empty() {
            return Vec::new();
        }
        let mut release_pool = CfReleasePool::new();
        let cf_str = CFString::from_str(&text);

        // Apply the OpenType features by copying the face's font with a
        // feature-settings descriptor; that font drives shaping.
        let feature_desc = feature_settings_descriptor(features);
        let run_font = match feature_desc.as_ref() {
            // SAFETY: `self.font`/`desc` are live; a null matrix is valid; size
            // `0.0` preserves the current size.
            Some(desc) => unsafe {
                self.font
                    .copy_with_attributes(0.0, std::ptr::null(), Some(&desc))
            },
            None => self.font.clone(),
        };

        // The attributed string binds the (feature-applied) font to the run.
        let attrs = CFMutableDictionary::<CFString, CFType>::empty();
        // SAFETY: `kCTFontAttributeName` is a static key; `run_font` is live and
        // retained by the dictionary on insertion.
        unsafe {
            CFMutableDictionary::set_value(
                Some(attrs.as_opaque()),
                (kCTFontAttributeName as *const CFString).cast::<c_void>(),
                (&*run_font as *const CTFont).cast::<c_void>(),
            );
        }
        // SAFETY: `cf_str`/`attrs` are live; CoreText reads them.
        let Some(attr_str) =
            (unsafe { CFAttributedString::new(None, Some(&cf_str), Some(attrs.as_opaque())) })
        else {
            release_pool.push(cf_str);
            if let Some(desc) = feature_desc {
                release_pool.push(desc);
            }
            release_pool.push(run_font);
            release_pool.push(attrs);
            release_pool.flush();
            return Vec::new();
        };

        // Shape the line and read each run's glyphs and string indices.
        // SAFETY: `attr_str` is a live attributed string.
        let line = unsafe { CTLine::with_attributed_string(&attr_str) };
        // SAFETY: `line` is live; the runs array's elements are `CTRun`s.
        let runs = unsafe { line.glyph_runs() };
        let runs: CFRetained<CFArray<CTRun>> = unsafe { CFRetained::cast_unchecked(runs) };

        let mut cells = Vec::new();
        // The pen's x: the accumulated advance width across the whole line (all
        // runs) — upstream's `run_offset.x`.
        let mut pen: f64 = 0.0;
        // The current cell's cluster and the pen-x captured at its start —
        // upstream's `cell_offset`. Line-wide (persists across runs). `x_offset`
        // is measured from `cell_x`, not the running pen, so a multi-glyph cell's
        // later glyphs are offset from the cell origin.
        let mut cell_cluster: u32 = 0;
        let mut cell_x: f64 = 0.0;
        // The maximum cluster among glyphs already emitted — upstream's
        // `run_offset.cluster`. A glyph whose cluster is `<=` this was rendered
        // out of input order (a reordered glyph); we skip its cell reset.
        let mut run_offset_cluster: u32 = 0;
        // CoreText, despite an enforced LTR embedding level, may emit runs that
        // are non-monotonic or right-to-left, leaving `cells` out of grid order.
        // If any run carries either status, we sort the buffer by `x` at the end.
        let mut non_ltr = false;
        for i in 0..runs.len() {
            let Some(run) = runs.get(i) else { continue };
            // SAFETY: `run` is live.
            let status = unsafe { run.status() };
            if status.intersects(CTRunStatus::RightToLeft | CTRunStatus::NonMonotonic) {
                non_ltr = true;
            }
            // SAFETY: `run` is live.
            let n = unsafe { run.glyph_count() }.max(0) as usize;
            if n == 0 {
                release_pool.push(run);
                continue;
            }
            let glyphs = run_glyphs(&run, n);
            let indices = run_string_indices(&run, n);
            let positions = run_positions(&run, n);
            let advances = run_advances(&run, n);
            for k in 0..n {
                // Map the glyph back to its cluster (cell). On a new cluster,
                // reset the cell origin to the current pen — but skip the reset
                // when the glyph is from a cluster we've already passed (`is_after`,
                // a reordered glyph) or is not its cluster's first codepoint
                // (`!is_first`, a ligature/within-cluster-reordered glyph). In
                // either case it inherits the current cell instead of snapping the
                // origin back.
                let idx = indices[k].max(0) as usize;
                debug_assert!(idx < clusters.len());
                let cluster = clusters.get(idx).copied().unwrap_or(0);
                if cell_cluster != cluster {
                    let is_after = cluster <= run_offset_cluster;
                    let is_first = is_first_codepoint_in_cluster(&clusters, &pads, idx, cluster);
                    if is_first && !is_after {
                        cell_cluster = cluster;
                        cell_x = pen;
                    }
                }
                cells.push(shape::Cell {
                    x: cell_cluster as u16,
                    x_offset: (positions[k].x - cell_x).round() as i16,
                    y_offset: positions[k].y.round() as i16,
                    glyph_index: glyphs[k] as u32,
                });
                // The advance applies to the next glyph's pen position; the max
                // cluster tracks the furthest cell we've emitted.
                pen += advances[k].width;
                run_offset_cluster = run_offset_cluster.max(cluster);
            }
            release_pool.push(run);
        }
        // A non-LTR run left the buffer out of grid order; restore it by `x`.
        // (Exceptionally rare — only complex-shaping scripts trigger this.)
        if non_ltr {
            cells.sort_by(|a, b| a.x.cmp(&b.x));
        }

        release_pool.push(cf_str);
        if let Some(desc) = feature_desc {
            release_pool.push(desc);
        }
        release_pool.push(run_font);
        release_pool.push(attrs);
        release_pool.push(attr_str);
        release_pool.push(line);
        release_pool.push(runs);
        release_pool.flush();

        cells
    }

    /// Discover the font CoreText would use to render `cp`, starting from this
    /// (original) face, or `None` if only the `LastResort` (replacement-glyph)
    /// font matches. Faithful port of the CoreText call of upstream's
    /// `discoverCodepoint` (`CTFontCreateForString`). The caller selects the
    /// original face by style; this performs the codepoint search on it.
    pub(crate) fn font_for_codepoint(&self, cp: u32) -> Option<Face> {
        let c = char::from_u32(cp)?;
        let s = CFString::from_str(&c.to_string());
        // The `CTFontCreateForString` range is in UTF-16 units (1 for a BMP
        // scalar, 2 for a supplementary scalar's surrogate pair).
        let range = CFRange {
            location: 0,
            length: c.len_utf16() as isize,
        };
        // SAFETY: `s` is a live `CFString`; the range is within it.
        let font = unsafe { self.font.for_string(&s, range) };

        // Reject CoreText's `LastResort` font (it renders only replacement
        // boxes) — that means "no real font covers the codepoint".
        // SAFETY: `font` is a live `CTFont`.
        if unsafe { font.post_script_name() }.to_string() == "LastResort" {
            return None;
        }
        Some(Face::from_ct_font(font))
    }

    /// Create a synthetic-bold face for the named font — a convenience for
    /// `Face::new(name, size).synthetic_bold()`.
    pub(crate) fn new_synthetic_bold(name: &str, size: f64) -> Face {
        Face::new(name, size).synthetic_bold()
    }

    /// Copy the raw bytes of an OpenType table identified by its four-character
    /// tag (e.g. `b"head"`), or `None` if the font has no such table.
    pub(crate) fn copy_table(&self, tag: &[u8; 4]) -> Option<Vec<u8>> {
        // The table tag is a big-endian-packed four-character code.
        let table_tag = u32::from_be_bytes(*tag);
        // SAFETY: `self.font` is a live `CTFont`; the tag and (empty) options
        // are valid arguments to `CTFontCopyTable`.
        let data = unsafe { self.font.table(table_tag, CTFontTableOptions(0)) }?;
        Some(data.to_vec())
    }

    /// The point size the face was created at (pixels per em).
    pub(crate) fn size(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.size() }
    }

    /// Copy this face's CoreText font for Quick Look/Look Up attributed text.
    /// Upstream passes an unscaled primary font to AppKit, undoing the backing
    /// scale that terminal rendering applies to the font grid.
    pub(crate) fn copy_quicklook_font(&self, content_scale_y: f64) -> CFRetained<CTFont> {
        let scale = if content_scale_y.is_finite() && content_scale_y > 0.0 {
            content_scale_y
        } else {
            1.0
        };
        // SAFETY: `self.font` is a live `CTFont`; a null matrix/descriptor keeps
        // the same face attributes while applying the requested point size.
        unsafe {
            self.font
                .copy_with_attributes(self.size() / scale, std::ptr::null(), None)
        }
    }

    /// The font's units per em (the head-table fallback).
    pub(crate) fn units_per_em(&self) -> u32 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.units_per_em() as u32 }
    }

    /// CoreText ascent in pixels (the hhea-absent fallback).
    pub(crate) fn ascent(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.ascent() }
    }

    /// CoreText descent in pixels, as a **positive** magnitude (CoreText's
    /// convention); the metric assembly negates it. The hhea-absent fallback.
    pub(crate) fn descent(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.descent() }
    }

    /// CoreText leading (line gap) in pixels (the hhea-absent fallback).
    pub(crate) fn leading(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.leading() }
    }

    /// CoreText cap height in pixels (the OS/2 `sCapHeight`-absent fallback).
    pub(crate) fn cap_height(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.cap_height() }
    }

    /// CoreText x-height in pixels (the OS/2 `sxHeight`-absent fallback).
    pub(crate) fn x_height(&self) -> f64 {
        // SAFETY: `self.font` is a live `CTFont`.
        unsafe { self.font.x_height() }
    }

    /// Map each input UTF-16 code unit to its glyph ID (`0` = no glyph).
    pub(crate) fn glyphs_for_characters(&self, chars: &[u16]) -> Vec<u16> {
        if chars.is_empty() {
            return Vec::new();
        }
        let mut glyphs = vec![0u16; chars.len()];
        let chars_ptr = NonNull::new(chars.as_ptr() as *mut u16).unwrap();
        let glyphs_ptr = NonNull::new(glyphs.as_mut_ptr()).unwrap();
        // SAFETY: `chars` and `glyphs` are non-empty slices of length `count`;
        // CoreText reads `characters` (const) and writes one glyph per char.
        unsafe {
            self.font
                .glyphs_for_characters(chars_ptr, glyphs_ptr, chars.len() as isize);
        }
        glyphs
    }

    /// The horizontal advance width of each glyph, in pixels.
    pub(crate) fn advances_for_glyphs(&self, glyphs: &[u16]) -> Vec<f64> {
        if glyphs.is_empty() {
            return Vec::new();
        }
        let mut advances = vec![CGSize::new(0.0, 0.0); glyphs.len()];
        let glyphs_ptr = NonNull::new(glyphs.as_ptr() as *mut u16).unwrap();
        // SAFETY: `glyphs` is a non-empty slice of length `count`; `advances` is
        // a buffer of the same length that CoreText fills.
        unsafe {
            self.font.advances_for_glyphs(
                CTFontOrientation::Horizontal,
                glyphs_ptr,
                advances.as_mut_ptr(),
                glyphs.len() as isize,
            );
        }
        advances.iter().map(|s| s.width).collect()
    }

    /// The overall bounding rectangle for the glyphs, as `(width, height)` in
    /// pixels.
    pub(crate) fn bounding_rect_for_glyphs(&self, glyphs: &[u16]) -> (f64, f64) {
        if glyphs.is_empty() {
            return (0.0, 0.0);
        }
        let glyphs_ptr = NonNull::new(glyphs.as_ptr() as *mut u16).unwrap();
        // SAFETY: `glyphs` is a non-empty slice of length `count`; a null
        // `bounding_rects` pointer requests only the overall rect (the return).
        let rect = unsafe {
            self.font.bounding_rects_for_glyphs(
                CTFontOrientation::Horizontal,
                glyphs_ptr,
                std::ptr::null_mut(),
                glyphs.len() as isize,
            )
        };
        (rect.size.width, rect.size.height)
    }

    /// Assemble the face's metrics from its OpenType tables and CoreText
    /// measurements. Faithful port of upstream `getMetrics`.
    pub(crate) fn get_metrics(&self) -> FaceMetrics {
        // Read the metric tables. `head` falls back to the byte-identical `bhed`
        // tag used by bitmap-only fonts.
        let head = self
            .copy_table(b"head")
            .or_else(|| self.copy_table(b"bhed"))
            .and_then(|b| Head::from_bytes(&b).ok());
        let post = self
            .copy_table(b"post")
            .and_then(|b| Post::from_bytes(&b).ok());
        let os2 = self
            .copy_table(b"OS/2")
            .and_then(|b| Os2::from_bytes(&b).ok());
        let hhea = self
            .copy_table(b"hhea")
            .and_then(|b| Hhea::from_bytes(&b).ok());

        let units_per_em = head
            .map(|h| h.units_per_em as f64)
            .unwrap_or_else(|| self.units_per_em() as f64);
        let px_per_em = self.size();
        let px_per_unit = px_per_em / units_per_em;

        // Vertical metrics fallback chain.
        let (ascent, descent, line_gap) = match hhea {
            // No hhea: use CoreText's pixel metrics directly (CoreText returns
            // descent as a positive magnitude, so negate it).
            None => (self.ascent(), -self.descent(), self.leading()),
            Some(hhea) => {
                let ha = hhea.ascender as f64;
                let hd = hhea.descender as f64;
                let hg = hhea.line_gap as f64;
                match os2 {
                    None => (ha * px_per_unit, hd * px_per_unit, hg * px_per_unit),
                    Some(os2) => {
                        let oa = os2.s_typo_ascender as f64;
                        let od = os2.s_typo_descender as f64;
                        let og = os2.s_typo_line_gap as f64;
                        if os2.fs_selection.use_typo_metrics() {
                            (oa * px_per_unit, od * px_per_unit, og * px_per_unit)
                        } else if hhea.ascender != 0 || hhea.descender != 0 {
                            (ha * px_per_unit, hd * px_per_unit, hg * px_per_unit)
                        } else if os2.s_typo_ascender != 0 || os2.s_typo_descender != 0 {
                            (oa * px_per_unit, od * px_per_unit, og * px_per_unit)
                        } else {
                            // usWinDescent is positive-down, so negate it.
                            (
                                os2.us_win_ascent as f64 * px_per_unit,
                                -(os2.us_win_descent as f64) * px_per_unit,
                                0.0,
                            )
                        }
                    }
                }
            }
        };

        // Underline from `post` (degenerate-zero thickness/position -> None).
        let (underline_position, underline_thickness) = match post {
            None => (None, None),
            Some(post) => {
                let broken = post.underline_thickness == 0;
                let pos = if broken && post.underline_position == 0 {
                    None
                } else {
                    Some(post.underline_position as f64 * px_per_unit)
                };
                let thick = if broken {
                    None
                } else {
                    Some(post.underline_thickness as f64 * px_per_unit)
                };
                (pos, thick)
            }
        };

        // Strikethrough from `OS/2` (same degenerate-zero logic).
        let (strikethrough_position, strikethrough_thickness) = match os2 {
            None => (None, None),
            Some(os2) => {
                let broken = os2.y_strikeout_size == 0;
                let pos = if broken && os2.y_strikeout_position == 0 {
                    None
                } else {
                    Some(os2.y_strikeout_position as f64 * px_per_unit)
                };
                let thick = if broken {
                    None
                } else {
                    Some(os2.y_strikeout_size as f64 * px_per_unit)
                };
                (pos, thick)
            }
        };

        // Cap/ex height: OS/2 values when present, else CoreText.
        let (cap_height, ex_height) = match os2 {
            None => (Some(self.cap_height()), Some(self.x_height())),
            Some(os2) => (
                Some(
                    os2.s_cap_height
                        .map(|v| v as f64 * px_per_unit)
                        .unwrap_or_else(|| self.cap_height()),
                ),
                Some(
                    os2.sx_height
                        .map(|v| v as f64 * px_per_unit)
                        .unwrap_or_else(|| self.x_height()),
                ),
            ),
        };

        // Cell width = widest printable-ASCII advance; ASCII height = the
        // overall bounding-box height of those glyphs.
        let ascii: Vec<u16> = (0x20u16..0x7F).collect();
        let ascii_glyphs = self.glyphs_for_characters(&ascii);
        let cell_width = self
            .advances_for_glyphs(&ascii_glyphs)
            .into_iter()
            .fold(0.0_f64, f64::max);
        let ascii_height = self.bounding_rect_for_glyphs(&ascii_glyphs).1;

        // Ideographic width: the advance of `水`, discarded if absent or if its
        // bounds are wider than its advance (a butchered patched-CJK font).
        let ic_width = {
            let glyph = self.glyphs_for_characters(&[0x6C34])[0];
            if glyph == 0 {
                None
            } else {
                let advance = self.advances_for_glyphs(&[glyph])[0];
                let bounds_w = self.bounding_rect_for_glyphs(&[glyph]).0;
                if bounds_w > advance {
                    None
                } else {
                    Some(advance)
                }
            }
        };

        FaceMetrics {
            px_per_em,
            cell_width,
            ascent,
            descent,
            line_gap,
            underline_position,
            underline_thickness,
            strikethrough_position,
            strikethrough_thickness,
            cap_height,
            ex_height,
            ascii_height: Some(ascii_height),
            ic_width,
        }
    }

    /// Rasterize a single glyph to a grayscale coverage bitmap sized to its
    /// natural bounding box (with sub-pixel positioning), plus its whole-pixel
    /// bearings, or `None` if the glyph has no (or a sub-pixel) outline.
    /// Monochrome and unconstrained: no cell constraints, color, or synthetic
    /// bold (the deferred branches of upstream `renderGlyph`).
    pub(crate) fn rasterize_glyph(&self, glyph: u16) -> Option<RasterizedGlyph> {
        let glyphs = [glyph];
        let glyphs_ptr = NonNull::new(glyphs.as_ptr() as *mut u16).unwrap();
        // SAFETY: `glyphs` is a 1-element slice; a null per-glyph buffer requests
        // only the overall rect.
        let rect = unsafe {
            self.font.bounding_rects_for_glyphs(
                CTFontOrientation::Horizontal,
                glyphs_ptr,
                std::ptr::null_mut(),
                1,
            )
        };

        // No outline (or one too small to render) -> empty glyph.
        if rect.size.width < 0.25 || rect.size.height < 0.25 {
            return None;
        }

        // Whole-pixel bottom-left bearings, with the fractional remainder kept
        // for sub-pixel positioning. The canvas is sized to fit the glyph plus
        // that fractional offset.
        let px_x = rect.origin.x.floor() as i32;
        let px_y = rect.origin.y.floor() as i32;
        let frac_x = rect.origin.x - rect.origin.x.floor();
        let frac_y = rect.origin.y - rect.origin.y.floor();
        let px_w = (rect.size.width + frac_x).ceil() as usize;
        let px_h = (rect.size.height + frac_y).ceil() as usize;

        // Unconstrained: identity scale, drawn at the negated raw bearings, no
        // synthetic-bold stroke.
        let bitmap = self.draw_coverage(
            glyph,
            -rect.origin.x,
            -rect.origin.y,
            frac_x,
            frac_y,
            1.0,
            1.0,
            px_w,
            px_h,
            false,
            1.0,
            None,
            false,
        )?;

        Some(RasterizedGlyph {
            width: px_w as u32,
            height: px_h as u32,
            bitmap,
            bearing_x: px_x,
            bearing_y: px_y,
        })
    }

    /// Draw a single glyph into a fresh `px_w * px_h` coverage buffer (1 byte per
    /// pixel grayscale, or 4 bytes per pixel premultiplied-first BGRA when
    /// `color`). The CTM is translated by `(tx, ty)` (sub-pixel positioning plus
    /// any canvas padding) then scaled by `(scale_x, scale_y)` (the constraint
    /// stretch; `1.0` when unconstrained), and the glyph is drawn at `(draw_x,
    /// draw_y)` — the caller passes the negated raw bearings so the outline's
    /// bottom-left maps to the CTM origin. Returns the buffer, or `None` if the
    /// bitmap context can't be created.
    #[allow(clippy::too_many_arguments)]
    fn draw_coverage(
        &self,
        glyph: u16,
        draw_x: f64,
        draw_y: f64,
        tx: f64,
        ty: f64,
        scale_x: f64,
        scale_y: f64,
        px_w: usize,
        px_h: usize,
        thicken: bool,
        fill_gray: f64,
        stroke_width: Option<f64>,
        color: bool,
    ) -> Option<Vec<u8>> {
        // Color (emoji) glyphs render to a Display-P3, premultiplied-first,
        // 4-byte-per-pixel BGRA buffer; monochrome glyphs to a 1-byte device-gray
        // buffer.
        let (colorspace, depth, bitmap_info) = if color {
            // SAFETY: `kCGColorSpaceDisplayP3` is a static `CFString` name.
            let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceDisplayP3 }))?;
            let info =
                CGImageByteOrderInfo::Order32Little.0 | CGImageAlphaInfo::PremultipliedFirst.0;
            (space, 4usize, info)
        } else {
            (CGColorSpace::new_device_gray()?, 1usize, 0u32)
        };
        let mut buf = vec![0u8; px_w * px_h * depth];

        // SAFETY: `buf` is `px_w * px_h * depth` bytes matching the colorspace,
        // `depth`, and `bitmap_info`; the colorspace is live.
        let ctx = unsafe {
            CGBitmapContextCreate(
                buf.as_mut_ptr().cast(),
                px_w,
                px_h,
                8,
                px_w * depth,
                Some(&colorspace),
                bitmap_info,
            )
        }?;

        // "Font smoothing" is the optional thickening that makes text look closer
        // to native macOS applications.
        CGContext::set_allows_font_smoothing(Some(&ctx), true);
        CGContext::set_should_smooth_fonts(Some(&ctx), thicken);

        // Sub-pixel positioning lets glyphs land at non-integer coordinates,
        // which we need for our own alignment via the CTM translate.
        CGContext::set_allows_font_subpixel_positioning(Some(&ctx), true);
        CGContext::set_should_subpixel_position_fonts(Some(&ctx), true);

        // We carefully manage glyph positions ourselves, so we disable CoreText's
        // sub-pixel quantization to keep it from snapping them to the grid.
        CGContext::set_allows_font_subpixel_quantization(Some(&ctx), false);
        CGContext::set_should_subpixel_quantize_fonts(Some(&ctx), false);

        CGContext::set_should_antialias(Some(&ctx), true);
        CGContext::set_allows_antialiasing(Some(&ctx), true);

        // Set the drawing color. Color glyphs use opaque white RGBA (the glyph
        // carries its own colors); monochrome glyphs use a white (or
        // `thicken_strength`-grayed) fill where the gray value is the coverage.
        // The stroke color matches the fill and is set unconditionally (it only
        // takes effect when stroking).
        if color {
            CGContext::set_rgb_fill_color(Some(&ctx), 1.0, 1.0, 1.0, 1.0);
            CGContext::set_rgb_stroke_color(Some(&ctx), 1.0, 1.0, 1.0, 1.0);
        } else {
            CGContext::set_gray_fill_color(Some(&ctx), fill_gray, 1.0);
            CGContext::set_gray_stroke_color(Some(&ctx), fill_gray, 1.0);
        }

        // Synthetic bold: fill *and* stroke the outline at the given line width,
        // making the glyph heavier. Set before the CTM transforms (upstream
        // order), so the stroke width scales with any constraint stretch.
        if let Some(lw) = stroke_width {
            CGContext::set_text_drawing_mode(Some(&ctx), CGTextDrawingMode::FillStroke);
            CGContext::set_line_width(Some(&ctx), lw);
        }

        // Shift by `(tx, ty)` (the fractional bearing plus any canvas padding)
        // for sub-pixel positioning, then scale so the raw outline is stretched
        // to the constrained size. Order matters: translate before scale.
        CGContext::translate_ctm(Some(&ctx), tx, ty);
        CGContext::scale_ctm(Some(&ctx), scale_x, scale_y);

        let glyphs = [glyph];
        let glyphs_ptr = NonNull::new(glyphs.as_ptr() as *mut u16).unwrap();
        let positions = [CGPoint {
            x: draw_x,
            y: draw_y,
        }];
        let positions_ptr = NonNull::new(positions.as_ptr() as *mut CGPoint).unwrap();
        // SAFETY: `glyphs`/`positions` are 1-element; `ctx` is the live grayscale
        // context drawn into.
        unsafe {
            self.font.draw_glyphs(glyphs_ptr, positions_ptr, 1, &ctx);
        }

        // Release the context before moving `buf`: it holds a raw pointer into it.
        drop(ctx);

        Some(buf)
    }

    /// Render a glyph into the `atlas`, applying the sizing/alignment constraint
    /// in `opts`, and return its [`Glyph`] (pixel size, whole-pixel bearings, and
    /// atlas coordinates). Faithful port of upstream `renderGlyph`: cell
    /// constraints, thicken, synthetic bold, and the color/sbix path are applied.
    /// The atlas must match the glyph's color depth (a `Bgra` atlas for color
    /// glyphs, `Grayscale` for monochrome) or [`RenderGlyphError::InvalidAtlasFormat`]
    /// is returned.
    pub(crate) fn render_glyph(
        &self,
        atlas: &mut Atlas,
        glyph: u16,
        opts: &RenderOptions,
    ) -> Result<Glyph, RenderGlyphError> {
        // `is_color` selects BGRA rendering; `sbix` (a color glyph in an sbix
        // bitmap font) additionally skips synthetic bold, thicken padding, and
        // sub-pixel positioning. An SVG color glyph is color but not sbix.
        // Faithful split of upstream's `is_color` / `sbix = is_color and …sbix`.
        let is_color = self.is_color_glyph(glyph);
        let sbix = is_color && self.color.as_ref().is_some_and(|c| c.sbix);

        // The atlas pixel format must match the glyph's color depth.
        let required = if is_color {
            Format::Bgra
        } else {
            Format::Grayscale
        };
        if atlas.format() != required {
            return Err(RenderGlyphError::InvalidAtlasFormat);
        }

        let glyphs = [glyph];
        let glyphs_ptr = NonNull::new(glyphs.as_ptr() as *mut u16).unwrap();
        // SAFETY: `glyphs` is a 1-element slice; a null per-glyph buffer requests
        // only the overall rect.
        let rect = unsafe {
            self.font.bounding_rects_for_glyphs(
                CTFontOrientation::Horizontal,
                glyphs_ptr,
                std::ptr::null_mut(),
                1,
            )
        };

        // Synthetic bold gains half the line width on every edge, so grow the
        // rect by the line width before everything downstream (the guard,
        // `constrain`, the draw position, and the scale denominators) sees it.
        // Bitmap (sbix) glyphs aren't affected by synthetic bold.
        let (mut rw, mut rh, mut ox, mut oy) = (
            rect.size.width,
            rect.size.height,
            rect.origin.x,
            rect.origin.y,
        );
        if !sbix {
            if let Some(lw) = self.synthetic_bold {
                rw += lw;
                rh += lw;
                ox -= lw / 2.0;
                oy -= lw / 2.0;
            }
        }

        // No outline (or one too small to render) -> a zero glyph, matching
        // upstream. Nothing is reserved in the atlas.
        if rw < 0.25 || rh < 0.25 {
            return Ok(Glyph {
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                atlas_x: 0,
                atlas_y: 0,
            });
        }

        let cell_width = opts.grid_metrics.cell_width as f64;
        let cell_baseline = opts.grid_metrics.cell_baseline as f64;

        // Apply the constraint to get the final size and position of the glyph.
        // The baseline is added to `y` first because `constrain` operates on
        // cell-relative positions, not baseline-relative ones.
        let glyph_size = opts.constraint.constrain(
            GlyphSize {
                width: rw,
                height: rh,
                x: ox,
                y: oy + cell_baseline,
            },
            &opts.grid_metrics,
            opts.constraint_width,
        );

        let mut x = glyph_size.x;
        let mut y = glyph_size.y;
        let mut width = glyph_size.width;
        let mut height = glyph_size.height;

        // Center the glyph within the pixel-rounded cell if it's wider than the
        // face, so it isn't off to the left. Skipped for stretch, which already
        // positioned against the new cell width.
        if opts.constraint.size != Size::Stretch {
            let dx = (cell_width - opts.grid_metrics.face_width) / 2.0;
            x += dx;
            if dx < 0.0 {
                // For a negative diff (cell narrower than advance), drop the
                // integer part and keep only the fractional sub-pixel adjustment.
                x -= dx.trunc();
            }
        }

        // A bitmap (sbix) glyph always renders as full pixels, so quantize its
        // position and size to whole pixels for good results.
        if sbix {
            let cell_height = opts.grid_metrics.cell_height as f64;
            width = cell_width - (cell_width - width - x).round() - x.round();
            height = cell_height - (cell_height - height - y).round() - y.round();
            x = x.round();
            y = y.round();
        }

        // Font smoothing ("thicken") can add up to one pixel on every edge, so
        // we pad the canvas by that much when it's enabled to avoid clipping.
        // Bitmap (sbix) glyphs aren't affected by smoothing, so no padding.
        let canvas_padding: i32 = if opts.thicken && !sbix { 1 } else { 0 };

        // Whole-pixel bearings and canvas from the constrained values, with the
        // fractional remainder kept for sub-pixel positioning. The padding
        // shifts the bearings out and grows the canvas by two per axis.
        let px_x = x.floor() as i32 - canvas_padding;
        let px_y = y.floor() as i32 - canvas_padding;
        let frac_x = x - x.floor();
        let frac_y = y - y.floor();
        let px_w = (width + frac_x).ceil() as usize + 2 * canvas_padding as usize;
        let px_h = (height + frac_y).ceil() as usize + 2 * canvas_padding as usize;

        // Draw at the negated raw bearings, scaling the raw outline to the
        // constrained size. The translate folds in the canvas padding so the
        // glyph stays centered in the padded canvas. `draw_coverage` returns
        // `None` only if CoreGraphics can't create the bitmap context (very
        // unlikely for an in-bounds `px_w, px_h >= 1` grayscale buffer, but
        // propagated rather than panicked, matching upstream).
        let pad = canvas_padding as f64;
        let bitmap = self
            .draw_coverage(
                glyph,
                -ox,
                -oy,
                frac_x + pad,
                frac_y + pad,
                width / rw,
                height / rh,
                px_w,
                px_h,
                opts.thicken,
                opts.thicken_strength as f64 / 255.0,
                self.synthetic_bold,
                is_color,
            )
            .ok_or(RenderGlyphError::ContextCreationFailed)?;

        let region = atlas.reserve(px_w as u32, px_h as u32)?;
        atlas.set(region, &bitmap);

        Ok(Glyph {
            width: px_w as u32,
            height: px_h as u32,
            // Left bearing; top bearing = bottom bearing + height.
            offset_x: px_x,
            offset_y: px_y + px_h as i32,
            atlas_x: region.x,
            atlas_y: region.y,
        })
    }
}

/// Build a `CTFontDescriptor` carrying the OpenType feature settings for
/// `features` (`None` for an empty list). Faithful port of upstream
/// `makeFeaturesDict`: an array of per-feature dictionaries
/// `{ kCTFontOpenTypeFeatureTag: <tag>, kCTFontOpenTypeFeatureValue: <value> }`,
/// stored under `kCTFontFeatureSettingsAttribute`. Applying this descriptor to a
/// font (via `copy_with_attributes`) yields a font that shapes with those
/// features.
fn feature_settings_descriptor(
    features: &[shape::Feature],
) -> Option<CFRetained<CTFontDescriptor>> {
    if features.is_empty() {
        return None;
    }
    let dicts: Vec<CFRetained<CFMutableDictionary<CFString, CFType>>> = features
        .iter()
        .filter_map(|f| {
            // CoreText's `kCTFontOpenTypeFeatureValue` is a signed int (as
            // upstream's `@intCast` to `c_int`); skip any value that does not fit
            // rather than wrapping. Real feature values are small.
            let value_i32 = i32::try_from(f.value).ok()?;
            let dict = CFMutableDictionary::<CFString, CFType>::empty();
            // OpenType feature tags are 4 ASCII bytes by construction (upstream
            // builds the string from exactly these bytes).
            let tag = CFString::from_str(
                std::str::from_utf8(&f.tag).expect("an OpenType feature tag is 4 ASCII bytes"),
            );
            let value = CFNumber::new_i32(value_i32);
            // SAFETY: the keys are static CF strings; the dict retains both values
            // on insertion.
            unsafe {
                CFMutableDictionary::set_value(
                    Some(dict.as_opaque()),
                    (kCTFontOpenTypeFeatureTag as *const CFString).cast::<c_void>(),
                    (&*tag as *const CFString).cast::<c_void>(),
                );
                CFMutableDictionary::set_value(
                    Some(dict.as_opaque()),
                    (kCTFontOpenTypeFeatureValue as *const CFString).cast::<c_void>(),
                    (&*value as *const CFNumber).cast::<c_void>(),
                );
            }
            Some(dict)
        })
        .collect();
    // Every feature was out of range — no settings to apply.
    if dicts.is_empty() {
        return None;
    }
    let list = CFArray::from_retained_objects(&dicts);

    let settings = CFMutableDictionary::<CFString, CFType>::empty();
    // SAFETY: `kCTFontFeatureSettingsAttribute` is a static key; the dict retains
    // the array on insertion.
    unsafe {
        CFMutableDictionary::set_value(
            Some(settings.as_opaque()),
            (kCTFontFeatureSettingsAttribute as *const CFString).cast::<c_void>(),
            (&*list as *const CFArray<CFMutableDictionary<CFString, CFType>>).cast::<c_void>(),
        );
    }
    // SAFETY: `settings` is a live attributes dictionary.
    Some(unsafe { CTFontDescriptor::with_attributes(settings.as_opaque()) })
}

/// Whether the glyph at UTF-16 index `idx` is the first codepoint of `cluster` in
/// the input stream: walk backward from `idx`, skip surrogate-pad units (`pads[j]`
/// true, i.e. codepoint `0`), and report whether the nearest real predecessor has
/// a different cluster. No real predecessor (start of run, or only padding before)
/// ⇒ first. Faithful port of upstream's `is_first_codepoint_in_cluster`.
fn is_first_codepoint_in_cluster(
    clusters: &[u32],
    pads: &[bool],
    idx: usize,
    cluster: u32,
) -> bool {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        if pads[j] {
            continue;
        }
        return clusters[j] != cluster;
    }
    true
}

/// Read a `CTRun`'s `n` glyph ids — via the fast direct pointer, or a copy into
/// an owned buffer if CoreText does not expose one.
fn run_glyphs(run: &CTRun, n: usize) -> Vec<CGGlyph> {
    // SAFETY: `run` is live; the fast-path pointer is valid for `n` glyphs while
    // the run is alive.
    let ptr = unsafe { run.glyphs_ptr() };
    if !ptr.is_null() {
        return unsafe { std::slice::from_raw_parts(ptr, n) }.to_vec();
    }
    let mut buf = vec![0 as CGGlyph; n];
    // SAFETY: `buf` holds `n` elements; the range covers the whole run.
    unsafe {
        run.glyphs(
            CFRange {
                location: 0,
                length: n as isize,
            },
            NonNull::new(buf.as_mut_ptr()).unwrap(),
        );
    }
    buf
}

/// Read a `CTRun`'s `n` source string indices (the cluster of each glyph) — via
/// the fast direct pointer, or a copy if CoreText does not expose one.
fn run_string_indices(run: &CTRun, n: usize) -> Vec<CFIndex> {
    // SAFETY: see `run_glyphs`.
    let ptr = unsafe { run.string_indices_ptr() };
    if !ptr.is_null() {
        return unsafe { std::slice::from_raw_parts(ptr, n) }.to_vec();
    }
    let mut buf = vec![0 as CFIndex; n];
    // SAFETY: `buf` holds `n` elements; the range covers the whole run.
    unsafe {
        run.string_indices(
            CFRange {
                location: 0,
                length: n as isize,
            },
            NonNull::new(buf.as_mut_ptr()).unwrap(),
        );
    }
    buf
}

/// Read a `CTRun`'s `n` glyph positions (line-origin-relative) — fast pointer or
/// copy.
fn run_positions(run: &CTRun, n: usize) -> Vec<CGPoint> {
    // SAFETY: see `run_glyphs`.
    let ptr = unsafe { run.positions_ptr() };
    if !ptr.is_null() {
        return unsafe { std::slice::from_raw_parts(ptr, n) }.to_vec();
    }
    let mut buf = vec![CGPoint { x: 0.0, y: 0.0 }; n];
    // SAFETY: `buf` holds `n` elements; the range covers the whole run.
    unsafe {
        run.positions(
            CFRange {
                location: 0,
                length: n as isize,
            },
            NonNull::new(buf.as_mut_ptr()).unwrap(),
        );
    }
    buf
}

/// Read a `CTRun`'s `n` glyph advances (pen movement) — fast pointer or copy.
fn run_advances(run: &CTRun, n: usize) -> Vec<CGSize> {
    // SAFETY: see `run_glyphs`.
    let ptr = unsafe { run.advances_ptr() };
    if !ptr.is_null() {
        return unsafe { std::slice::from_raw_parts(ptr, n) }.to_vec();
    }
    let mut buf = vec![
        CGSize {
            width: 0.0,
            height: 0.0
        };
        n
    ];
    // SAFETY: `buf` holds `n` elements; the range covers the whole run.
    unsafe {
        run.advances(
            CFRange {
                location: 0,
                length: n as isize,
            },
            NonNull::new(buf.as_mut_ptr()).unwrap(),
        );
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::face::constraint::Align;
    use crate::font::opentype::head::Head;

    #[test]
    fn face_copies_and_parses_head() {
        let face = Face::new("Menlo", 12.0);
        let bytes = face
            .copy_table(b"head")
            .expect("the font should have a head table");
        let head = Head::from_bytes(&bytes).expect("head table should parse");

        // `magic_number` is `0x5F0F3CF5` in every valid `head` table, regardless
        // of which font CoreText resolved — a version-independent check that the
        // FFI round-trip and parser are correct.
        assert_eq!(head.magic_number, 0x5F0F_3CF5);
        // units-per-em must be in the spec's valid range.
        assert!((16..=16384).contains(&head.units_per_em));
    }

    #[test]
    fn missing_table_is_none() {
        let face = Face::new("Menlo", 12.0);
        // No font has a `ZZZZ` table.
        assert!(face.copy_table(b"ZZZZ").is_none());
    }

    #[test]
    fn scalar_metrics_are_plausible() {
        let face = Face::new("Menlo", 12.0);
        assert_eq!(face.size(), 12.0);
        assert!((16..=16384).contains(&face.units_per_em()));
        assert!(face.ascent() > 0.0);
        assert!(face.descent() > 0.0); // CoreText returns descent positive
        assert!(face.leading() >= 0.0);
        assert!(face.cap_height() > 0.0);
        assert!(face.x_height() > 0.0);
        // Capitals are taller than the x-height.
        assert!(face.cap_height() > face.x_height());
    }

    #[test]
    fn glyph_measurement() {
        let face = Face::new("Menlo", 12.0);
        let glyphs = face.glyphs_for_characters(&[b'M' as u16, b'i' as u16]);
        assert_eq!(glyphs.len(), 2);
        assert!(glyphs.iter().all(|&g| g != 0)); // both chars have glyphs

        let advances = face.advances_for_glyphs(&glyphs);
        assert_eq!(advances.len(), 2);
        assert!(advances.iter().all(|&w| w > 0.0));
        // Menlo is monospaced, so 'M' and 'i' advance identically.
        assert_eq!(advances[0], advances[1]);

        let (w, h) = face.bounding_rect_for_glyphs(&glyphs);
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn empty_glyph_inputs() {
        let face = Face::new("Menlo", 12.0);
        assert!(face.glyphs_for_characters(&[]).is_empty());
        assert!(face.advances_for_glyphs(&[]).is_empty());
        assert_eq!(face.bounding_rect_for_glyphs(&[]), (0.0, 0.0));
    }

    #[test]
    fn get_metrics_is_sane() {
        let fm = Face::new("Menlo", 14.0).get_metrics();
        assert_eq!(fm.px_per_em, 14.0);
        assert!(fm.cell_width > 0.0);
        assert!(fm.ascent > 0.0);
        assert!(fm.descent < 0.0); // below the baseline
        assert!(fm.line_gap >= 0.0);
        assert!(fm.cap_height.unwrap() > 0.0);
        assert!(fm.ex_height.unwrap() > 0.0);
        assert!(fm.cap_height.unwrap() > fm.ex_height.unwrap());
        assert!(fm.ascii_height.unwrap() > 0.0);
    }

    #[test]
    fn get_metrics_feeds_calc() {
        use crate::font::metrics::Metrics;

        let m = Metrics::calc(Face::new("Menlo", 14.0).get_metrics());
        assert!(m.cell_width > 0);
        assert!(m.cell_height > 0);
        assert!(m.cell_baseline <= m.cell_height);
        assert!(m.underline_thickness >= 1);
    }

    #[test]
    fn rasterize_glyph_has_ink() {
        let face = Face::new("Menlo", 32.0);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        let rg = face.rasterize_glyph(glyph).expect("'M' should rasterize");
        assert!(rg.width > 0);
        assert!(rg.height > 0);
        assert_eq!(rg.bitmap.len(), (rg.width * rg.height) as usize);
        // The glyph has ink: a non-trivial fraction of pixels are non-zero.
        let inked = rg.bitmap.iter().filter(|&&b| b != 0).count();
        assert!(inked > 0, "rasterized 'M' has no ink");
        assert!(
            inked * 20 > rg.bitmap.len(),
            "'M' coverage implausibly sparse"
        );
    }

    #[test]
    fn rasterize_space_is_empty_or_none() {
        let face = Face::new("Menlo", 32.0);
        let glyph = face.glyphs_for_characters(&[b' ' as u16])[0];
        // Space has no outline: either None or an all-zero bitmap.
        match face.rasterize_glyph(glyph) {
            None => {}
            Some(rg) => assert!(rg.bitmap.iter().all(|&b| b == 0), "space has ink"),
        }
    }

    /// A `.none`-constraint `RenderOptions` using the face's own grid metrics.
    fn none_opts(face: &Face) -> RenderOptions {
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
    fn render_glyph_places_m_in_atlas() {
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let face = Face::new("Menlo", 32.0);
        let opts = none_opts(&face);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("'M' should render into the atlas");
        assert!(g.width > 0);
        assert!(g.height > 0);
        // 'M' sits above the baseline, so its top bearing is positive.
        assert!(
            g.offset_y > 0,
            "'M' top bearing should be above the baseline"
        );
        // The reserved region fits inside the atlas.
        assert!((g.atlas_x + g.width) as usize <= 512);
        assert!((g.atlas_y + g.height) as usize <= 512);
    }

    #[test]
    fn render_glyph_space_is_zero() {
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let face = Face::new("Menlo", 32.0);
        let opts = none_opts(&face);
        let glyph = face.glyphs_for_characters(&[b' ' as u16])[0];
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("space should render (as a zero glyph)");
        // No outline -> a zero glyph, no atlas reservation.
        assert_eq!(g.width, 0);
        assert_eq!(g.height, 0);
        assert_eq!(g.offset_x, 0);
        assert_eq!(g.offset_y, 0);
        assert_eq!(g.atlas_x, 0);
        assert_eq!(g.atlas_y, 0);
    }

    #[test]
    fn render_glyph_stretch_fills_cell() {
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let face = Face::new("Menlo", 32.0);
        let metrics = Metrics::calc(face.get_metrics());
        // A stretch constraint maps any outline to exactly the cell, so the
        // resulting Glyph is deterministic regardless of the raw bbox.
        let opts = RenderOptions {
            grid_metrics: metrics,
            cell_width: None,
            constraint: Constraint {
                size: Size::Stretch,
                align_horizontal: Align::Start,
                align_vertical: Align::Center1,
                ..Default::default()
            },
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        };
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("'M' should render stretched into the atlas");

        // The constrained size drives the canvas and bearings.
        assert_eq!(g.width, metrics.cell_width);
        assert_eq!(g.height, metrics.cell_height);
        assert_eq!(g.offset_x, 0);
        assert_eq!(g.offset_y, metrics.cell_height as i32);

        // Measure the inked-pixel bounding box within the reserved region. A
        // stretched 'M' fills nearly the whole cell, so it must span most of it
        // in both axes — this fails if `scale_ctm` were omitted (raw 'M' clipped
        // in height) or inverted (glyph shrinks to a dot).
        let size = 512usize;
        let data = atlas.data();
        let (mut min_x, mut min_y) = (g.width, g.height);
        let (mut max_x, mut max_y) = (0u32, 0u32);
        let mut inked = false;
        for row in 0..g.height {
            for col in 0..g.width {
                let px = data[((g.atlas_y + row) as usize) * size + (g.atlas_x + col) as usize];
                if px != 0 {
                    inked = true;
                    min_x = min_x.min(col);
                    min_y = min_y.min(row);
                    max_x = max_x.max(col);
                    max_y = max_y.max(row);
                }
            }
        }
        assert!(inked, "stretched 'M' produced no ink");
        let ink_w = max_x - min_x + 1;
        let ink_h = max_y - min_y + 1;
        assert!(
            ink_w as f64 >= 0.8 * g.width as f64,
            "ink width {ink_w} should span most of the {} cell",
            g.width
        );
        assert!(
            ink_h as f64 >= 0.8 * g.height as f64,
            "ink height {ink_h} should span most of the {} cell",
            g.height
        );
    }

    #[test]
    fn render_glyph_thicken_pads_canvas() {
        let face = Face::new("Menlo", 32.0);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];

        let mut plain_atlas = Atlas::new(512, Format::Grayscale);
        let plain_opts = none_opts(&face);
        let plain = face
            .render_glyph(&mut plain_atlas, glyph, &plain_opts)
            .expect("plain 'M' should render");

        let mut thick_atlas = Atlas::new(512, Format::Grayscale);
        let thick_opts = RenderOptions {
            thicken: true,
            ..none_opts(&face)
        };
        let thick = face
            .render_glyph(&mut thick_atlas, glyph, &thick_opts)
            .expect("thick 'M' should render");

        // Thicken adds one pixel of canvas padding on every edge: the canvas
        // grows by two per axis, the left bearing moves out by one, and the top
        // bearing moves up by one.
        assert_eq!(thick.width, plain.width + 2);
        assert_eq!(thick.height, plain.height + 2);
        assert_eq!(thick.offset_x, plain.offset_x - 1);
        assert_eq!(thick.offset_y, plain.offset_y + 1);

        // Both still have ink.
        assert!(plain_atlas.data().iter().any(|&b| b != 0));
        assert!(thick_atlas.data().iter().any(|&b| b != 0));
    }

    #[test]
    fn render_glyph_strength_dims_fill() {
        let face = Face::new("Menlo", 32.0);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];

        // The gray fill is `thicken_strength / 255`, so a lower strength caps the
        // glyph's coverage darker — the brightest pixel is dimmer.
        let max_pixel = |strength: u8| -> u8 {
            let mut atlas = Atlas::new(512, Format::Grayscale);
            let opts = RenderOptions {
                thicken_strength: strength,
                ..none_opts(&face)
            };
            let g = face
                .render_glyph(&mut atlas, glyph, &opts)
                .expect("'M' should render");
            let size = 512usize;
            let data = atlas.data();
            let mut max = 0u8;
            for row in 0..g.height {
                for col in 0..g.width {
                    let px = data[((g.atlas_y + row) as usize) * size + (g.atlas_x + col) as usize];
                    max = max.max(px);
                }
            }
            max
        };

        assert!(
            max_pixel(255) > max_pixel(64),
            "a stronger fill should reach a brighter peak coverage"
        );
    }

    #[test]
    fn new_face_has_no_synthetic_bold() {
        assert_eq!(Face::new("Menlo", 32.0).synthetic_bold, None);
    }

    #[test]
    fn new_synthetic_bold_sets_width() {
        let face = Face::new_synthetic_bold("Menlo", 32.0);
        assert_eq!(face.synthetic_bold, Some((32.0_f64 / 14.0).max(1.0)));
    }

    #[test]
    fn synthetic_bold_is_heavier() {
        // Total ink (sum of pixel coverage) and canvas size for a face's 'M'.
        let measure = |face: &Face| -> (u64, u32, u32) {
            let mut atlas = Atlas::new(512, Format::Grayscale);
            let opts = none_opts(face);
            let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
            let g = face
                .render_glyph(&mut atlas, glyph, &opts)
                .expect("'M' should render");
            let size = 512usize;
            let data = atlas.data();
            let mut sum = 0u64;
            for row in 0..g.height {
                for col in 0..g.width {
                    let px = data[((g.atlas_y + row) as usize) * size + (g.atlas_x + col) as usize];
                    sum += px as u64;
                }
            }
            (sum, g.width, g.height)
        };

        let plain = Face::new("Menlo", 32.0);
        let bold = Face::new_synthetic_bold("Menlo", 32.0);
        let (plain_ink, plain_w, plain_h) = measure(&plain);
        let (bold_ink, bold_w, bold_h) = measure(&bold);

        // The grown rect makes the bold canvas at least as large.
        assert!(bold_w >= plain_w, "bold width {bold_w} < plain {plain_w}");
        assert!(bold_h >= plain_h, "bold height {bold_h} < plain {plain_h}");
        // Fill-stroke makes the bold glyph strictly heavier.
        assert!(
            bold_ink > plain_ink,
            "bold ink {bold_ink} should exceed plain ink {plain_ink}"
        );
    }

    #[test]
    fn text_font_has_no_color() {
        let face = Face::new("Menlo", 32.0);
        assert!(!face.has_color());
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        assert!(!face.is_color_glyph(glyph));
    }

    #[test]
    fn emoji_font_has_color() {
        let face = Face::new("Apple Color Emoji", 32.0);
        assert!(face.has_color(), "Apple Color Emoji should be a color font");

        // 😀 U+1F600 is outside the BMP, so encode it as a UTF-16 surrogate pair
        // and take the first resolved glyph.
        let utf16: Vec<u16> = '\u{1F600}'.encode_utf16(&mut [0u16; 2]).to_vec();
        let glyph = face.glyphs_for_characters(&utf16)[0];
        assert_ne!(
            glyph, 0,
            "the emoji glyph should resolve (no font fallback)"
        );
        assert!(face.is_color_glyph(glyph));
    }

    #[test]
    fn render_color_glyph_into_bgra_atlas() {
        let face = Face::new("Apple Color Emoji", 32.0);
        let utf16: Vec<u16> = '\u{1F600}'.encode_utf16(&mut [0u16; 2]).to_vec();
        let glyph = face.glyphs_for_characters(&utf16)[0];
        assert_ne!(glyph, 0, "the emoji glyph should resolve");

        let mut atlas = Atlas::new(1024, Format::Bgra);
        let opts = RenderOptions {
            grid_metrics: Metrics::calc(face.get_metrics()),
            cell_width: None,
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        };
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("the emoji should render into the BGRA atlas");
        assert!(g.width > 0);
        assert!(g.height > 0);
        assert!((g.atlas_x + g.width) as usize <= 1024);
        assert!((g.atlas_y + g.height) as usize <= 1024);

        // The region must contain real color — at least one pixel with a
        // non-zero color channel (not just alpha).
        let size = 1024usize;
        let data = atlas.data();
        let mut has_color = false;
        for row in 0..g.height {
            for col in 0..g.width {
                // BGRA: bytes 0..3 are B,G,R (premultiplied), byte 3 is A.
                let base = (((g.atlas_y + row) as usize) * size + (g.atlas_x + col) as usize) * 4;
                if data[base] != 0 || data[base + 1] != 0 || data[base + 2] != 0 {
                    has_color = true;
                }
            }
        }
        assert!(has_color, "the emoji should render with non-zero color");
    }

    #[test]
    fn font_renderer_residual_color_sbix_thicken_skips_canvas_padding() {
        let face = Face::new("Apple Color Emoji", 32.0);
        assert!(
            face.color.as_ref().is_some_and(|c| c.sbix),
            "Apple Color Emoji should exercise the sbix bitmap-color path"
        );

        let utf16: Vec<u16> = '\u{1F600}'.encode_utf16(&mut [0u16; 2]).to_vec();
        let glyph = face.glyphs_for_characters(&utf16)[0];
        assert_ne!(glyph, 0, "the emoji glyph should resolve");

        let opts = RenderOptions {
            grid_metrics: Metrics::calc(face.get_metrics()),
            cell_width: None,
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        };
        let mut plain_atlas = Atlas::new(1024, Format::Bgra);
        let plain = face
            .render_glyph(&mut plain_atlas, glyph, &opts)
            .expect("plain sbix emoji should render");

        let mut thick_atlas = Atlas::new(1024, Format::Bgra);
        let thick = face
            .render_glyph(
                &mut thick_atlas,
                glyph,
                &RenderOptions {
                    thicken: true,
                    thicken_strength: 64,
                    ..opts
                },
            )
            .expect("thickened sbix emoji should render");

        assert_eq!(thick.width, plain.width, "sbix thicken skips width padding");
        assert_eq!(
            thick.height, plain.height,
            "sbix thicken skips height padding"
        );
        assert_eq!(
            thick.offset_x, plain.offset_x,
            "sbix thicken keeps x bearing"
        );
        assert_eq!(
            thick.offset_y, plain.offset_y,
            "sbix thicken keeps y bearing"
        );
    }

    #[test]
    fn mono_glyph_still_renders() {
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let face = Face::new("Menlo", 32.0);
        let opts = none_opts(&face);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("'M' should still render into the grayscale atlas");
        assert!(g.width > 0 && g.height > 0);
        assert!(atlas.data().iter().any(|&b| b != 0), "'M' has ink");
    }

    #[test]
    fn wrong_atlas_format_errors() {
        // A color glyph needs a BGRA atlas; a grayscale atlas is rejected.
        let emoji = Face::new("Apple Color Emoji", 32.0);
        let utf16: Vec<u16> = '\u{1F600}'.encode_utf16(&mut [0u16; 2]).to_vec();
        let color_glyph = emoji.glyphs_for_characters(&utf16)[0];
        let mut gray = Atlas::new(512, Format::Grayscale);
        let emoji_opts = RenderOptions {
            grid_metrics: Metrics::calc(emoji.get_metrics()),
            cell_width: None,
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        };
        assert_eq!(
            emoji.render_glyph(&mut gray, color_glyph, &emoji_opts),
            Err(RenderGlyphError::InvalidAtlasFormat)
        );

        // A mono glyph needs a grayscale atlas; a BGRA atlas is rejected.
        let text = Face::new("Menlo", 32.0);
        let mono_glyph = text.glyphs_for_characters(&[b'M' as u16])[0];
        let mut bgra = Atlas::new(512, Format::Bgra);
        assert_eq!(
            text.render_glyph(&mut bgra, mono_glyph, &none_opts(&text)),
            Err(RenderGlyphError::InvalidAtlasFormat)
        );
    }

    #[test]
    fn glyph_index_maps_codepoints() {
        let menlo = Face::new("Menlo", 32.0);
        // A basic ASCII glyph resolves to a non-zero id.
        assert!(menlo.glyph_index('M' as u32).is_some_and(|g| g != 0));
        // A Private-Use codepoint Menlo doesn't cover resolves to None.
        assert_eq!(menlo.glyph_index(0xE000), None);
        // A non-scalar codepoint (a lone surrogate) is None.
        assert_eq!(menlo.glyph_index(0xD800), None);

        // A non-BMP emoji resolves via the surrogate-pair path in its font.
        let emoji = Face::new("Apple Color Emoji", 32.0);
        assert!(emoji.glyph_index(0x1F600).is_some_and(|g| g != 0));
    }

    /// Total ink (sum of grayscale coverage) of `'M'` rendered by `face`.
    fn m_ink(face: &Face) -> u64 {
        let mut atlas = Atlas::new(512, Format::Grayscale);
        let opts = none_opts(face);
        let glyph = face.glyphs_for_characters(&[b'M' as u16])[0];
        let g = face
            .render_glyph(&mut atlas, glyph, &opts)
            .expect("'M' should render");
        let size = 512usize;
        let data = atlas.data();
        let mut sum = 0u64;
        for row in 0..g.height {
            for col in 0..g.width {
                sum +=
                    data[((g.atlas_y + row) as usize) * size + (g.atlas_x + col) as usize] as u64;
            }
        }
        sum
    }

    #[test]
    fn synthetic_bold_method_sets_width() {
        let menlo = Face::new("Menlo", 28.0);
        let bold = menlo.synthetic_bold();
        assert_eq!(bold.synthetic_bold, Some((28.0_f64 / 14.0).max(1.0)));
        // The synthetic-bold 'M' is heavier than the plain one.
        assert!(m_ink(&bold) > m_ink(&menlo));
    }

    #[test]
    fn synthetic_italic_renders() {
        let italic = Face::new("Menlo", 32.0).synthetic_italic();
        assert_eq!(italic.synthetic_bold, None);
        assert!(italic.glyph_index('M' as u32).is_some());
        // It renders with ink.
        assert!(m_ink(&italic) > 0);

        // The skew matrix was actually applied (not a null matrix).
        // SAFETY: `italic.font` is a live `CTFont`.
        let m = unsafe { italic.font.matrix() };
        assert_eq!(m.a, ITALIC_SKEW.a);
        assert_eq!(m.b, ITALIC_SKEW.b);
        assert_eq!(m.c, ITALIC_SKEW.c);
        assert_eq!(m.d, ITALIC_SKEW.d);
        assert_eq!(m.tx, ITALIC_SKEW.tx);
        assert_eq!(m.ty, ITALIC_SKEW.ty);
    }

    #[test]
    fn synthetic_face_inherits_color_detection() {
        // A text font's synthetic variants are still non-color.
        assert!(!Face::new("Menlo", 32.0).synthetic_italic().has_color());
        assert!(!Face::new("Menlo", 32.0).synthetic_bold().has_color());
    }

    #[test]
    fn set_size_resizes() {
        let mut face = Face::new("Menlo", 32.0);
        assert!((face.size() - 32.0).abs() < 1e-6);
        face.set_size(20.0);
        assert!((face.size() - 20.0).abs() < 1e-6);
        // The resized face still resolves and renders 'M'.
        assert!(face.glyph_index('M' as u32).is_some());
        assert!(m_ink(&face) > 0);
    }

    #[test]
    fn set_size_preserves_synthetic_bold() {
        let mut face = Face::new_synthetic_bold("Menlo", 32.0);
        assert!(face.synthetic_bold.is_some());
        face.set_size(24.0);
        assert!((face.size() - 24.0).abs() < 1e-6);
        // The synthetic-bold marker survives, with its width recomputed for the
        // new size (not the stale 32pt width).
        assert_eq!(face.synthetic_bold, Some((24.0_f64 / 14.0).max(1.0)));
    }

    #[test]
    fn color_state_svg_branch() {
        // A synthetic SVG-only color state (no sbix): glyph 5 has an SVG
        // document, so it is a color glyph; its neighbors are not. A macOS system
        // font with an `SVG ` table is not guaranteed, so the SVG branch of
        // `ColorState::is_color_glyph` is exercised with a hand-built table.
        let mut table = Vec::new();
        table.extend_from_slice(&0u16.to_be_bytes()); // version 0
        table.extend_from_slice(&6u32.to_be_bytes()); // doc-list offset
        table.extend_from_slice(&1u16.to_be_bytes()); // numEntries = 1
        table.extend_from_slice(&5u16.to_be_bytes()); // startGlyphID
        table.extend_from_slice(&5u16.to_be_bytes()); // endGlyphID
        table.extend_from_slice(&0u32.to_be_bytes()); // svgDocOffset
        table.extend_from_slice(&0u32.to_be_bytes()); // svgDocLength
        let svg = Svg::from_bytes(&table).expect("parses");

        let cs = ColorState {
            sbix: false,
            svg: Some(svg),
        };
        assert!(cs.is_color_glyph(5), "glyph 5 has an SVG document");
        assert!(!cs.is_color_glyph(6), "glyph 6 does not");

        // sbix short-circuits regardless of the SVG table.
        let cs_sbix = ColorState {
            sbix: true,
            svg: None,
        };
        assert!(cs_sbix.is_color_glyph(99), "sbix colors every glyph");
    }

    #[test]
    fn font_for_codepoint_cjk() {
        // Menlo lacks CJK; CoreText's CTFontCreateForString finds a CJK font.
        let menlo = Face::new("Menlo", 24.0);
        let face = menlo
            .font_for_codepoint(0x4E00)
            .expect("a CJK font for U+4E00");
        assert!(
            face.glyph_index(0x4E00).is_some(),
            "the discovered font renders U+4E00"
        );
    }

    #[test]
    fn font_for_codepoint_ascii() {
        let menlo = Face::new("Menlo", 24.0);
        let face = menlo
            .font_for_codepoint('M' as u32)
            .expect("a font for 'M'");
        assert!(face.glyph_index('M' as u32).is_some(), "renders 'M'");
    }

    #[test]
    fn font_for_codepoint_supplementary() {
        // U+1F600 is a supplementary scalar (UTF-16 surrogate pair, len 2).
        let menlo = Face::new("Menlo", 24.0);
        let face = menlo
            .font_for_codepoint(0x1F600)
            .expect("a font for the emoji");
        assert!(face.glyph_index(0x1F600).is_some(), "renders the emoji");
    }

    #[test]
    fn font_for_codepoint_none() {
        // U+FDD0 is a permanent noncharacter no font covers; CoreText returns
        // its LastResort font, which `font_for_codepoint` rejects as `None`.
        let menlo = Face::new("Menlo", 24.0);
        assert!(
            menlo.font_for_codepoint(0xFDD0).is_none(),
            "a noncharacter resolves to no real font"
        );
    }

    #[test]
    fn shape_ascii_monospace() {
        // Menlo is monospace with no ASCII ligatures: 3 codepoints → 3 glyphs in
        // order, each glyph matching the face's direct cmap lookup.
        let face = Face::new("Menlo", 24.0);
        let cps = ['A' as u32, 'B' as u32, 'C' as u32];
        let cells = face.shape_codepoints(&cps);
        assert_eq!(cells.len(), 3, "one cell per codepoint");
        for (i, &cp) in cps.iter().enumerate() {
            assert_eq!(
                cells[i].glyph_index as u16,
                face.glyph_index(cp).expect("a glyph"),
                "cell {i} shapes to the cmap glyph"
            );
            assert_eq!(
                cells[i].x, i as u16,
                "x is the cluster (== index for ASCII)"
            );
        }
    }

    #[test]
    fn shape_single() {
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&['Z' as u32]);
        assert_eq!(cells.len(), 1);
        assert_eq!(
            cells[0].glyph_index as u16,
            face.glyph_index('Z' as u32).expect("a glyph")
        );
    }

    #[test]
    fn shape_empty() {
        let face = Face::new("Menlo", 24.0);
        assert!(face.shape_codepoints(&[]).is_empty());
    }

    #[test]
    fn shape_plain_offsets_zero() {
        // Plain monospace ASCII sits exactly at the pen, on the baseline: the
        // advance-based x_offset and y_offset round to zero for every glyph.
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&['A' as u32, 'B' as u32, 'C' as u32]);
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(c.x_offset, 0, "cell {i} x_offset is zero for plain text");
            assert_eq!(c.y_offset, 0, "cell {i} y_offset is zero for plain text");
        }
    }

    #[test]
    fn shape_advances_monotonic() {
        // The advance accumulation does not corrupt the per-glyph output: one
        // cell per codepoint, with non-decreasing string-index `x`.
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&['x' as u32, 'y' as u32, 'z' as u32]);
        assert_eq!(cells.len(), 3, "one cell per codepoint");
        assert!(
            cells.windows(2).all(|w| w[0].x <= w[1].x),
            "the cell x positions are non-decreasing"
        );
    }

    #[test]
    fn shape_ltr_stays_sorted() {
        // Pure LTR text never trips `non_ltr`; the 1:1 monospace cells keep their
        // string-index order (x = 0, 1, 2) and the buffer stays grid-ordered.
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&['A' as u32, 'B' as u32, 'C' as u32]);
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(c.x, i as u16, "cell {i} keeps its string-index x");
        }
    }

    #[test]
    fn shape_rtl_grid_ordered() {
        // Hebrew "שלום". On a host whose CoreText shapes this RTL, the raw run
        // order is reversed; the non-LTR sort restores ascending `x`. The
        // post-condition (cells sorted by `x`) holds regardless of the fallback
        // font the host picks, so this is robust.
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&[0x05E9, 0x05DC, 0x05D5, 0x05DD]);
        assert!(
            cells.windows(2).all(|w| w[0].x <= w[1].x),
            "the cell x positions are non-decreasing after the non-LTR sort"
        );
    }

    #[test]
    fn shape_clusters_monospace() {
        // Each ASCII scalar is its own cluster/cell: x = 0, 1, 2 and, since the
        // cell origin resets to the pen at each, every x_offset is 0.
        let face = Face::new("Menlo", 24.0);
        let cells = face.shape_codepoints(&['A' as u32, 'B' as u32, 'C' as u32]);
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(c.x, i as u16, "cell {i} maps to its cluster");
            assert_eq!(c.x_offset, 0, "cell {i} sits at the cell origin");
        }
    }

    #[test]
    fn shape_cluster_collapses_surrogate() {
        // 'A' (cluster 0), U+1D400 𝐀 (non-BMP, cluster 1, UTF-16 units 1–2), then
        // 'B' (cluster 2, UTF-16 unit 3). CoreText assigns 'B' string index 3; the
        // cluster table collapses the surrogate pair so 'B' maps to cluster 2 —
        // NOT the raw UTF-16 index 3. We find the 'B' cell by its cmap glyph (the
        // run order may vary with the host's font for 𝐀).
        let face = Face::new("Menlo", 24.0);
        let b_glyph = face.glyph_index('B' as u32).expect("a glyph for 'B'") as u32;
        let cells = face.shape_codepoints(&['A' as u32, 0x1D400, 'B' as u32]);
        let b_cell = cells
            .iter()
            .find(|c| c.glyph_index == b_glyph)
            .expect("the 'B' cell");
        assert_eq!(
            b_cell.x, 2,
            "'B' maps to its cluster (2), collapsing the surrogate pair, not the UTF-16 index (3)"
        );
    }

    #[test]
    fn shape_run_combining_marks() {
        // 'n' + two U+0308 combining diaereses grouped into cell 0 (one grapheme),
        // then 'a' in cell 1. The marks share the base's cluster, so they fold
        // into cell 0 — NOT cells 1/2 the sequential mapping would produce. Robust
        // to how many glyphs the host emits for the grapheme.
        let face = Face::new("Menlo", 24.0);
        let run = [
            shape::Codepoint {
                codepoint: 'n' as u32,
                cluster: 0,
            },
            shape::Codepoint {
                codepoint: 0x0308,
                cluster: 0,
            },
            shape::Codepoint {
                codepoint: 0x0308,
                cluster: 0,
            },
            shape::Codepoint {
                codepoint: 'a' as u32,
                cluster: 1,
            },
        ];
        let cells = face.shape_run(&run);
        assert!(
            !cells.is_empty(),
            "the grapheme shapes to at least one cell"
        );
        assert!(
            cells.iter().all(|c| c.x <= 1),
            "every cell maps to cluster 0 or 1 (the marks fold into the base's cell)"
        );
        assert!(
            cells.iter().any(|c| c.x == 0),
            "the base/marks occupy cell 0"
        );
        assert_eq!(cells.last().unwrap().x, 1, "'a' occupies its own cell (1)");
    }

    #[test]
    fn shape_run_matches_sequential_wrapper() {
        // `shape_codepoints` is exactly `shape_run` with sequential clusters.
        let face = Face::new("Menlo", 24.0);
        let cps = ['A' as u32, 'B' as u32, 'C' as u32];
        let run: Vec<shape::Codepoint> = cps
            .iter()
            .enumerate()
            .map(|(i, &codepoint)| shape::Codepoint {
                codepoint,
                cluster: i as u32,
            })
            .collect();
        assert_eq!(face.shape_codepoints(&cps), face.shape_run(&run));
    }

    #[test]
    fn shape_run_reorder_skips_reset() {
        // Mechanically exercises the `is_after` reorder guard (not full
        // complex-shaping). Synthetic descending clusters [2, 1, 0] over ABC: 'A'
        // resets to cell 2, then 'B' (1 ≤ 2) and 'C' (0 ≤ 2) are from clusters
        // already passed, so their resets are skipped and they inherit cell 2.
        // Under the unconditional reset this would have been [2, 1, 0]. Menlo
        // emits one glyph per ASCII scalar in order, so `is_first` is true at each
        // transition (it does not change this case); the `is_after` guard alone
        // drives the skips.
        let face = Face::new("Menlo", 24.0);
        let run = [
            shape::Codepoint {
                codepoint: 'A' as u32,
                cluster: 2,
            },
            shape::Codepoint {
                codepoint: 'B' as u32,
                cluster: 1,
            },
            shape::Codepoint {
                codepoint: 'C' as u32,
                cluster: 0,
            },
        ];
        let cells = face.shape_run(&run);
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(
                c.x, 2,
                "cell {i} inherits cell 2 (the reorder guard skips its reset)"
            );
        }
    }

    #[test]
    fn shape_run_forward_clusters_unchanged() {
        // Forward clusters never trip `is_after`, so the reset always happens and
        // monotonic runs are undisturbed: x = 0, 1, 2.
        let face = Face::new("Menlo", 24.0);
        let run = [
            shape::Codepoint {
                codepoint: 'A' as u32,
                cluster: 0,
            },
            shape::Codepoint {
                codepoint: 'B' as u32,
                cluster: 1,
            },
            shape::Codepoint {
                codepoint: 'C' as u32,
                cluster: 2,
            },
        ];
        let cells = face.shape_run(&run);
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(c.x, i as u16, "cell {i} keeps its forward cluster");
        }
    }

    #[test]
    fn feature_settings_descriptor_some_none() {
        assert!(
            feature_settings_descriptor(&[]).is_none(),
            "an empty feature list yields no descriptor"
        );
        let desc = feature_settings_descriptor(&[shape::Feature {
            tag: *b"liga",
            value: 1,
        }]);
        assert!(
            desc.is_some(),
            "a non-empty feature list yields a descriptor"
        );
    }

    #[test]
    fn shape_run_with_default_features() {
        // Shaping now copies the font with the default-feature descriptor; plain
        // monospace shaping is unaffected (Menlo has no `liga`).
        let face = Face::new("Menlo", 24.0);
        let cps = ['A' as u32, 'B' as u32, 'C' as u32];
        let cells = face.shape_codepoints(&cps);
        assert_eq!(
            cells.len(),
            3,
            "default features do not break plain shaping"
        );
        for (i, &cp) in cps.iter().enumerate() {
            assert_eq!(
                cells[i].glyph_index as u16,
                face.glyph_index(cp).expect("a glyph"),
                "cell {i} still shapes to the cmap glyph"
            );
        }
    }

    #[test]
    fn shape_run_options_regression() {
        // The merged-features path with default options matches the default path.
        let face = Face::new("Menlo", 24.0);
        let cps = ['A' as u32, 'B' as u32, 'C' as u32];
        let run: Vec<shape::Codepoint> = cps
            .iter()
            .enumerate()
            .map(|(i, &codepoint)| shape::Codepoint {
                codepoint,
                cluster: i as u32,
            })
            .collect();
        assert_eq!(
            face.shape_run_options(&run, &shape::Options::default()),
            face.shape_codepoints(&cps)
        );
    }

    #[test]
    fn feature_settings_descriptor_skips_out_of_range() {
        // A value exceeding `i32::MAX` is skipped rather than wrapping.
        let too_big = shape::Feature {
            tag: *b"aalt",
            value: i32::MAX as u32 + 1,
        };
        assert!(
            feature_settings_descriptor(&[too_big]).is_none(),
            "a lone out-of-range feature yields no descriptor"
        );
        let liga = shape::Feature {
            tag: *b"liga",
            value: 1,
        };
        assert!(
            feature_settings_descriptor(&[liga, too_big]).is_some(),
            "a valid feature survives alongside a skipped out-of-range one"
        );
    }

    #[test]
    fn is_first_codepoint_in_cluster_walk() {
        // The extracted backward walk, over synthetic clusters/pads arrays.
        // No predecessor → first.
        assert!(is_first_codepoint_in_cluster(&[5], &[false], 0, 5));
        // Nearest real predecessor in a different cluster → first.
        assert!(is_first_codepoint_in_cluster(
            &[3, 5],
            &[false, false],
            1,
            5
        ));
        // Nearest real predecessor in the same cluster → not first.
        assert!(!is_first_codepoint_in_cluster(
            &[5, 5],
            &[false, false],
            1,
            5
        ));
        // A surrogate-pad unit is skipped to reach the real predecessor.
        assert!(!is_first_codepoint_in_cluster(
            &[5, 5, 5],
            &[false, true, false],
            2,
            5
        ));
        assert!(is_first_codepoint_in_cluster(
            &[3, 3, 5],
            &[false, true, false],
            2,
            5
        ));
        // Only padding precedes → first (the lone predecessor is skipped).
        assert!(is_first_codepoint_in_cluster(&[9, 9], &[true, false], 1, 9));
    }

    #[test]
    fn shape_run_full_condition_regression() {
        // The full `is_first && !is_after` condition leaves the Exp 343 outcomes
        // intact: `is_first` is true at each reset-relevant cluster transition.
        let face = Face::new("Menlo", 24.0);
        let mk = |cps: &[(u32, u32)]| -> Vec<shape::Codepoint> {
            cps.iter()
                .map(|&(codepoint, cluster)| shape::Codepoint { codepoint, cluster })
                .collect()
        };

        // Reorder [2, 1, 0] still folds to cell 2.
        let cells = face.shape_run(&mk(&[('A' as u32, 2), ('B' as u32, 1), ('C' as u32, 0)]));
        assert_eq!(cells.len(), 3);
        assert!(
            cells.iter().all(|c| c.x == 2),
            "reorder still inherits cell 2"
        );

        // Forward [0, 1, 2] still maps 1:1.
        let cells = face.shape_run(&mk(&[('A' as u32, 0), ('B' as u32, 1), ('C' as u32, 2)]));
        assert_eq!(cells.len(), 3);
        for (i, c) in cells.iter().enumerate() {
            assert_eq!(c.x, i as u16, "forward cell {i} unchanged");
        }

        // Combining marks [0, 0, 0, 1] still fold into cell 0, with 'a' in cell 1.
        let cells = face.shape_run(&mk(&[
            ('n' as u32, 0),
            (0x0308, 0),
            (0x0308, 0),
            ('a' as u32, 1),
        ]));
        assert!(!cells.is_empty());
        assert!(
            cells.iter().all(|c| c.x <= 1),
            "marks still fold into cell 0/1"
        );
        assert_eq!(cells.last().unwrap().x, 1, "'a' still in cell 1");
    }

    #[test]
    fn set_variations_empty_noop() {
        // No variations → the face is left usable and unchanged.
        let mut face = Face::new("Menlo", 24.0);
        let before = face.glyph_index('A' as u32);
        assert!(before.is_some(), "Menlo renders 'A'");
        face.set_variations(&[]);
        assert_eq!(face.glyph_index('A' as u32), before, "unchanged by no-op");
    }

    #[test]
    fn set_variations_runs_on_face() {
        // Setting a `wght` axis on a non-variable font does not crash and leaves a
        // usable face (CoreText returns a valid copy even with no matching axis).
        // Exercises the descriptor-copy + font-rebuild path end to end.
        let mut face = Face::new("Menlo", 24.0);
        face.set_variations(&[Variation {
            id: Variation::id_from_tag(b"wght"),
            value: 700.0,
        }]);
        assert!(
            face.glyph_index('A' as u32).is_some(),
            "the varied face still renders 'A'"
        );
    }
}
