//! The run iterator — grouping a terminal row's cells into shaping runs.
//!
//! Faithful (in progress) port of upstream `font/shaper/run.zig`. Provides the
//! shaping input ([`RunOptions`]/[`RunCell`]) and output ([`TextRun`]/[`run_hash`]),
//! the per-cell decision helpers ([`font_style`]/[`is_bad_ligature_break`]/
//! [`presentation_for_grapheme`]/[`comparable_style`]), and [`RunIterator`]'s
//! cell-walking grouping loop (with the trailing-empty trim, the invisible/spacer
//! skips, and the selection/cursor/style/ligature breaks). The renderer code that
//! builds [`RunCell`]s from terminal cells is a later sub-area.

use std::hash::{Hash, Hasher};

use crate::font::codepoint_resolver::CodepointResolver;
use crate::font::collection::Index;
use crate::font::shape::Codepoint;
use crate::font::{Presentation, Style};
use crate::terminal::kitty::graphics_unicode::PLACEHOLDER;
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

/// The wide kind of a cell: a normal narrow/wide cell, or a spacer that pads a
/// wide cell or wraps a line. Mirrors `terminal::page::Wide` (which is
/// `pub(super)`); the renderer maps the terminal value to this. The run iterator
/// skips the spacer kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Wide {
    /// A normal single-cell-wide cell.
    #[default]
    Narrow,
    /// The first cell of a two-cell-wide character.
    Wide,
    /// The padding cell after a wide character.
    SpacerTail,
    /// A padding cell at the end of a row before a wide character that wrapped.
    SpacerHead,
}

/// The decoded per-cell data the run iterator reads from a terminal row — what
/// upstream reads off the `cells`/`graphemes`/`styles` slices. The renderer
/// extracts this from a terminal `Cell` (whose accessors are `pub(super)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunCell {
    /// The cell's primary Unicode scalar (`0` for an empty cell).
    pub codepoint: u32,
    /// The grapheme's additional codepoints, in order (empty if not a grapheme).
    pub graphemes: Vec<u32>,
    /// The cell's effective style (the default style if the cell is unstyled).
    pub style: TermStyle,
    /// The cell's style id (the fast-path equality the run iterator uses before
    /// the `comparable_style` comparison).
    pub style_id: u16,
    /// The cell's wide kind (spacers are skipped by the run iterator).
    pub wide: Wide,
    /// Whether the cell is empty (rendered as a space).
    pub is_empty: bool,
    /// Whether the cell's content is a plain codepoint (vs a background-color
    /// cell) — the guard for the bad-ligature break.
    pub is_codepoint: bool,
}

impl RunCell {
    /// Whether the cell carries a grapheme (additional combined codepoints).
    pub(crate) fn has_grapheme(&self) -> bool {
        !self.graphemes.is_empty()
    }
}

/// The input to a run iterator: a terminal row's decoded cells plus the run
/// breaks. Faithful port of `shape.RunOptions` — `grid` is omitted (roastty
/// passes the `CodepointResolver` to the iterator separately) and the cells are
/// pre-decoded [`RunCell`]s rather than a terminal `MultiArrayList` slice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunOptions {
    /// The row's cells, left to right, including empty cells (the run iterator's
    /// trimming, offset, and cluster math depend on positional indexes).
    pub cells: Vec<RunCell>,
    /// The `[start, end]` selection column bounds in this row, if any (a run
    /// breaks at a selection boundary).
    pub selection: Option<[u16; 2]>,
    /// The cursor's column in this row, if any (a run breaks around the cursor).
    pub cursor_x: Option<u16>,
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

/// One run's shaped input: the [`TextRun`] descriptor plus the accumulated
/// `(codepoint, cluster)` stream to hand to `Face::shape_run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunOutput {
    pub run: TextRun,
    pub codepoints: Vec<Codepoint>,
}

/// Groups a terminal row's cells into shaping runs. Faithful port of upstream
/// `RunIterator` (its common path): each [`RunIterator::next`] yields the next run
/// of cells that share a font and a comparable style. The spacer skip and the
/// selection/cursor breaks are deferred (Exp 357); this slice handles narrow cells
/// with no selection and no cursor.
pub(crate) struct RunIterator<'a> {
    opts: &'a RunOptions,
    resolver: &'a mut CodepointResolver,
    /// The current position in the row.
    i: usize,
    /// The exclusive upper bound after trimming trailing empty cells.
    max: usize,
}

impl<'a> RunIterator<'a> {
    /// Create an iterator over `opts`'s row, resolving fonts through `resolver`.
    pub(crate) fn new(opts: &'a RunOptions, resolver: &'a mut CodepointResolver) -> Self {
        let max = trailing_trim(&opts.cells);
        Self {
            opts,
            resolver,
            i: 0,
            max,
        }
    }

    /// The next run, or `None` when the row is exhausted.
    pub(crate) fn next(&mut self) -> Option<RunOutput> {
        let cells = &self.opts.cells;
        // Skip leading invisible cells.
        while self.i < self.max && cells[self.i].style.flags.invisible {
            self.i += 1;
        }
        if self.i >= self.max {
            return None;
        }
        let start = self.i;
        let style = cells[start].style;
        let mut codepoints: Vec<Codepoint> = Vec::new();
        // The run's font, the default index until set at the first cell (matching
        // upstream's `Collection.Index = .{}` — so a run that begins on a skipped
        // spacer keeps a following default-font cell rather than breaking).
        let mut current_font = Index::new(Style::Regular, 0);

        let mut j = start;
        while j < self.max {
            let cell = &cells[j];
            // A run-relative cluster (a column count fits `u16`, so `u32` is safe).
            let cluster = u32::try_from(j - start).expect("a run cluster fits u32");

            // Selection break: split at the selection's start column and just past
            // its end. Compare the loop index (`usize`) to the widened bounds.
            if let Some(bounds) = self.opts.selection {
                if j > start {
                    if bounds[0] > 0 && j == usize::from(bounds[0]) {
                        break;
                    }
                    if bounds[1] > 0 && j == usize::from(bounds[1]) + 1 {
                        break;
                    }
                }
            }

            // Spacer skip: a wide cell's padding carries no glyph (but still
            // advances the index, preserving the cluster gap).
            if matches!(cell.wide, Wide::SpacerHead | Wide::SpacerTail) {
                j += 1;
                continue;
            }

            if j > start {
                let prev = &cells[j - 1];
                // Bad-ligature break (both cells must be plain codepoints).
                if prev.is_codepoint
                    && cell.is_codepoint
                    && is_bad_ligature_break(prev.codepoint, cell.codepoint)
                {
                    break;
                }
                // Style break: a different `style_id` whose comparable style
                // (ignoring background) differs ends the run.
                if prev.style_id != cell.style_id
                    && comparable_style(style) != comparable_style(cell.style)
                {
                    break;
                }
            }

            let fstyle = font_style(style.flags.bold, style.flags.italic);
            let presentation = if cell.has_grapheme() {
                presentation_for_grapheme(cell.graphemes[0])
            } else {
                None
            };

            // Cursor break (non-grapheme cells only): isolate the cursor cell so a
            // row with a cursor has up to three runs (before / exactly / after).
            if !cell.has_grapheme() {
                if let Some(cursor_x) = self.opts.cursor_x {
                    let cursor = usize::from(cursor_x);
                    if start == cursor && j == start + 1 {
                        break;
                    }
                    if start < cursor && j == cursor {
                        break;
                    }
                }
            }

            let (idx, fallback) = self.resolve_font(cell, fstyle, presentation);
            if j == start {
                current_font = idx;
            }
            if idx != current_font {
                break; // font change → run ends (cell `j` starts the next run)
            }

            if let Some(cp) = fallback {
                // A fallback substitutes a single codepoint, not the grapheme.
                codepoints.push(Codepoint {
                    codepoint: cp,
                    cluster,
                });
                j += 1;
                continue;
            }
            // A kitty unicode placeholder shapes as a blank space (it is a
            // positioning marker for an image, not a glyph).
            if cell.codepoint == PLACEHOLDER {
                codepoints.push(Codepoint {
                    codepoint: ' ' as u32,
                    cluster,
                });
                j += 1;
                continue;
            }
            let primary = if cell.codepoint == 0 {
                ' ' as u32
            } else {
                cell.codepoint
            };
            codepoints.push(Codepoint {
                codepoint: primary,
                cluster,
            });
            for &cp in &cell.graphemes {
                // Presentation selectors are not sent to the shaper.
                if cp == 0xFE0E || cp == 0xFE0F {
                    continue;
                }
                codepoints.push(Codepoint {
                    codepoint: cp,
                    cluster,
                });
            }
            j += 1;
        }

        let font_index = current_font;
        let cell_count = u16::try_from(j - start).expect("a run's cell count fits u16");
        let offset = u16::try_from(start).expect("a run's column offset fits u16");
        self.i = j;
        Some(RunOutput {
            run: TextRun {
                hash: run_hash(&codepoints, cell_count, font_index),
                offset,
                cells: cell_count,
                font_index,
            },
            codepoints,
        })
    }

    /// Resolve a cell's font index, with upstream's fallback chain: the grapheme's
    /// own font, else `U+FFFD`, else space. Returns the index and, for a fallback,
    /// the substituted codepoint. A kitty placeholder resolves like an empty cell
    /// (a space), matching upstream `indexForCell`.
    fn resolve_font(
        &mut self,
        cell: &RunCell,
        fstyle: Style,
        p: Option<Presentation>,
    ) -> (Index, Option<u32>) {
        // The placeholder is an image-positioning marker; resolve it as a space.
        let primary_cp = if cell.codepoint == PLACEHOLDER {
            0
        } else {
            cell.codepoint
        };
        if let Some(idx) = self
            .resolver
            .index_for_grapheme(primary_cp, &cell.graphemes, fstyle, p)
        {
            return (idx, None);
        }
        if let Some(idx) = self.resolver.get_index(0xFFFD, fstyle, p) {
            return (idx, Some(0xFFFD));
        }
        let idx = self
            .resolver
            .get_index(' ' as u32, fstyle, p)
            .expect("a font renders space");
        (idx, Some(' ' as u32))
    }
}

/// The exclusive upper bound after trimming trailing empty cells (last non-empty
/// cell index + 1, or `0` if the row is entirely empty).
fn trailing_trim(cells: &[RunCell]) -> usize {
    for k in 0..cells.len() {
        let rev = cells.len() - 1 - k;
        if !cells[rev].is_empty {
            return rev + 1;
        }
    }
    0
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

    fn sample_cell(codepoint: u32, graphemes: Vec<u32>) -> RunCell {
        RunCell {
            codepoint,
            graphemes,
            style: TermStyle::default(),
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: codepoint == 0,
            is_codepoint: true,
        }
    }

    #[test]
    fn run_cell_has_grapheme() {
        assert!(!sample_cell('A' as u32, vec![]).has_grapheme());
        assert!(sample_cell('n' as u32, vec![0x0308]).has_grapheme());
    }

    #[test]
    fn run_options_default() {
        let o = RunOptions::default();
        assert!(o.cells.is_empty());
        assert_eq!(o.selection, None);
        assert_eq!(o.cursor_x, None);
    }

    #[test]
    fn run_cell_construction() {
        let cell = RunCell {
            codepoint: 'Z' as u32,
            graphemes: vec![0xFE0F],
            style: TermStyle::default(),
            style_id: 7,
            wide: Wide::Wide,
            is_empty: false,
            is_codepoint: true,
        };
        assert_eq!(cell.codepoint, 'Z' as u32);
        assert_eq!(cell.graphemes, vec![0xFE0F]);
        assert_eq!(cell.style_id, 7);
        assert_eq!(cell.wide, Wide::Wide);
        assert!(cell.has_grapheme());

        let opts = RunOptions {
            cells: vec![cell.clone(), sample_cell('A' as u32, vec![])],
            selection: Some([1, 4]),
            cursor_x: Some(2),
        };
        assert_eq!(opts.cells.len(), 2);
        assert_eq!(opts.cells[0], cell);
        assert_eq!(opts.selection, Some([1, 4]));
        assert_eq!(opts.cursor_x, Some(2));
    }

    fn menlo_resolver() -> CodepointResolver {
        use crate::font::collection::Collection;
        use crate::font::face::coretext::Face;
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        CodepointResolver::new(c)
    }

    fn narrow(codepoint: u32) -> RunCell {
        RunCell {
            codepoint,
            graphemes: vec![],
            style: TermStyle::default(),
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: codepoint == 0,
            is_codepoint: true,
        }
    }

    fn cps(out: &RunOutput) -> Vec<(u32, u32)> {
        out.codepoints
            .iter()
            .map(|c| (c.codepoint, c.cluster))
            .collect()
    }

    fn with_wide(codepoint: u32, wide: Wide) -> RunCell {
        RunCell {
            wide,
            ..narrow(codepoint)
        }
    }

    #[test]
    fn next_groups_one_run() {
        let opts = RunOptions {
            cells: vec![narrow('A' as u32), narrow('B' as u32)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(out.run.offset, 0);
        assert_eq!(out.run.cells, 2);
        assert_eq!(cps(&out), vec![('A' as u32, 0), ('B' as u32, 1)]);
        assert!(it.next().is_none(), "the row has one run");
    }

    #[test]
    fn next_trims_trailing_empties() {
        let opts = RunOptions {
            cells: vec![narrow('A' as u32), narrow('B' as u32), narrow(0), narrow(0)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(out.run.cells, 2, "trailing empty cells are trimmed");
        assert_eq!(cps(&out), vec![('A' as u32, 0), ('B' as u32, 1)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_breaks_on_bad_ligature() {
        let opts = RunOptions {
            cells: vec![narrow('f' as u32), narrow('l' as u32)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let first = it.next().expect("the 'f' run");
        assert_eq!(first.run.offset, 0);
        assert_eq!(first.run.cells, 1);
        assert_eq!(cps(&first), vec![('f' as u32, 0)]);
        let second = it.next().expect("the 'l' run");
        assert_eq!(second.run.offset, 1);
        assert_eq!(second.run.cells, 1);
        assert_eq!(cps(&second), vec![('l' as u32, 0)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_empty_cell_is_space() {
        // A leading empty (non-invisible) cell contributes a space codepoint.
        let opts = RunOptions {
            cells: vec![narrow(0), narrow('A' as u32)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(
            cps(&out),
            vec![(' ' as u32, 0), ('A' as u32, 1)],
            "the empty cell shapes as a space"
        );
    }

    #[test]
    fn next_all_empty_is_none() {
        let mut r = menlo_resolver();
        // An all-empty row.
        let opts = RunOptions {
            cells: vec![narrow(0), narrow(0)],
            ..Default::default()
        };
        assert!(RunIterator::new(&opts, &mut r).next().is_none());
        // An empty row.
        let empty = RunOptions::default();
        assert!(RunIterator::new(&empty, &mut r).next().is_none());
    }

    #[test]
    fn next_skips_spacer() {
        // A wide char, its spacer-tail padding, then a narrow cell: one run; the
        // spacer at index 1 emits nothing but its cluster gap remains (A → 2).
        let opts = RunOptions {
            cells: vec![
                with_wide('W' as u32, Wide::Wide),
                with_wide(0, Wide::SpacerTail),
                narrow('A' as u32),
            ],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(out.run.cells, 3);
        assert_eq!(cps(&out), vec![('W' as u32, 0), ('A' as u32, 2)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_breaks_on_selection() {
        // "ABCD" with selection [1, 2] breaks at j==1 and at j==3 (= bounds[1]+1).
        let opts = RunOptions {
            cells: vec![
                narrow('A' as u32),
                narrow('B' as u32),
                narrow('C' as u32),
                narrow('D' as u32),
            ],
            selection: Some([1, 2]),
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        assert_eq!(cps(&it.next().expect("run 1")), vec![('A' as u32, 0)]);
        assert_eq!(
            cps(&it.next().expect("run 2")),
            vec![('B' as u32, 0), ('C' as u32, 1)]
        );
        assert_eq!(cps(&it.next().expect("run 3")), vec![('D' as u32, 0)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_breaks_on_cursor_exact() {
        // Cursor at column 0: the cursor cell is its own run.
        let opts = RunOptions {
            cells: vec![narrow('A' as u32), narrow('B' as u32)],
            cursor_x: Some(0),
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        assert_eq!(cps(&it.next().expect("run 1")), vec![('A' as u32, 0)]);
        assert_eq!(cps(&it.next().expect("run 2")), vec![('B' as u32, 0)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_breaks_on_cursor_before() {
        // Cursor at column 1: the run breaks when reaching the cursor.
        let opts = RunOptions {
            cells: vec![narrow('A' as u32), narrow('B' as u32)],
            cursor_x: Some(1),
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        assert_eq!(cps(&it.next().expect("run 1")), vec![('A' as u32, 0)]);
        assert_eq!(cps(&it.next().expect("run 2")), vec![('B' as u32, 0)]);
        assert!(it.next().is_none());
    }

    #[test]
    fn next_leading_spacer_default_font() {
        // A leading spacer then 'A' (which resolves to the default Menlo regular
        // face): one run — the spacer is skipped but does not break the run.
        let opts = RunOptions {
            cells: vec![with_wide(0, Wide::SpacerTail), narrow('A' as u32)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(out.run.cells, 2);
        assert_eq!(
            cps(&out),
            vec![('A' as u32, 1)],
            "the spacer does not break"
        );
        assert!(it.next().is_none());
    }

    #[test]
    fn next_placeholder_is_space() {
        // A kitty unicode placeholder shapes as a blank space (resolved via the
        // space font, not U+FFFD), so it groups with the surrounding text.
        let opts = RunOptions {
            cells: vec![narrow('A' as u32), narrow(PLACEHOLDER), narrow('B' as u32)],
            ..Default::default()
        };
        let mut r = menlo_resolver();
        let mut it = RunIterator::new(&opts, &mut r);
        let out = it.next().expect("a run");
        assert_eq!(out.run.cells, 3, "the placeholder joins the run");
        assert_eq!(
            cps(&out),
            vec![('A' as u32, 0), (' ' as u32, 1), ('B' as u32, 2)],
            "the placeholder shapes as a space"
        );
        assert!(it.next().is_none());
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
