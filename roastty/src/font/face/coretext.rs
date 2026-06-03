//! A CoreText-backed font face (`CTFont`).
//!
//! Faithful (macOS) port of the `CTFont` plumbing in upstream
//! `font/face/coretext.zig`. This slice provides face construction and raw
//! OpenType table access (`CTFontCopyTable`), the building block
//! `Face::get_metrics` will use to read `head`/`hhea`/`OS/2`/`post`. The full
//! metric assembly and glyph rasterization land in later experiments.

use std::ptr::NonNull;

use objc2_core_foundation::{CFRetained, CFString, CGSize};
use objc2_core_text::{CTFont, CTFontOrientation, CTFontTableOptions};

/// A font face backed by a CoreText `CTFont`. `CFRetained` manages the
/// underlying CoreFoundation retain/release.
pub(crate) struct Face {
    font: CFRetained<CTFont>,
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
        Face { font }
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
