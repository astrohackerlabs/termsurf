//! A collection of font faces, grouped by style.
//!
//! Faithful port of upstream `font/Collection.zig`: the packed [`Index`] handle,
//! the per-style [`Collection`] of eagerly-loaded [`Entry`] faces with
//! `add`/`get_entry`/`get_face`, and codepoint resolution
//! (`get_index`/`has_codepoint`). Deferred-face loading + discovery, per-entry
//! scale factors, and style aliasing land in later experiments.

use crate::font::face::coretext::Face;
use crate::font::metrics::FaceMetrics;
use crate::font::{Presentation, Style};

/// Bits used for the face index within an [`Index`]. `Style` is a 3-bit field,
/// leaving 13 bits of a `u16` for the index (up to 8192 fonts per style).
const IDX_BITS: u32 = 13;
/// Bits used for the style within an [`Index`].
const STYLE_BITS: u32 = 3;
/// Mask for the index portion (the low `IDX_BITS` of the unshifted value).
const IDX_MASK: u16 = (1 << IDX_BITS) - 1;

/// The special-case "fonts" that don't map to a real font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Special {
    /// Sprite drawing, rendered just-in-time via 2D graphics APIs.
    Sprite,
}

impl Special {
    /// Special indices start here; all `idx` values `>= START` are special.
    const START: u16 = IDX_MASK;

    /// The `idx` value encoding this special font.
    const fn idx(self) -> u16 {
        match self {
            // `sprite = start` upstream.
            Special::Sprite => Special::START,
        }
    }
}

/// Names a specific font within a [`Collection`](self).
///
/// Faithful port of upstream's `packed struct(u16) { style: Style, idx: u13 }`:
/// the `style` occupies the low 3 bits and the `idx` the high 13 bits of the
/// `u16` backing. The fields are private so the 13-bit `idx` invariant (which
/// upstream gets for free from its `u13` field) is enforced at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Index {
    style: Style,
    idx: u16,
}

impl Index {
    /// Create an index for `idx`-th face of `style`. Panics if `idx` exceeds the
    /// 13-bit range (`> 8191`) — upstream's `u13` makes that unrepresentable, so
    /// this is the runtime analog (a hard `assert!`, live in release too).
    pub(crate) fn new(style: Style, idx: u16) -> Index {
        assert!(idx <= IDX_MASK, "font index {idx} exceeds the 13-bit range");
        Index { style, idx }
    }

    /// Create a special (non-face) index, e.g. for sprite glyphs.
    pub(crate) fn special(v: Special) -> Index {
        // Upstream: `{ .style = .regular, .idx = @intFromEnum(v) }`.
        Index {
            style: Style::Regular,
            idx: v.idx(),
        }
    }

    /// The style component.
    pub(crate) fn style(&self) -> Style {
        self.style
    }

    /// The face index component (`0..=8191`).
    pub(crate) fn idx(&self) -> u16 {
        self.idx
    }

    /// The `u16` backing value (`style` in the low 3 bits, `idx` in the high 13).
    pub(crate) fn int(&self) -> u16 {
        // No masking: `idx` is a valid 13-bit value by construction.
        (self.style as u16) | (self.idx << STYLE_BITS)
    }

    /// Decode an [`Index`] from its `u16` backing. Any `u16` yields a valid
    /// 13-bit `idx` (`v >> 3 <= 8191`).
    pub(crate) fn from_int(v: u16) -> Index {
        let style = match v & ((1 << STYLE_BITS) - 1) {
            0 => Style::Regular,
            1 => Style::Bold,
            2 => Style::Italic,
            // Only 0..=3 are valid styles; 4..=7 are unused by upstream and
            // can't occur for a round-tripped `Index`.
            _ => Style::BoldItalic,
        };
        Index {
            style,
            idx: v >> STYLE_BITS,
        }
    }

    /// The special kind if this is a special index, else `None`. Faithful to
    /// upstream's `if (idx < start) null else @enumFromInt(idx)`.
    pub(crate) fn special_kind(&self) -> Option<Special> {
        if self.idx < Special::START {
            None
        } else {
            // Only one special value exists; `idx == START` is `Sprite`.
            Some(Special::Sprite)
        }
    }
}

impl Default for Index {
    /// Upstream's field defaults: `{ .style = .regular, .idx = 0 }`.
    fn default() -> Index {
        Index::new(Style::Regular, 0)
    }
}

/// A single face in a [`Collection`]. Faithful (eager) port of upstream `Entry`:
/// it owns a loaded [`Face`] and a fallback flag. (The deferred-face arm and the
/// per-entry scale factor are deferred to later experiments.)
pub(crate) struct Entry {
    face: Face,
    fallback: bool,
}

impl Entry {
    /// The loaded face.
    pub(crate) fn face(&self) -> &Face {
        &self.face
    }

    /// Whether this is a fallback face (searched after the primary faces).
    pub(crate) fn fallback(&self) -> bool {
        self.fallback
    }

    /// Whether this face has the given codepoint in the requested presentation.
    /// Faithful port of upstream `Entry.hasCodepoint`.
    pub(crate) fn has_codepoint(&self, cp: u32, p_mode: PresentationMode) -> bool {
        match p_mode {
            // Fallback fonts require explicit presentation matching; non-fallback
            // fonts accept any presentation.
            PresentationMode::Default(p) => {
                let resolved = if self.fallback {
                    PresentationMode::Explicit(p)
                } else {
                    PresentationMode::Any
                };
                self.has_codepoint(cp, resolved)
            }
            PresentationMode::Explicit(p) => match self.face.glyph_index(cp) {
                None => false,
                Some(idx) => match p {
                    Presentation::Text => !self.face.is_color_glyph(idx),
                    Presentation::Emoji => self.face.is_color_glyph(idx),
                },
            },
            PresentationMode::Any => self.face.glyph_index(cp).is_some(),
        }
    }
}

/// How to match a codepoint's presentation when resolving it to a face.
/// Faithful port of upstream `PresentationMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentationMode {
    /// The codepoint has an explicit, required presentation (e.g. VS15/VS16).
    Explicit(Presentation),
    /// The codepoint has no explicit presentation; use the default (from the
    /// Unicode character database).
    Default(Presentation),
    /// Any presentation is acceptable.
    Any,
}

/// An error adding a face to a [`Collection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddError {
    /// There's no more room in the collection for this style.
    CollectionFull,
    /// An alias's target index doesn't name a direct (non-alias) entry.
    InvalidAliasTarget,
}

/// An error resolving an [`Index`] to an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryError {
    /// The index is a special (built-in) font with no associated face.
    SpecialHasNoFace,
    /// The index is out of bounds for its style's face list.
    IndexOutOfBounds,
}

/// An error completing the styles of a [`Collection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompleteError {
    /// There is no regular face with text glyphs to fall back to.
    DefaultUnavailable,
}

/// Which styles may be **synthesized** (vs. aliased) when missing. Stand-in for
/// upstream's `config.FontSyntheticStyle` (the config subsystem is a separate
/// future area).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntheticStyle {
    pub italic: bool,
    pub bold: bool,
    pub bold_italic: bool,
}

/// How to scale a (fallback) face to match the primary face — the
/// `font-size-adjust` behavior. Faithful port of upstream `SizeAdjustment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizeAdjustment {
    /// Don't adjust; use the face's original size.
    None,
    /// Match the ideograph width with the primary face.
    IcWidth,
    /// Match the ex height with the primary face.
    ExHeight,
    /// Match the cap height with the primary face.
    CapHeight,
    /// Match the line height with the primary face.
    LineHeight,
}

/// The factor by which to scale a face (with metrics `face`) so the chosen
/// `adjustment` metric matches the `primary` face. Faithful port of upstream
/// `scaleFactor`: the metrics are normalized from pixels to ems (so the actual
/// sizes don't matter), and the chosen metric falls through
/// `ic_width → ex_height → cap_height → line_height` whenever the **face** does
/// not validly define it.
pub(crate) fn scale_factor(
    primary: &FaceMetrics,
    face: &FaceMetrics,
    adjustment: SizeAdjustment,
) -> f64 {
    if adjustment == SizeAdjustment::None {
        return 1.0;
    }

    // Normalize px to ems so the faces' actual sizes don't matter.
    let primary_scale = 1.0 / primary.px_per_em;
    let face_scale = 1.0 / face.px_per_em;

    // Fall through any metric the face doesn't validly define (its effective
    // accessor would return an estimate, not the stored value). `line_height` is
    // the always-valid terminus.
    let mut adj = adjustment;
    loop {
        match adj {
            SizeAdjustment::IcWidth => {
                if face.ic_width.is_some_and(|v| v > 0.0) {
                    break;
                }
                adj = SizeAdjustment::ExHeight;
            }
            SizeAdjustment::ExHeight => {
                if face.ex_height.is_some_and(|v| v > 0.0) {
                    break;
                }
                adj = SizeAdjustment::CapHeight;
            }
            SizeAdjustment::CapHeight => {
                if face.cap_height.is_some_and(|v| v > 0.0) {
                    break;
                }
                adj = SizeAdjustment::LineHeight;
            }
            SizeAdjustment::LineHeight => break,
            SizeAdjustment::None => unreachable!(),
        }
    }

    let (primary_metric, face_metric) = match adj {
        SizeAdjustment::IcWidth => (
            primary.effective_ic_width() * primary_scale,
            face.effective_ic_width() * face_scale,
        ),
        SizeAdjustment::ExHeight => (
            primary.effective_ex_height() * primary_scale,
            face.effective_ex_height() * face_scale,
        ),
        SizeAdjustment::CapHeight => (
            primary.effective_cap_height() * primary_scale,
            face.effective_cap_height() * face_scale,
        ),
        SizeAdjustment::LineHeight => (
            primary.line_height() * primary_scale,
            face.line_height() * face_scale,
        ),
        SizeAdjustment::None => unreachable!(),
    };

    primary_metric / face_metric
}

/// A slot in a style's face list: either an owned [`Entry`] or an `Alias` to a
/// face elsewhere in the collection. Faithful port of upstream `EntryOrAlias`,
/// with the alias stored as an [`Index`] (upstream's `*Entry` pointer is not
/// expressible in safe Rust; the behavior — resolving to the same target entry —
/// is identical). Aliases always point to a direct `Entry`, never another alias.
pub(crate) enum EntryOrAlias {
    Entry(Entry),
    Alias(Index),
}

/// A collection of font faces grouped by [`Style`].
///
/// Faithful port of upstream `Collection`, scoped to **eagerly loaded** faces:
/// the per-style face lists (entries or aliases) with `add`/`add_alias`/
/// `get_entry`/`get_face` and codepoint resolution. Deferred-face loading +
/// discovery, per-entry scale factors, and the `completeStyles` style-completion
/// logic land in later experiments.
pub(crate) struct Collection {
    /// The per-style face lists, indexed by `Style as usize` (`0..=3`). Each
    /// slot is an owned entry or an alias to a face elsewhere in the collection.
    faces: [Vec<EntryOrAlias>; 4],
}

/// True if a style's face list (of length `len`) can't accept another face
/// without producing a special index. Upstream guards `idx >= Special.start - 1`.
fn list_is_full(len: usize) -> bool {
    len >= (Special::START - 1) as usize
}

impl Collection {
    /// Create an empty collection.
    pub(crate) fn new() -> Collection {
        Collection {
            faces: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        }
    }

    /// Add an eagerly-loaded `face` of `style`, returning its [`Index`]. The face
    /// is added last in priority for its style. `fallback` marks a fallback face.
    pub(crate) fn add(
        &mut self,
        face: Face,
        style: Style,
        fallback: bool,
    ) -> Result<Index, AddError> {
        let list = &mut self.faces[style as usize];
        let idx = list.len();
        if list_is_full(idx) {
            return Err(AddError::CollectionFull);
        }
        list.push(EntryOrAlias::Entry(Entry { face, fallback }));
        Ok(Index::new(style, idx as u16))
    }

    /// Add an `alias` of `style` pointing at `target`, returning its [`Index`].
    /// `target` must name a **direct** entry (not a special / out-of-bounds /
    /// alias index), preserving the invariant that aliases never chain.
    pub(crate) fn add_alias(&mut self, style: Style, target: Index) -> Result<Index, AddError> {
        // Validate the target is a direct entry by inspecting it directly (not
        // via `get_entry`, which would follow an alias).
        if target.special_kind().is_some() {
            return Err(AddError::InvalidAliasTarget);
        }
        let tlist = &self.faces[target.style() as usize];
        match tlist.get(target.idx() as usize) {
            Some(EntryOrAlias::Entry(_)) => {}
            _ => return Err(AddError::InvalidAliasTarget),
        }

        let list = &mut self.faces[style as usize];
        let idx = list.len();
        if list_is_full(idx) {
            return Err(AddError::CollectionFull);
        }
        list.push(EntryOrAlias::Alias(target));
        Ok(Index::new(style, idx as u16))
    }

    /// Resolve a list slot to its underlying entry, following an alias (one step
    /// — aliases never point to aliases).
    fn entry_of<'a>(&'a self, eoa: &'a EntryOrAlias) -> &'a Entry {
        match eoa {
            EntryOrAlias::Entry(e) => e,
            EntryOrAlias::Alias(target) => {
                match &self.faces[target.style() as usize][target.idx() as usize] {
                    EntryOrAlias::Entry(e) => e,
                    EntryOrAlias::Alias(_) => {
                        unreachable!("alias points to another alias")
                    }
                }
            }
        }
    }

    /// Get the entry for an index, or the faithful error for a special index or
    /// an out-of-bounds index. Follows an alias to its target entry.
    pub(crate) fn get_entry(&self, index: Index) -> Result<&Entry, EntryError> {
        if index.special_kind().is_some() {
            return Err(EntryError::SpecialHasNoFace);
        }
        let list = &self.faces[index.style() as usize];
        let i = index.idx() as usize;
        if i >= list.len() {
            return Err(EntryError::IndexOutOfBounds);
        }
        Ok(self.entry_of(&list[i]))
    }

    /// Get the loaded face for an index. (Deferred-face loading is deferred.)
    pub(crate) fn get_face(&self, index: Index) -> Result<&Face, EntryError> {
        Ok(self.get_entry(index)?.face())
    }

    /// Return the index of the first face (in priority order) of `style` that
    /// has `cp` in the requested presentation, or `None`. Does not load faces.
    pub(crate) fn get_index(
        &self,
        cp: u32,
        style: Style,
        p_mode: PresentationMode,
    ) -> Option<Index> {
        let list = &self.faces[style as usize];
        for (i, eoa) in list.iter().enumerate() {
            if self.entry_of(eoa).has_codepoint(cp, p_mode) {
                return Some(Index::new(style, i as u16));
            }
        }
        None
    }

    /// Whether the face at `index` has `cp` in the requested presentation. An
    /// out-of-bounds (incl. special) index is `false`.
    pub(crate) fn has_codepoint(&self, index: Index, cp: u32, p_mode: PresentationMode) -> bool {
        let list = &self.faces[index.style() as usize];
        let i = index.idx() as usize;
        if i >= list.len() {
            return false;
        }
        self.entry_of(&list[i]).has_codepoint(cp, p_mode)
    }

    /// Ensure every style has at least one face. For each missing style, either
    /// synthesize a face (when enabled in `syn`) or alias it to the first regular
    /// face that has text glyphs. Faithful port of upstream `completeStyles`.
    ///
    /// No-ops if every style is already populated. Returns `DefaultUnavailable`
    /// if there are regular faces but none has text glyphs; returns `Ok` (doing
    /// nothing) if there is no regular face at all.
    pub(crate) fn complete_styles(&mut self, syn: SyntheticStyle) -> Result<(), CompleteError> {
        // The common case: every style already has at least one entry.
        if self.faces.iter().all(|list| !list.is_empty()) {
            return Ok(());
        }

        // Find the first regular face that has non-color text glyphs. This is
        // the face we fall back to; it may not be index 0 (e.g. if an emoji font
        // is configured first). Capture its canonical direct-entry index.
        let regular_list = &self.faces[Style::Regular as usize];
        if regular_list.is_empty() {
            // No regular face to fall back to; nothing we can do.
            return Ok(());
        }
        let mut regular: Option<Index> = None;
        for i in 0..regular_list.len() {
            // Canonicalize an alias slot to its direct-entry target so the later
            // `add_alias` accepts it (mirrors upstream resolving to the entry).
            let canonical = match &regular_list[i] {
                EntryOrAlias::Entry(_) => Index::new(Style::Regular, i as u16),
                EntryOrAlias::Alias(target) => *target,
            };
            let face = self
                .get_face(canonical)
                .expect("a regular slot resolves to a face");
            // Auto-italicize a normal text font; for mixed color/non-color fonts
            // accept the regular face if it at least has basic ASCII.
            if !face.has_color() || face.glyph_index('A' as u32).is_some() {
                regular = Some(canonical);
                break;
            }
        }
        let Some(regular) = regular else {
            // No regular text face found; we can't provide any fallback.
            return Err(CompleteError::DefaultUnavailable);
        };

        // Capture whether bold/italic were *originally* present, before we
        // complete them — the bold-italic preference below depends on this.
        let have_bold = !self.faces[Style::Bold as usize].is_empty();
        let have_italic = !self.faces[Style::Italic as usize].is_empty();

        // The `expect`s below are invariant-backed: `regular` (and `Bold,0`/
        // `Italic,0` once completed) is a validated direct entry, and each
        // destination style list is empty, so neither `get_face` nor `add` can
        // fail. The Rust synthetic methods are infallible, so upstream's
        // synthesis-failure alias fallbacks don't occur.

        // Italic: synthesize from the regular face, or alias to it.
        if !have_italic {
            if syn.italic {
                let face = self
                    .get_face(regular)
                    .expect("regular resolves")
                    .synthetic_italic();
                self.add(face, Style::Italic, false)
                    .expect("italic list is empty");
            } else {
                self.add_alias(Style::Italic, regular)
                    .expect("regular is a valid direct entry");
            }
        }

        // Bold: synthesize from the regular face, or alias to it.
        if !have_bold {
            if syn.bold {
                let face = self
                    .get_face(regular)
                    .expect("regular resolves")
                    .synthetic_bold();
                self.add(face, Style::Bold, false)
                    .expect("bold list is empty");
            } else {
                self.add_alias(Style::Bold, regular)
                    .expect("regular is a valid direct entry");
            }
        }

        // Bold-italic: prefer to synthesize on top of whatever we already had —
        // italicize the bold face if bold was original, else embolden the italic
        // face. If disabled, alias to the regular face.
        if self.faces[Style::BoldItalic as usize].is_empty() {
            if !syn.bold_italic {
                self.add_alias(Style::BoldItalic, regular)
                    .expect("regular is a valid direct entry");
            } else if have_bold {
                let face = self
                    .get_face(Index::new(Style::Bold, 0))
                    .expect("bold resolves")
                    .synthetic_italic();
                self.add(face, Style::BoldItalic, false)
                    .expect("bold-italic list is empty");
            } else {
                let face = self
                    .get_face(Index::new(Style::Italic, 0))
                    .expect("italic resolves")
                    .synthetic_bold();
                self.add(face, Style::BoldItalic, false)
                    .expect("bold-italic list is empty");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::metrics::FaceMetrics;

    #[test]
    fn index_bit_layout() {
        // style=Bold(1) in the low 3 bits, idx=5 in the high 13 bits: 1 | (5<<3).
        let i = Index::new(Style::Bold, 5);
        assert_eq!(i.int(), 1 | (5 << 3));
        assert_eq!(i.int(), 41);
        assert_eq!(Index::from_int(41), i);
    }

    #[test]
    fn index_round_trips() {
        for style in [
            Style::Regular,
            Style::Bold,
            Style::Italic,
            Style::BoldItalic,
        ] {
            for idx in [0u16, 1, 42, 8190] {
                let i = Index::new(style, idx);
                assert_eq!(Index::from_int(i.int()), i);
            }
        }
    }

    #[test]
    fn index_default_is_zero() {
        assert_eq!(Index::default().int(), 0);
        assert_eq!(Index::default().style(), Style::Regular);
        assert_eq!(Index::default().idx(), 0);
    }

    #[test]
    fn idx_bits_is_13() {
        assert_eq!(IDX_BITS, 13);
        // The maximum non-special index round-trips.
        let i = Index::new(Style::Italic, 8190);
        assert_eq!(Index::from_int(i.int()), i);
    }

    #[test]
    fn special_index() {
        let sprite = Index::special(Special::Sprite);
        assert_eq!(sprite.idx(), 8191);
        assert_eq!(sprite.special_kind(), Some(Special::Sprite));

        // Normal indices are not special.
        for idx in [0u16, 1, 8190] {
            assert_eq!(Index::new(Style::Regular, idx).special_kind(), None);
        }
    }

    #[test]
    fn from_int_idx_is_valid() {
        // Any u16 decodes to a valid 13-bit idx.
        assert_eq!(Index::from_int(u16::MAX).idx(), 8191);
    }

    #[test]
    #[should_panic]
    fn new_rejects_out_of_range_idx() {
        let _ = Index::new(Style::Regular, 8192);
    }

    #[test]
    fn add_and_get_face() {
        let mut c = Collection::new();
        let menlo = c
            .add(Face::new("Menlo", 32.0), Style::Regular, false)
            .expect("add Menlo");
        let emoji = c
            .add(Face::new("Apple Color Emoji", 32.0), Style::Regular, true)
            .expect("add emoji");

        assert_eq!(menlo, Index::new(Style::Regular, 0));
        assert_eq!(emoji, Index::new(Style::Regular, 1));

        // The faces round-trip and are distinguishable by their color state.
        assert!(!c.get_face(menlo).expect("menlo face").has_color());
        assert!(c.get_face(emoji).expect("emoji face").has_color());

        // The fallback flags are preserved.
        assert!(!c.get_entry(menlo).unwrap().fallback());
        assert!(c.get_entry(emoji).unwrap().fallback());
    }

    #[test]
    fn add_to_distinct_styles() {
        let mut c = Collection::new();
        let _ = c
            .add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        let bold = c
            .add(Face::new("Menlo", 32.0), Style::Bold, false)
            .expect("add bold");
        // The Bold list is independent of Regular, so the bold face is index 0.
        assert_eq!(bold, Index::new(Style::Bold, 0));
    }

    #[test]
    fn get_entry_special_has_no_face() {
        let c = Collection::new();
        assert_eq!(
            c.get_entry(Index::special(Special::Sprite)).err(),
            Some(EntryError::SpecialHasNoFace)
        );
    }

    #[test]
    fn get_entry_out_of_bounds() {
        let c = Collection::new();
        assert_eq!(
            c.get_entry(Index::new(Style::Regular, 0)).err(),
            Some(EntryError::IndexOutOfBounds)
        );
    }

    #[test]
    fn collection_full_boundary() {
        // Count 8189 can still add (produces idx 8189); 8190 is full.
        assert!(!list_is_full(8189));
        assert!(list_is_full(8190));
    }

    const EMOJI: u32 = 0x1F600; // 😀

    const NO_SYNTHESIS: SyntheticStyle = SyntheticStyle {
        italic: false,
        bold: false,
        bold_italic: false,
    };
    const ALL_SYNTHESIS: SyntheticStyle = SyntheticStyle {
        italic: true,
        bold: true,
        bold_italic: true,
    };

    fn menlo_collection() -> Collection {
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        c
    }

    #[test]
    fn get_index_text() {
        let c = menlo_collection();
        let m = 'M' as u32;
        assert_eq!(
            c.get_index(m, Style::Regular, PresentationMode::Any),
            Some(Index::new(Style::Regular, 0))
        );
        assert_eq!(
            c.get_index(
                m,
                Style::Regular,
                PresentationMode::Explicit(Presentation::Text)
            ),
            Some(Index::new(Style::Regular, 0))
        );
        // 'M' is not an emoji glyph, so an explicit-emoji request finds nothing.
        assert_eq!(
            c.get_index(
                m,
                Style::Regular,
                PresentationMode::Explicit(Presentation::Emoji)
            ),
            None
        );
    }

    #[test]
    fn get_index_emoji() {
        let mut c = menlo_collection();
        c.add(Face::new("Apple Color Emoji", 32.0), Style::Regular, false)
            .unwrap();
        // Menlo (idx 0) lacks the emoji; the color face (idx 1) has it as color.
        let at_one = Some(Index::new(Style::Regular, 1));
        assert_eq!(
            c.get_index(EMOJI, Style::Regular, PresentationMode::Any),
            at_one
        );
        assert_eq!(
            c.get_index(
                EMOJI,
                Style::Regular,
                PresentationMode::Explicit(Presentation::Emoji)
            ),
            at_one
        );
        // It's a color glyph, so an explicit-text request finds nothing.
        assert_eq!(
            c.get_index(
                EMOJI,
                Style::Regular,
                PresentationMode::Explicit(Presentation::Text)
            ),
            None
        );
    }

    #[test]
    fn default_presentation_fallback() {
        // Non-fallback: Default => Any, so the emoji glyph matches regardless of
        // the requested presentation.
        let mut c = Collection::new();
        c.add(Face::new("Apple Color Emoji", 32.0), Style::Regular, false)
            .unwrap();
        assert!(c
            .get_entry(Index::new(Style::Regular, 0))
            .unwrap()
            .has_codepoint(EMOJI, PresentationMode::Default(Presentation::Text)));

        // Fallback: Default => Explicit(Text), and the emoji glyph is color, so
        // it does not match a text request.
        let mut c = Collection::new();
        c.add(Face::new("Apple Color Emoji", 32.0), Style::Regular, true)
            .unwrap();
        assert!(!c
            .get_entry(Index::new(Style::Regular, 0))
            .unwrap()
            .has_codepoint(EMOJI, PresentationMode::Default(Presentation::Text)));
    }

    #[test]
    fn has_codepoint_bounds() {
        let c = menlo_collection();
        // Out-of-bounds index resolves to false (no panic).
        assert!(!c.has_codepoint(
            Index::new(Style::Regular, 5),
            'M' as u32,
            PresentationMode::Any
        ));
        // The in-bounds face does have 'M'.
        assert!(c.has_codepoint(
            Index::new(Style::Regular, 0),
            'M' as u32,
            PresentationMode::Any
        ));
    }

    #[test]
    fn alias_resolves_to_target() {
        let mut c = menlo_collection(); // Menlo at {Regular, 0}
        let italic = c
            .add_alias(Style::Italic, Index::new(Style::Regular, 0))
            .expect("alias italic -> regular");
        assert_eq!(italic, Index::new(Style::Italic, 0));

        // The alias resolves to the Menlo face and entry.
        assert!(!c.get_face(italic).expect("aliased face").has_color());
        assert!(!c.get_entry(italic).unwrap().fallback());
        assert!(c.has_codepoint(italic, 'M' as u32, PresentationMode::Any));
    }

    #[test]
    fn get_index_follows_alias() {
        let mut c = menlo_collection();
        c.add_alias(Style::Italic, Index::new(Style::Regular, 0))
            .unwrap();
        // The italic alias position resolves the codepoint through the target.
        assert_eq!(
            c.get_index('M' as u32, Style::Italic, PresentationMode::Any),
            Some(Index::new(Style::Italic, 0))
        );
        // Bold has no entry or alias.
        assert_eq!(
            c.get_index('M' as u32, Style::Bold, PresentationMode::Any),
            None
        );
    }

    #[test]
    fn complete_styles_aliases_missing() {
        let mut c = menlo_collection(); // Menlo at {Regular, 0}
        c.complete_styles(NO_SYNTHESIS).expect("complete");

        // Each missing style now aliases the regular Menlo face at index 0.
        for style in [Style::Italic, Style::Bold, Style::BoldItalic] {
            let idx = Index::new(style, 0);
            assert!(!c.get_face(idx).expect("aliased face").has_color());
            assert!(c.has_codepoint(idx, 'M' as u32, PresentationMode::Any));
        }
    }

    #[test]
    fn complete_styles_noop_when_full() {
        let mut c = Collection::new();
        for style in [
            Style::Regular,
            Style::Bold,
            Style::Italic,
            Style::BoldItalic,
        ] {
            c.add(Face::new("Menlo", 32.0), style, false).unwrap();
        }
        c.complete_styles(NO_SYNTHESIS).expect("complete");
        // No style gained a second entry: index 1 is out of bounds everywhere.
        for style in [
            Style::Regular,
            Style::Bold,
            Style::Italic,
            Style::BoldItalic,
        ] {
            assert_eq!(
                c.get_entry(Index::new(style, 1)).err(),
                Some(EntryError::IndexOutOfBounds)
            );
        }
    }

    #[test]
    fn complete_styles_empty_is_ok() {
        let mut c = Collection::new();
        c.complete_styles(NO_SYNTHESIS).expect("complete on empty");
        // Still empty: no regular face to alias to.
        assert_eq!(
            c.get_entry(Index::new(Style::Italic, 0)).err(),
            Some(EntryError::IndexOutOfBounds)
        );
    }

    #[test]
    fn complete_styles_default_unavailable() {
        let emoji = Face::new("Apple Color Emoji", 32.0);
        // Precondition: this color font lacks a text 'A' glyph. If that ever
        // changes, the heuristic would accept it and this test's premise is moot.
        if emoji.glyph_index('A' as u32).is_some() {
            return;
        }
        let mut c = Collection::new();
        c.add(emoji, Style::Regular, false).unwrap();
        assert_eq!(
            c.complete_styles(NO_SYNTHESIS),
            Err(CompleteError::DefaultUnavailable)
        );
    }

    #[test]
    fn complete_styles_synthesizes() {
        let mut c = menlo_collection(); // Menlo at {Regular, 0}; no bold/italic.
        c.complete_styles(ALL_SYNTHESIS).expect("complete");

        // Bold is synthesized from the regular face (has a bold line width).
        assert!(c
            .get_face(Index::new(Style::Bold, 0))
            .unwrap()
            .synthetic_bold_width()
            .is_some());
        // Italic is synthesized (sheared).
        assert!(c
            .get_face(Index::new(Style::Italic, 0))
            .unwrap()
            .is_skewed());
        // Bold-italic: bold wasn't originally present, so it's bold-on-italic —
        // its base is the synthetic italic, so it's both bold-width and skewed.
        let bi = c.get_face(Index::new(Style::BoldItalic, 0)).unwrap();
        assert!(bi.synthetic_bold_width().is_some());
        assert!(bi.is_skewed());
    }

    #[test]
    fn complete_styles_bold_italic_prefers_bold() {
        // Regular and Bold present (so have_bold is true), italic/bold-italic not.
        let mut c = menlo_collection();
        c.add(Face::new("Menlo", 32.0), Style::Bold, false).unwrap();
        c.complete_styles(ALL_SYNTHESIS).expect("complete");

        // Bold-italic is synthesized as italic-on-bold: the base is the real
        // (non-synthetic) bold Menlo, so it's skewed but has no bold line width.
        let bi = c.get_face(Index::new(Style::BoldItalic, 0)).unwrap();
        assert!(bi.is_skewed());
        assert!(bi.synthetic_bold_width().is_none());
    }

    #[test]
    fn complete_styles_alias_when_disabled() {
        let mut c = menlo_collection();
        c.complete_styles(NO_SYNTHESIS).expect("complete");
        // With synthesis off, the missing styles alias the plain regular face:
        // no bold width, no skew.
        assert!(c
            .get_face(Index::new(Style::Bold, 0))
            .unwrap()
            .synthetic_bold_width()
            .is_none());
        assert!(!c
            .get_face(Index::new(Style::Italic, 0))
            .unwrap()
            .is_skewed());
    }

    #[test]
    fn add_alias_rejects_bad_target() {
        // Target doesn't exist (empty collection).
        let mut c = Collection::new();
        assert_eq!(
            c.add_alias(Style::Italic, Index::new(Style::Regular, 0)),
            Err(AddError::InvalidAliasTarget)
        );

        // Target is itself an alias -> rejected (aliases must point to a direct
        // entry, so they never chain).
        let mut c = menlo_collection();
        let italic = c
            .add_alias(Style::Italic, Index::new(Style::Regular, 0))
            .unwrap();
        assert_eq!(
            c.add_alias(Style::Bold, italic),
            Err(AddError::InvalidAliasTarget)
        );

        // A special target is rejected too.
        assert_eq!(
            c.add_alias(Style::Bold, Index::special(Special::Sprite)),
            Err(AddError::InvalidAliasTarget)
        );
    }

    /// A `FaceMetrics` fixture; non-relevant fields are zeroed.
    fn fm(
        px_per_em: f64,
        ascent: f64,
        descent: f64,
        line_gap: f64,
        cell_width: f64,
        cap_height: Option<f64>,
        ex_height: Option<f64>,
        ic_width: Option<f64>,
    ) -> FaceMetrics {
        FaceMetrics {
            px_per_em,
            cell_width,
            ascent,
            descent,
            line_gap,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height,
            ex_height,
            ascii_height: None,
            ic_width,
        }
    }

    #[test]
    fn scale_factor_none_is_one() {
        let p = fm(16.0, 12.0, -4.0, 0.0, 8.0, Some(9.0), Some(7.0), Some(15.0));
        let f = fm(
            32.0,
            24.0,
            -8.0,
            2.0,
            16.0,
            Some(18.0),
            Some(14.0),
            Some(30.0),
        );
        assert_eq!(scale_factor(&p, &f, SizeAdjustment::None), 1.0);
    }

    #[test]
    fn scale_factor_same_metrics_is_one() {
        let m = fm(16.0, 12.0, -4.0, 1.0, 8.0, Some(9.0), Some(7.0), Some(15.0));
        for adj in [
            SizeAdjustment::IcWidth,
            SizeAdjustment::ExHeight,
            SizeAdjustment::CapHeight,
            SizeAdjustment::LineHeight,
        ] {
            assert!((scale_factor(&m, &m, adj) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn scale_factor_line_height() {
        // primary line height = 12 - (-4) + 0 = 16, at 16 px/em -> 1.0 em.
        let p = fm(16.0, 12.0, -4.0, 0.0, 8.0, None, None, None);
        // face line height = 30 - (-9) + 0 = 39, at 30 px/em -> 1.3 em.
        let f = fm(30.0, 30.0, -9.0, 0.0, 16.0, None, None, None);
        let expected = (16.0 / 16.0) / (39.0 / 30.0);
        assert!((scale_factor(&p, &f, SizeAdjustment::LineHeight) - expected).abs() < 1e-12);
    }

    #[test]
    fn scale_factor_falls_through() {
        let p = fm(16.0, 12.0, -4.0, 0.0, 8.0, Some(9.0), Some(7.0), Some(15.0));
        // The face validly defines ex_height but not ic_width, so IcWidth falls
        // through to ExHeight.
        let f = fm(16.0, 12.0, -4.0, 0.0, 8.0, Some(9.0), Some(8.0), None);
        let via_ic = scale_factor(&p, &f, SizeAdjustment::IcWidth);
        let via_ex = scale_factor(&p, &f, SizeAdjustment::ExHeight);
        assert!((via_ic - via_ex).abs() < 1e-12);
        // And it is NOT what forcing the ic_width estimate would give.
        let ic_forced = (p.effective_ic_width() / 16.0) / (f.effective_ic_width() / 16.0);
        assert!((via_ic - ic_forced).abs() > 1e-9);

        // A face with none of ic/ex/cap falls all the way to line_height.
        let f2 = fm(16.0, 12.0, -4.0, 0.0, 8.0, None, None, None);
        let via_ic2 = scale_factor(&p, &f2, SizeAdjustment::IcWidth);
        let via_lh2 = scale_factor(&p, &f2, SizeAdjustment::LineHeight);
        assert!((via_ic2 - via_lh2).abs() < 1e-12);
    }
}
