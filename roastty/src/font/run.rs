//! The run iterator — grouping a terminal row's cells into shaping runs.
//!
//! Faithful (in progress) port of upstream `font/shaper/run.zig`. This slice
//! provides the pure decision helpers of `RunIterator.next()`: the bold/italic
//! style mapping, the bad-ligature run split, and the grapheme presentation
//! derivation. The cell-walking `next()` loop (which extracts these values from a
//! terminal `Cell`), `comparableStyle`, the selection/cursor/spacer breaks, and
//! the `TextRun` value type are later sub-areas.

use std::hash::{Hash, Hasher};

use crate::font::collection::Index;
use crate::font::shape::Codepoint;
use crate::font::{Presentation, Style};
use crate::terminal::style::{Color, Style as TermStyle};

/// The position-independent content hash of a run — a shaping-cache key. Hashes
/// each codepoint's `(codepoint, cluster)` (clusters are run-relative, so the hash
/// is position-independent), then the run's `cell_count` and `font_index`.
/// Faithful port of `RunIterator.next()`'s hash construction.
///
/// Like [`crate::font::discovery::Descriptor::hashcode`], the concrete hasher is
/// roastty's deterministic `DefaultHasher` rather than upstream's Wyhash — the
/// value is an internal cache key, so only the content, order, and determinism
/// matter, not the exact number.
pub(crate) fn run_hash(codepoints: &[Codepoint], cell_count: u16, font_index: Index) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for cp in codepoints {
        cp.codepoint.hash(&mut h); // codepoint first…
        cp.cluster.hash(&mut h); // …then the run-relative cluster
    }
    cell_count.hash(&mut h); // the run's cell count
    font_index.int().hash(&mut h); // the run's font index (packed `u16`)
    h.finish()
}

/// A single text run produced by the run iterator: one row's worth of cells that
/// share a font and a comparable style. Faithful port of upstream `TextRun` — the
/// `grid` pointer is omitted (roastty resolves the face from `font_index` via the
/// `CodepointResolver` at the call site rather than carrying the grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextRun {
    /// A position-independent content hash, for caching shaping results.
    pub hash: u64,
    /// The run's start column in the row (added to each shaped cell's `x`).
    pub offset: u16,
    /// The number of cells the run produced.
    pub cells: u16,
    /// The font index for the run's glyphs.
    pub font_index: Index,
}

/// The style that must be identical for a run to continue: the cell's style with
/// the background color cleared. Background colors may differ within a run — the
/// cell background is painted regardless and the glyph lands on top in its own
/// color. Faithful port of upstream `comparableStyle`.
pub(crate) fn comparable_style(mut style: TermStyle) -> TermStyle {
    style.bg_color = Color::None;
    style
}

/// The font [`Style`] for a cell's bold/italic flags. Faithful port of upstream
/// `RunIterator.next()`'s `font_style` derivation (bold-with-italic is
/// bold-italic, not just bold).
pub(crate) fn font_style(bold: bool, italic: bool) -> Style {
    match (bold, italic) {
        (true, true) => Style::BoldItalic,
        (true, false) => Style::Bold,
        (false, true) => Style::Italic,
        (false, false) => Style::Regular,
    }
}

/// Whether a run should split between two adjacent plain codepoints to avoid a
/// commonly-undesirable ligature (`fl`, `fi`, `st`). Directional: `prev_cp`
/// precedes `cp`. Faithful port of upstream `RunIterator.next()`'s bad-ligature
/// break. (The caller applies the `content_tag == codepoint` guard — both cells
/// must be plain codepoints — before calling this.)
pub(crate) fn is_bad_ligature_break(prev_cp: u32, cp: u32) -> bool {
    // `const` bindings so the match arms read as the ASCII letters (a cast
    // expression like `b'f' as u32` is not a valid match pattern).
    const F: u32 = b'f' as u32;
    const L: u32 = b'l' as u32;
    const I: u32 = b'i' as u32;
    const S: u32 = b's' as u32;
    const T: u32 = b't' as u32;
    match prev_cp {
        F => cp == L || cp == I,
        S => cp == T,
        _ => false,
    }
}

/// The explicit presentation a grapheme's first codepoint forces, or `None`. A
/// variation selector `U+FE0E` forces text and `U+FE0F` forces emoji; any other
/// first codepoint leaves the presentation to the font grid's default. Faithful
/// port of upstream `RunIterator.next()`'s grapheme presentation derivation.
pub(crate) fn presentation_for_grapheme(first_cp: u32) -> Option<Presentation> {
    match first_cp {
        0xFE0E => Some(Presentation::Text),
        0xFE0F => Some(Presentation::Emoji),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_style_combinations() {
        assert_eq!(font_style(false, false), Style::Regular);
        assert_eq!(font_style(true, false), Style::Bold);
        assert_eq!(font_style(false, true), Style::Italic);
        assert_eq!(font_style(true, true), Style::BoldItalic);
    }

    #[test]
    fn bad_ligature_breaks() {
        // The undesirable ligatures split the run.
        assert!(is_bad_ligature_break('f' as u32, 'l' as u32));
        assert!(is_bad_ligature_break('f' as u32, 'i' as u32));
        assert!(is_bad_ligature_break('s' as u32, 't' as u32));
        // Everything else does not.
        assert!(!is_bad_ligature_break('f' as u32, 'x' as u32));
        assert!(!is_bad_ligature_break('s' as u32, 'x' as u32));
        assert!(!is_bad_ligature_break('a' as u32, 'b' as u32));
        // Directional: the reverse pair does not break.
        assert!(!is_bad_ligature_break('l' as u32, 'f' as u32));
        assert!(!is_bad_ligature_break('t' as u32, 's' as u32));
    }

    #[test]
    fn presentation_for_grapheme_selectors() {
        assert_eq!(presentation_for_grapheme(0xFE0E), Some(Presentation::Text));
        assert_eq!(presentation_for_grapheme(0xFE0F), Some(Presentation::Emoji));
        assert_eq!(presentation_for_grapheme('a' as u32), None);
        assert_eq!(presentation_for_grapheme(0x200D), None);
    }

    #[test]
    fn comparable_style_clears_bg() {
        let mut s = TermStyle::default();
        s.bg_color = Color::Palette(5);
        s.fg_color = Color::Palette(3);
        s.flags.bold = true;
        let c = comparable_style(s);
        assert_eq!(c.bg_color, Color::None, "bg is cleared");
        assert_eq!(c.fg_color, Color::Palette(3), "fg is unchanged");
        assert_eq!(c.underline_color, s.underline_color, "underline unchanged");
        assert_eq!(c.flags, s.flags, "flags unchanged");
    }

    #[test]
    fn comparable_style_bg_only_equal() {
        // Two styles differing only in background compare equal after.
        let mut a = TermStyle::default();
        a.bg_color = Color::Palette(1);
        let mut b = TermStyle::default();
        b.bg_color = Color::Palette(2);
        assert_eq!(comparable_style(a), comparable_style(b));
        // A foreground difference still breaks the run.
        let mut d = TermStyle::default();
        d.fg_color = Color::Palette(9);
        assert_ne!(comparable_style(TermStyle::default()), comparable_style(d));
    }

    #[test]
    fn text_run_fields() {
        let run = TextRun {
            hash: 42,
            offset: 3,
            cells: 5,
            font_index: Index::new(Style::Regular, 0),
        };
        assert_eq!(run.hash, 42);
        assert_eq!(run.offset, 3);
        assert_eq!(run.cells, 5);
        let copy = run; // `Copy`
        assert_eq!(run, copy); // `PartialEq`
    }

    fn cp(codepoint: u32, cluster: u32) -> Codepoint {
        Codepoint { codepoint, cluster }
    }

    #[test]
    fn run_hash_deterministic() {
        let cps = [cp('A' as u32, 0), cp('B' as u32, 1)];
        let idx = Index::new(Style::Regular, 0);
        assert_eq!(run_hash(&cps, 2, idx), run_hash(&cps, 2, idx));
    }

    #[test]
    fn run_hash_distinguishes() {
        let base_cps = [cp('A' as u32, 0), cp('B' as u32, 1)];
        let idx = Index::new(Style::Regular, 0);
        let base = run_hash(&base_cps, 2, idx);
        // A different codepoint.
        assert_ne!(
            base,
            run_hash(&[cp('A' as u32, 0), cp('C' as u32, 1)], 2, idx)
        );
        // A different cluster.
        assert_ne!(
            base,
            run_hash(&[cp('A' as u32, 0), cp('B' as u32, 2)], 2, idx)
        );
        // A different cell count.
        assert_ne!(base, run_hash(&base_cps, 3, idx));
        // A different font index.
        assert_ne!(base, run_hash(&base_cps, 2, Index::new(Style::Bold, 0)));
    }

    #[test]
    fn run_hash_position_independent() {
        // The hash is over run-relative clusters: identical relative content (the
        // caller subtracts the run start) hashes the same; a run with different
        // (absolute-looking) clusters hashes differently.
        let idx = Index::new(Style::Regular, 0);
        let relative = [cp('x' as u32, 0), cp('y' as u32, 1), cp('z' as u32, 2)];
        let same_relative = [cp('x' as u32, 0), cp('y' as u32, 1), cp('z' as u32, 2)];
        assert_eq!(
            run_hash(&relative, 3, idx),
            run_hash(&same_relative, 3, idx)
        );
        let absolute = [cp('x' as u32, 5), cp('y' as u32, 6), cp('z' as u32, 7)];
        assert_ne!(run_hash(&relative, 3, idx), run_hash(&absolute, 3, idx));
    }
}
