//! Font discovery descriptors.
//!
//! Faithful port of the font-search data types from upstream `font/discovery.zig`
//! (and the `Variation` from `font/face.zig`). A [`Descriptor`] describes a font
//! to search for; a [`Variation`] is a font-variation axis setting.
//! [`Descriptor::to_core_text_descriptor`] turns one into a CoreText
//! `CTFontDescriptor` (the query object), [`Descriptor::discover_descriptors`]
//! runs the collection match and ranks the candidates best-first by [`Score`],
//! and [`Descriptor::discover_faces`] lazily turns those into [`Face`]s. The
//! variation-axis score refinement, `discoverFallback`, and the resolver wiring
//! are later sub-areas.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{
    kCFNull, CFArray, CFCharacterSet, CFDictionary, CFMutableDictionary, CFNumber, CFRange,
    CFRetained, CFString, CFType,
};
use objc2_core_text::{
    kCTFontCharacterSetAttribute, kCTFontFamilyNameAttribute, kCTFontSizeAttribute,
    kCTFontStyleNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute, CTFont,
    CTFontCollection, CTFontDescriptor, CTFontSymbolicTraits, CTFontTableOptions,
};

use crate::font::face::coretext::Face;
use crate::font::opentype::{head::Head, os2::Os2};

/// A font-variation axis setting (e.g. weight `wght`, slant `slnt`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Variation {
    /// The axis identifier — a four-character code packed big-endian into a
    /// `u32` (e.g. `wght` is `2003265652`).
    pub id: u32,
    /// The axis value.
    pub value: f64,
}

impl Variation {
    /// Pack a four-character axis tag into its `u32` identifier. Faithful to
    /// upstream's `Variation.Id` (a `wght` tag yields `2003265652`).
    pub(crate) fn id_from_tag(tag: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*tag)
    }
}

/// Describes a font to search for. Faithful port of upstream
/// `discovery.Descriptor` (owned `String`s replace the caller-owned Zig strings).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Descriptor {
    /// The font family to search for (e.g. `"Fira Code"`, `"monospace"`).
    pub family: Option<String>,
    /// A specific font style string to filter by.
    pub style: Option<String>,
    /// A codepoint the font must be able to render (`0` = none).
    pub codepoint: u32,
    /// The font size in points the font should support (`0.0` = unspecified).
    pub size: f32,
    /// Search for a bold font.
    pub bold: bool,
    /// Search for an italic font.
    pub italic: bool,
    /// Search for a monospace font.
    pub monospace: bool,
    /// Variation axes to apply (preferred when searching).
    pub variations: Vec<Variation>,
}

impl Descriptor {
    /// A `u64` hash of every field — used to key a discovery descriptor cache.
    /// The `f32`/`f64` fields are hashed by their bit pattern (so it is a total,
    /// reflexive function). The exact value is internal (not serialized), so a
    /// consistent Rust hash is a faithful analog of upstream's `hashcode`.
    pub(crate) fn hashcode(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.family.hash(&mut h);
        self.style.hash(&mut h);
        self.codepoint.hash(&mut h);
        self.size.to_bits().hash(&mut h);
        self.bold.hash(&mut h);
        self.italic.hash(&mut h);
        self.monospace.hash(&mut h);
        for v in &self.variations {
            v.id.hash(&mut h);
            v.value.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

impl Descriptor {
    /// Convert this descriptor to a CoreText `CTFontDescriptor` — the query
    /// object CoreText's font-matching APIs consume. Faithful port of upstream
    /// `Descriptor.toCoreTextDescriptor`: only the present fields are set, the
    /// size is rounded to an `i32`, and the bold/italic/monospace symbolic traits
    /// go in a nested dictionary keyed by `kCTFontSymbolicTrait`.
    pub(crate) fn to_core_text_descriptor(&self) -> CFRetained<CTFontDescriptor> {
        let attrs = CFMutableDictionary::<CFString, CFType>::empty();

        // Set `value` under the CF string `key` in `attrs`. The dictionary uses
        // CF-type callbacks, so it retains both for its lifetime.
        let set = |key: &CFString, value: *const c_void| {
            // SAFETY: `key`/`value` are live CF objects (retained by the
            // dictionary on insertion); `attrs` is a mutable CF dictionary.
            unsafe {
                CFMutableDictionary::set_value(
                    Some(attrs.as_opaque()),
                    (key as *const CFString).cast::<c_void>(),
                    value,
                );
            }
        };

        // Family.
        if let Some(family) = &self.family {
            let s = CFString::from_str(family);
            // SAFETY: `kCTFontFamilyNameAttribute` is a static CF string key.
            set(unsafe { kCTFontFamilyNameAttribute }, ct_ptr(&*s));
        }

        // Style.
        if let Some(style) = &self.style {
            let s = CFString::from_str(style);
            // SAFETY: `kCTFontStyleNameAttribute` is a static CF string key.
            set(unsafe { kCTFontStyleNameAttribute }, ct_ptr(&*s));
        }

        // Codepoint support: a character set holding the single codepoint.
        if self.codepoint > 0 {
            // SAFETY: a single-codepoint range; a null allocator is valid.
            if let Some(cs) = unsafe {
                CFCharacterSet::with_characters_in_range(
                    None,
                    CFRange {
                        location: self.codepoint as isize,
                        length: 1,
                    },
                )
            } {
                // SAFETY: `kCTFontCharacterSetAttribute` is a static CF string key.
                set(unsafe { kCTFontCharacterSetAttribute }, ct_ptr(&*cs));
            }
        }

        // Size (rounded to an `SInt32`).
        if self.size > 0.0 {
            let n = CFNumber::new_i32(self.size.round() as i32);
            // SAFETY: `kCTFontSizeAttribute` is a static CF string key.
            set(unsafe { kCTFontSizeAttribute }, ct_ptr(&*n));
        }

        // Symbolic traits (bold/italic/monospace), in a nested dictionary.
        let mut traits = CTFontSymbolicTraits(0);
        if self.bold {
            traits |= CTFontSymbolicTraits::TraitBold;
        }
        if self.italic {
            traits |= CTFontSymbolicTraits::TraitItalic;
        }
        if self.monospace {
            traits |= CTFontSymbolicTraits::TraitMonoSpace;
        }
        if traits.0 != 0 {
            let traits_dict = CFMutableDictionary::<CFString, CFType>::empty();
            let n = CFNumber::new_i32(traits.0 as i32);
            // SAFETY: `kCTFontSymbolicTrait` is a static CF string key; the
            // nested dict retains the number.
            unsafe {
                CFMutableDictionary::set_value(
                    Some(traits_dict.as_opaque()),
                    (kCTFontSymbolicTrait as *const CFString).cast::<c_void>(),
                    ct_ptr(&*n),
                );
            }
            // SAFETY: `kCTFontTraitsAttribute` is a static CF string key.
            set(
                unsafe { kCTFontTraitsAttribute },
                ct_ptr(traits_dict.as_opaque()),
            );
        }

        // SAFETY: `attrs` is a valid attributes dictionary.
        unsafe { CTFontDescriptor::with_attributes(attrs.as_opaque()) }
    }
}

/// A `*const c_void` to a CF object, for the raw `set_value` calls.
fn ct_ptr<T>(obj: &T) -> *const c_void {
    (obj as *const T).cast::<c_void>()
}

impl Descriptor {
    /// Discover the candidate CoreText font descriptors matching this descriptor,
    /// ranked **best-first**. Faithful port of upstream `CoreText.discover`: wrap
    /// the query descriptor in a one-element `CFArray`, build a
    /// `CTFontCollection`, ask it for the matching descriptors, copy them into an
    /// owned (retained) `Vec`, and sort by the request's [`Score`]
    /// (`sortMatchingDescriptors`). An empty result means no matches.
    pub(crate) fn discover_descriptors(&self) -> Vec<CFRetained<CTFontDescriptor>> {
        let ct_desc = self.to_core_text_descriptor();
        let query = CFArray::from_retained_objects(&[ct_desc]);

        // SAFETY: `query` is a live `CFArray` of font descriptors; the collection
        // only reads it.
        let collection =
            unsafe { CTFontCollection::with_font_descriptors(Some(query.as_opaque()), None) };

        // SAFETY: the collection only reads to produce its matching descriptors.
        let Some(matches) = (unsafe { collection.matching_font_descriptors() }) else {
            return Vec::new();
        };

        // The matching array's elements are `CTFontDescriptor`s.
        // SAFETY: `CTFontCollectionCreateMatchingFontDescriptors` yields an array
        // of `CTFontDescriptor`.
        let matches: CFRetained<CFArray<CTFontDescriptor>> =
            unsafe { CFRetained::cast_unchecked(matches) };

        // `CFArray::get` retains each element (upstream retains explicitly).
        let mut out = Vec::with_capacity(matches.len());
        for i in 0..matches.len() {
            if let Some(d) = matches.get(i) {
                out.push(d);
            }
        }

        // Sort best-first by the request's score (`sortMatchingDescriptors`).
        // Each candidate is scored once (upstream recomputes per comparison, but
        // the order is identical since the comparator only depends on `int()`).
        let mut scored: Vec<(u32, CFRetained<CTFontDescriptor>)> =
            out.into_iter().map(|d| (self.score(&d).int(), d)).collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, d)| d).collect()
    }

    /// Discover the matching faces, ranked best-first, as a **lazy** iterator.
    /// Faithful port of upstream `DiscoverIterator.next`: each sorted candidate
    /// descriptor has its character-set attribute removed (it was a search
    /// filter, not a render constraint) and is turned into a [`Face`]. The face
    /// is produced only when the iterator advances. (Applying the requested
    /// variations to the face is deferred.)
    pub(crate) fn discover_faces(&self) -> impl Iterator<Item = Face> {
        self.discover_descriptors().into_iter().map(deferred_face)
    }

    /// Discover fallback faces for this descriptor, ranked best-first. Faithful
    /// port of upstream `discoverFallback`: for a CJK unified ideograph
    /// (`U+4E00..=U+9FFF`) go straight to CoreText's locale-aware codepoint
    /// search on `original` (falling through to the general path if it finds
    /// nothing); otherwise use the general discovery, falling back to the
    /// codepoint search only when the general match is empty. `original` is the
    /// font the codepoint search starts from (the caller selects it by style).
    pub(crate) fn discover_fallback_faces(&self, original: &Face) -> Vec<Face> {
        // CJK unified ideographs: CoreText handles the locale better than a
        // manual match. Only return early when the search succeeds (upstream's
        // `orelse break :han`); on `None`, fall through to the general path.
        if (0x4E00..=0x9FFF).contains(&self.codepoint) {
            if let Some(face) = original.font_for_codepoint(self.codepoint) {
                return vec![face];
            }
        }

        // General discovery; if it found nothing and we have a codepoint, fall
        // back to the codepoint search.
        let descriptors = self.discover_descriptors();
        if descriptors.is_empty() && self.codepoint > 0 {
            return original
                .font_for_codepoint(self.codepoint)
                .into_iter()
                .collect();
        }
        descriptors.into_iter().map(deferred_face).collect()
    }
}

/// Turn a sorted candidate descriptor into a [`Face`], **removing** the
/// character-set attribute first (upstream's `DiscoverIterator.next`).
fn deferred_face(desc: CFRetained<CTFontDescriptor>) -> Face {
    // Drop the character-set filter by copying the descriptor with the attribute
    // set to `kCFNull` (CoreText removes a null-valued attribute from the copy).
    let attrs = CFMutableDictionary::<CFString, CFType>::empty();
    // SAFETY: `kCTFontCharacterSetAttribute` is a static key; `kCFNull` is a
    // live singleton; the mutable dict retains both.
    unsafe {
        let null = kCFNull.expect("kCFNull is available");
        CFMutableDictionary::set_value(
            Some(attrs.as_opaque()),
            (kCTFontCharacterSetAttribute as *const CFString).cast::<c_void>(),
            (null as *const objc2_core_foundation::CFNull).cast::<c_void>(),
        );
    }
    // SAFETY: `desc` is live; `attrs` is a valid attributes dictionary.
    let desc = unsafe { desc.copy_with_attributes(attrs.as_opaque()) };

    // Create the font (size 12; resized later when actually used).
    // SAFETY: `desc` is live; a null matrix is valid.
    let font = unsafe { CTFont::with_font_descriptor(&desc, 12.0, std::ptr::null()) };
    Face::from_ct_font(font)
}

/// The ranking score for a discovery candidate. Faithful port of upstream's
/// `CoreText.Score` packed struct: the fields are laid out by **increasing
/// precedence**, so the integer projection [`Score::int`] compares as a single
/// value where a higher number is a better match. (Computing a `Score` from a
/// font — `score()` — and wiring the sort into discovery are later experiments.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Score {
    /// Tie-breaker: more glyphs is preferred, all else equal (bits `0..16`).
    pub glyph_count: u16,
    /// Fuzzy style-string match strength (bits `16..24`).
    pub fuzzy_style: u8,
    /// The font's bold-ness matches the request (bit `24`).
    pub bold: bool,
    /// The font's italic-ness matches the request (bit `25`).
    pub italic: bool,
    /// An exact (case-insensitive) style-string match (bit `26`).
    pub exact_style: bool,
    /// The font is monospace (bit `27`).
    pub monospace: bool,
    /// The font has the requested codepoint (bit `28`, the highest precedence).
    pub codepoint: bool,
}

impl Score {
    /// Project the score to a single integer for comparison, reproducing
    /// upstream's packed-struct bit layout (fields least- to most-significant).
    /// Upstream's backing integer is `u29`; `u32` is wider with the top bits
    /// always zero, so the ordering is identical.
    pub(crate) fn int(&self) -> u32 {
        self.glyph_count as u32
            | (self.fuzzy_style as u32) << 16
            | (self.bold as u32) << 24
            | (self.italic as u32) << 25
            | (self.exact_style as u32) << 26
            | (self.monospace as u32) << 27
            | (self.codepoint as u32) << 28
    }
}

impl Ord for Score {
    /// A natural ordering by [`Score::int`] — a higher score is `Greater`. A
    /// best-first sort reverses this (`sort_by(|a, b| b.cmp(a))`), matching
    /// upstream's "`lhs.int() > rhs.int()` ⇒ lhs is earlier".
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.int().cmp(&other.int())
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Descriptor {
    /// Compute the ranking [`Score`] for a candidate font descriptor against this
    /// (request) descriptor. Faithful port of the font-loaded and symbolic-trait
    /// fields of upstream `Score.score`: load the candidate font, count its
    /// glyphs, check whether it has the requested codepoint, and compare its
    /// monospace/bold/italic-ness (from the symbolic traits) to the request. The
    /// `head`/`OS/2`/variation bold-italic refinement and the style exact/fuzzy
    /// match are later experiments, so `bold`/`italic` here use the symbolic
    /// traits only and `fuzzy_style`/`exact_style` stay zero.
    pub(crate) fn score(&self, ct_desc: &CTFontDescriptor) -> Score {
        let mut s = Score::default();

        // Load the candidate font (size 12; the size is altered later when the
        // face is actually used). The objc2 constructor is non-fallible, so the
        // upstream "unloadable font scores 0" path is not modeled.
        // SAFETY: `ct_desc` is a live descriptor; a null matrix is valid.
        let font = unsafe { CTFont::with_font_descriptor(ct_desc, 12.0, std::ptr::null()) };

        // Prefer fonts with more glyphs, all else equal (clamped to `u16::MAX`).
        // SAFETY: `font` is live.
        let glyph_count = unsafe { font.glyph_count() };
        s.glyph_count = glyph_count.clamp(0, u16::MAX as isize) as u16;

        // If we're seeking a codepoint, prefer fonts that have it.
        if self.codepoint > 0 {
            s.codepoint = font_has_codepoint(&font, self.codepoint);
        }

        // Symbolic traits drive monospace and the initial bold/italic guesses.
        let traits = symbolic_traits(ct_desc);
        s.monospace = traits.contains(CTFontSymbolicTraits::TraitMonoSpace);
        let mut is_bold = traits.contains(CTFontSymbolicTraits::TraitBold);
        let mut is_italic = traits.contains(CTFontSymbolicTraits::TraitItalic);

        // Refine the guesses from the font's own tables, which are generally more
        // reliable than the symbolic traits. The `head` `macStyle` bits and the
        // `OS/2` `fsSelection` bits can only turn a flag on (OR-ed in). (The
        // variation-axis derivation, which overwrites these for variable fonts,
        // is a later experiment.)
        if let Some(head) = copy_table(&font, b"head").and_then(|d| Head::from_bytes(&d).ok()) {
            is_bold |= head.mac_style & 1 == 1;
            is_italic |= head.mac_style & 2 == 2;
        }
        if let Some(os2) = copy_table(&font, b"OS/2").and_then(|d| Os2::from_bytes(&d).ok()) {
            is_bold |= os2.fs_selection.bold();
            is_italic |= os2.fs_selection.italic();
        }

        // The bold/italic fields are whether the font matches the request.
        s.bold = self.bold == is_bold;
        s.italic = self.italic == is_italic;

        // Style-string match: an exact (case-insensitive) match on the first
        // desired style, plus a fuzzy substring score.
        let desired = desired_styles(self.style.as_deref(), self.bold, self.italic);
        let (exact, fuzzy) = style_score(&style_name(ct_desc), &desired);
        s.exact_style = exact;
        s.fuzzy_style = fuzzy;

        s
    }
}

/// The candidate's style name (e.g. `"Regular"`, `"Bold Italic"`), or `""`.
fn style_name(ct_desc: &CTFontDescriptor) -> String {
    // SAFETY: a static CF string key; the descriptor is live.
    unsafe { ct_desc.attribute(kCTFontStyleNameAttribute) }
        .and_then(|v| v.downcast::<CFString>().ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// The desired style strings for a request, in precedence order (the first is
/// the exact-match target). Faithful port of upstream's `desired_styles`: an
/// explicit style wins; otherwise the bold/italic combination picks the list.
fn desired_styles(style: Option<&str>, bold: bool, italic: bool) -> Vec<&str> {
    if let Some(s) = style {
        return vec![s];
    }
    if bold {
        if italic {
            vec!["bold italic", "bold", "italic", "oblique"]
        } else {
            vec!["bold", "upright"]
        }
    } else if italic {
        vec!["italic", "regular", "oblique"]
    } else {
        vec!["regular", "upright"]
    }
}

/// Score a font's style name against the desired styles: `(exact, fuzzy)`.
/// `exact` is a case-insensitive equality with the first desired style; `fuzzy`
/// rewards style names mostly composed of desired substrings (`255 −` the
/// leftover length after subtracting each matched desired substring). Faithful
/// port of upstream's byte-wise-ASCII style scoring.
fn style_score(style_str: &str, desired: &[&str]) -> (bool, u8) {
    let exact = style_str.eq_ignore_ascii_case(desired[0]);

    let lower = style_str.to_ascii_lowercase();
    let mut fuzzy = style_str.len().min(u8::MAX as usize) as u8;
    for ds in desired {
        if lower.contains(&ds.to_ascii_lowercase()) {
            fuzzy = fuzzy.saturating_sub(ds.len().min(u8::MAX as usize) as u8);
        }
    }
    (exact, u8::MAX.saturating_sub(fuzzy))
}

/// Copy the raw bytes of the OpenType table `tag` from `font`, or `None`.
/// Mirrors `Face::copy_table`.
fn copy_table(font: &CTFont, tag: &[u8; 4]) -> Option<Vec<u8>> {
    let table_tag = u32::from_be_bytes(*tag);
    // SAFETY: `font` is live; the tag and (empty) options are valid.
    let data = unsafe { font.table(table_tag, CTFontTableOptions(0)) }?;
    Some(data.to_vec())
}

/// Whether `font` has a glyph for the Unicode scalar `cp` (handling the
/// surrogate pair for a supplementary codepoint). Mirrors `Face::glyph_index`.
fn font_has_codepoint(font: &CTFont, cp: u32) -> bool {
    let Some(c) = char::from_u32(cp) else {
        return false;
    };
    let mut units = [0u16; 2];
    let units = c.encode_utf16(&mut units);
    let mut glyphs = [0u16; 2];
    let chars_ptr = NonNull::new(units.as_ptr() as *mut u16).unwrap();
    let glyphs_ptr = NonNull::new(glyphs.as_mut_ptr()).unwrap();
    // SAFETY: `units`/`glyphs` are length-`len` buffers; CoreText reads the
    // UTF-16 units and writes one glyph per unit, returning `false` if any unit
    // has no glyph.
    unsafe { font.glyphs_for_characters(chars_ptr, glyphs_ptr, units.len() as isize) }
}

/// Read the symbolic traits from a font descriptor's `kCTFontTraitsAttribute` →
/// `kCTFontSymbolicTrait` value, or empty traits if absent.
fn symbolic_traits(ct_desc: &CTFontDescriptor) -> CTFontSymbolicTraits {
    // SAFETY: a static CF string key; the descriptor is live.
    let Some(attr) = (unsafe { ct_desc.attribute(kCTFontTraitsAttribute) }) else {
        return CTFontSymbolicTraits(0);
    };
    let Ok(dict) = attr.downcast::<CFDictionary>() else {
        return CTFontSymbolicTraits(0);
    };
    // SAFETY: a static CF string key; the value (if present) is a `CFNumber`.
    let v = unsafe { dict.value((kCTFontSymbolicTrait as *const CFString).cast::<c_void>()) };
    if v.is_null() {
        return CTFontSymbolicTraits(0);
    }
    // SAFETY: the value stored under `kCTFontSymbolicTrait` is a `CFNumber`.
    let n = unsafe { &*(v as *const CFNumber) };
    CTFontSymbolicTraits(n.as_i32().unwrap_or(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variation_id_from_tag() {
        // Upstream-verified packed identifiers.
        assert_eq!(Variation::id_from_tag(b"wght"), 2003265652);
        assert_eq!(Variation::id_from_tag(b"slnt"), 1936486004);
    }

    #[test]
    fn descriptor_default() {
        let d = Descriptor::default();
        assert_eq!(d.codepoint, 0);
        assert_eq!(d.size, 0.0);
        assert!(!d.bold && !d.italic && !d.monospace);
        assert!(d.variations.is_empty());
        assert!(d.family.is_none() && d.style.is_none());
    }

    #[test]
    fn hashcode_consistent() {
        let a = Descriptor {
            family: Some("Foo".into()),
            size: 12.0,
            bold: true,
            ..Default::default()
        };
        assert_eq!(a.hashcode(), a.clone().hashcode(), "equal descriptors");
        assert_ne!(
            a.hashcode(),
            Descriptor {
                family: Some("Bar".into()),
                ..a.clone()
            }
            .hashcode(),
            "family differs"
        );
        assert_ne!(
            a.hashcode(),
            Descriptor {
                size: 13.0,
                ..a.clone()
            }
            .hashcode(),
            "size differs"
        );
        assert_ne!(
            a.hashcode(),
            Descriptor {
                bold: false,
                ..a.clone()
            }
            .hashcode(),
            "bold differs"
        );
    }

    #[test]
    fn descriptor_family_round_trips() {
        let d = Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        };
        let desc = d.to_core_text_descriptor();
        // SAFETY: a static CF string key; `desc` is live.
        let v = unsafe { desc.attribute(kCTFontFamilyNameAttribute) }.expect("family is set");
        let s = v.downcast::<CFString>().expect("the family is a CFString");
        assert_eq!(s.to_string(), "Menlo");
    }

    #[test]
    fn descriptor_size_rounded() {
        let d = Descriptor {
            size: 12.6,
            ..Default::default()
        };
        let desc = d.to_core_text_descriptor();
        // SAFETY: a static CF string key; `desc` is live.
        let v = unsafe { desc.attribute(kCTFontSizeAttribute) }.expect("size is set");
        let n = v.downcast::<CFNumber>().expect("the size is a CFNumber");
        // 12.6 rounds to 13 and is stored as an SInt32.
        assert_eq!(n.as_i32(), Some(13));
    }

    #[test]
    fn descriptor_traits_symbolic_bits() {
        use objc2_core_foundation::CFDictionary;
        // CoreText resolves a descriptor's attributes (it may infer values we did
        // not set), so we assert the symbolic-trait *content* rather than the
        // mere presence/absence of the traits attribute.
        let d = Descriptor {
            bold: true,
            italic: true,
            ..Default::default()
        };
        let desc = d.to_core_text_descriptor();
        // SAFETY: a static CF string key; the descriptor is live.
        let attr = unsafe { desc.attribute(kCTFontTraitsAttribute) }.expect("traits set");
        let dict = attr
            .downcast::<CFDictionary>()
            .expect("the traits are a dict");
        // SAFETY: a static CF string key; the stored value is the CFNumber we set.
        let v = unsafe { dict.value((kCTFontSymbolicTrait as *const CFString).cast::<c_void>()) };
        assert!(!v.is_null(), "the symbolic trait is present");
        // SAFETY: the value is the `CFNumber` we stored under this key.
        let n = unsafe { &*(v as *const CFNumber) };
        let bits = n.as_i32().expect("an i32 symbolic-trait value") as u32;
        assert!(
            bits & CTFontSymbolicTraits::TraitBold.0 != 0,
            "bold bit set"
        );
        assert!(
            bits & CTFontSymbolicTraits::TraitItalic.0 != 0,
            "italic bit set"
        );
        assert!(
            bits & CTFontSymbolicTraits::TraitMonoSpace.0 == 0,
            "monospace bit not set"
        );
    }

    #[test]
    fn descriptor_codepoint_charset_contains() {
        // The character set the descriptor carries holds the requested codepoint
        // and is not a catch-all (a BMP codepoint keeps the membership check on
        // the `u16` `is_character_member`).
        let d = Descriptor {
            codepoint: 0x00C0, // À
            ..Default::default()
        };
        let desc = d.to_core_text_descriptor();
        // SAFETY: a static CF string key; the descriptor is live.
        let attr = unsafe { desc.attribute(kCTFontCharacterSetAttribute) }.expect("charset set");
        let cs = attr.downcast::<CFCharacterSet>().expect("a CFCharacterSet");
        assert!(
            cs.is_character_member(0x00C0),
            "holds the requested codepoint"
        );
        assert!(!cs.is_character_member(0x41), "is not a catch-all set");
    }

    #[test]
    fn descriptor_builds_empty() {
        // An all-default descriptor builds a valid (empty-attributes) descriptor
        // without panicking.
        let _ = Descriptor::default().to_core_text_descriptor();
    }

    /// Whether any candidate descriptor reports the given family name.
    fn any_family(list: &[CFRetained<CTFontDescriptor>], name: &str) -> bool {
        list.iter().any(|desc| {
            // SAFETY: a static CF string key; the descriptor is live.
            unsafe { desc.attribute(kCTFontFamilyNameAttribute) }
                .and_then(|v| v.downcast::<CFString>().ok())
                .is_some_and(|s| s.to_string() == name)
        })
    }

    #[test]
    fn discover_descriptors_finds_menlo() {
        let d = Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        };
        let list = d.discover_descriptors();
        assert!(!list.is_empty(), "Menlo matches at least one face");
        assert!(
            any_family(&list, "Menlo"),
            "a candidate has the Menlo family"
        );
    }

    #[test]
    fn discover_descriptors_monospace() {
        // A traits-only search (monospace) goes through the collection and yields
        // the system's monospace faces.
        let d = Descriptor {
            monospace: true,
            ..Default::default()
        };
        let list = d.discover_descriptors();
        assert!(!list.is_empty(), "monospace search yields faces");
    }

    #[test]
    fn discover_descriptors_unknown_family() {
        // CoreText may return nothing or a permissive fallback for an impossible
        // family; either way no candidate should actually claim it, and the call
        // must not panic.
        let d = Descriptor {
            family: Some("__no_such_font_family__".into()),
            ..Default::default()
        };
        let list = d.discover_descriptors();
        assert!(
            !any_family(&list, "__no_such_font_family__"),
            "no candidate claims the impossible family"
        );
    }

    #[test]
    fn score_field_offsets() {
        let off = |s: Score| s.int();
        assert_eq!(
            off(Score {
                glyph_count: 0xABCD,
                ..Default::default()
            }),
            0xABCD
        );
        assert_eq!(
            off(Score {
                fuzzy_style: 0xEF,
                ..Default::default()
            }),
            0x00EF_0000
        );
        assert_eq!(
            off(Score {
                bold: true,
                ..Default::default()
            }),
            1 << 24
        );
        assert_eq!(
            off(Score {
                italic: true,
                ..Default::default()
            }),
            1 << 25
        );
        assert_eq!(
            off(Score {
                exact_style: true,
                ..Default::default()
            }),
            1 << 26
        );
        assert_eq!(
            off(Score {
                monospace: true,
                ..Default::default()
            }),
            1 << 27
        );
        assert_eq!(
            off(Score {
                codepoint: true,
                ..Default::default()
            }),
            1 << 28
        );
    }

    #[test]
    fn score_precedence() {
        // Each higher-precedence field, alone, outranks every lower field
        // maxed out together.
        let all_lower_of = |field: u8| -> Score {
            // Set every field strictly below `field` (0 = glyph_count ..
            // 6 = codepoint) to its maximum.
            Score {
                glyph_count: if field > 0 { u16::MAX } else { 0 },
                fuzzy_style: if field > 1 { u8::MAX } else { 0 },
                bold: field > 2,
                italic: field > 3,
                exact_style: field > 4,
                monospace: field > 5,
                codepoint: false,
            }
        };
        let only = |field: u8| -> Score {
            let mut s = Score::default();
            match field {
                1 => s.fuzzy_style = 1,
                2 => s.bold = true,
                3 => s.italic = true,
                4 => s.exact_style = true,
                5 => s.monospace = true,
                6 => s.codepoint = true,
                _ => s.glyph_count = 1,
            }
            s
        };
        for field in 1..=6u8 {
            assert!(
                only(field).int() > all_lower_of(field).int(),
                "field {field} must outrank all lower fields combined"
            );
        }
    }

    #[test]
    fn score_glyph_count_tiebreak() {
        let more = Score {
            monospace: true,
            glyph_count: 5000,
            ..Default::default()
        };
        let fewer = Score {
            monospace: true,
            glyph_count: 100,
            ..Default::default()
        };
        assert!(more.int() > fewer.int(), "more glyphs ranks higher");
        assert!(more > fewer, "Ord agrees");
    }

    #[test]
    fn score_ord_sorts_desc() {
        let mut v = vec![
            Score {
                glyph_count: 10,
                ..Default::default()
            },
            Score {
                codepoint: true,
                ..Default::default()
            },
            Score {
                monospace: true,
                ..Default::default()
            },
            Score {
                bold: true,
                ..Default::default()
            },
        ];
        // Best-first: reverse the natural ordering.
        v.sort_by(|a, b| b.cmp(a));
        let ints: Vec<u32> = v.iter().map(Score::int).collect();
        let mut sorted = ints.clone();
        sorted.sort_unstable();
        sorted.reverse();
        assert_eq!(ints, sorted, "sorted best-first (descending int)");
        // The codepoint score is first, the bare glyph_count score is last.
        assert!(v[0].codepoint);
        assert_eq!(v[3].glyph_count, 10);
    }

    /// Resolve a Menlo candidate descriptor (a matched font from discovery, not
    /// the query descriptor) for scoring.
    fn menlo_candidate() -> CFRetained<CTFontDescriptor> {
        let d = Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        };
        d.discover_descriptors()
            .into_iter()
            .find(|desc| {
                // SAFETY: a static CF string key; the descriptor is live.
                unsafe { desc.attribute(kCTFontFamilyNameAttribute) }
                    .and_then(|v| v.downcast::<CFString>().ok())
                    .is_some_and(|s| s.to_string() == "Menlo")
            })
            .expect("a resolved Menlo candidate")
    }

    #[test]
    fn score_menlo_is_monospace() {
        let c = menlo_candidate();
        let s = Descriptor::default().score(&c);
        assert!(s.monospace, "Menlo is monospace");
        assert!(s.glyph_count > 0, "Menlo has glyphs");
    }

    #[test]
    fn score_codepoint_present_absent() {
        let c = menlo_candidate();
        assert!(
            Descriptor {
                codepoint: 'M' as u32,
                ..Default::default()
            }
            .score(&c)
            .codepoint,
            "Menlo has 'M'"
        );
        assert!(
            !Descriptor {
                codepoint: 0x1F600,
                ..Default::default()
            }
            .score(&c)
            .codepoint,
            "Menlo lacks the emoji"
        );
        assert!(
            !Descriptor::default().score(&c).codepoint,
            "no codepoint sought"
        );
    }

    #[test]
    fn score_bold_italic_match_flips() {
        let c = menlo_candidate();
        // self.bold == is_bold, so flipping the request flips the match field
        // (deterministic regardless of the candidate's actual boldness).
        let bold_when_false = Descriptor {
            bold: false,
            ..Default::default()
        }
        .score(&c)
        .bold;
        let bold_when_true = Descriptor {
            bold: true,
            ..Default::default()
        }
        .score(&c)
        .bold;
        assert_ne!(
            bold_when_false, bold_when_true,
            "flipping the bold request flips the match"
        );

        let italic_when_false = Descriptor {
            italic: false,
            ..Default::default()
        }
        .score(&c)
        .italic;
        let italic_when_true = Descriptor {
            italic: true,
            ..Default::default()
        }
        .score(&c)
        .italic;
        assert_ne!(
            italic_when_false, italic_when_true,
            "flipping the italic request flips the match"
        );
    }

    /// All resolved Menlo candidate descriptors from discovery.
    fn menlo_candidates() -> Vec<CFRetained<CTFontDescriptor>> {
        Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        }
        .discover_descriptors()
    }

    #[test]
    fn score_detects_bold_variant() {
        let cands = menlo_candidates();
        assert!(!cands.is_empty(), "Menlo has candidates");
        // Some Menlo variant is bold: a bold request matches it and a non-bold
        // request does not (which holds iff `is_bold` for that candidate is true).
        let any_bold = cands.iter().any(|c| {
            Descriptor {
                bold: true,
                ..Default::default()
            }
            .score(c)
            .bold
                && !Descriptor {
                    bold: false,
                    ..Default::default()
                }
                .score(c)
                .bold
        });
        assert!(any_bold, "a bold Menlo variant is detected as bold");
    }

    #[test]
    fn score_detects_italic_variant() {
        let cands = menlo_candidates();
        let any_italic = cands.iter().any(|c| {
            Descriptor {
                italic: true,
                ..Default::default()
            }
            .score(c)
            .italic
                && !Descriptor {
                    italic: false,
                    ..Default::default()
                }
                .score(c)
                .italic
        });
        assert!(any_italic, "an italic Menlo variant is detected as italic");
    }

    #[test]
    fn score_regular_not_bold_italic() {
        let cands = menlo_candidates();
        // The regular Menlo face is detected as neither bold nor italic: a
        // non-bold/non-italic request matches both flags (so `is_bold` and
        // `is_italic` are both false for it — the refinement does not spuriously
        // flip a regular face).
        let any_regular = cands.iter().any(|c| {
            let s = Descriptor {
                bold: false,
                italic: false,
                ..Default::default()
            }
            .score(c);
            s.bold && s.italic
        });
        assert!(
            any_regular,
            "the regular Menlo face is detected as neither bold nor italic"
        );
    }

    #[test]
    fn desired_styles_chain() {
        assert_eq!(desired_styles(Some("Foo"), false, false), vec!["Foo"]);
        // An explicit style wins over bold/italic.
        assert_eq!(desired_styles(Some("Foo"), true, true), vec!["Foo"]);
        assert_eq!(
            desired_styles(None, true, true),
            vec!["bold italic", "bold", "italic", "oblique"]
        );
        assert_eq!(desired_styles(None, true, false), vec!["bold", "upright"]);
        assert_eq!(
            desired_styles(None, false, true),
            vec!["italic", "regular", "oblique"]
        );
        assert_eq!(
            desired_styles(None, false, false),
            vec!["regular", "upright"]
        );
    }

    #[test]
    fn style_score_pure() {
        // The whole name is consumed by a desired substring → max score.
        assert_eq!(style_score("Regular", &["regular", "upright"]), (true, 255));
        assert_eq!(style_score("Bold", &["bold", "upright"]), (true, 255));
        // Nothing matches → 255 − len leftover.
        assert_eq!(style_score("Regular", &["bold", "upright"]), (false, 248));
        assert_eq!(style_score("Italic", &["regular", "upright"]), (false, 249));
        // Empty style: no exact match, but zero leftover → max fuzzy.
        assert_eq!(style_score("", &["regular", "upright"]), (false, 255));
    }

    #[test]
    fn score_style_exact_integration() {
        // The Regular Menlo candidate exact-matches a default request but not a
        // bold one, and the matching desire consumes more of the style name.
        let cands = menlo_candidates();
        let regular = cands
            .iter()
            .find(|c| style_name(c).eq_ignore_ascii_case("Regular"))
            .expect("a Regular Menlo candidate");
        let default_score = Descriptor::default().score(regular);
        assert!(
            default_score.exact_style,
            "Regular exact-matches the default desire"
        );
        let bold_score = Descriptor {
            bold: true,
            ..Default::default()
        }
        .score(regular);
        assert!(
            !bold_score.exact_style,
            "Regular does not exact-match a bold desire"
        );
        assert!(
            default_score.fuzzy_style > bold_score.fuzzy_style,
            "the default desire consumes more of the name"
        );
    }

    #[test]
    fn discover_sorted_descending() {
        // The candidates come back ranked best-first by the request's score.
        let req = Descriptor {
            monospace: true,
            ..Default::default()
        };
        let list = req.discover_descriptors();
        assert!(!list.is_empty(), "monospace search yields candidates");
        let ints: Vec<u32> = list.iter().map(|c| req.score(c).int()).collect();
        assert!(
            ints.windows(2).all(|w| w[0] >= w[1]),
            "scores are non-increasing (best-first): {ints:?}"
        );
    }

    #[test]
    fn discover_bold_ranks_bold_first() {
        // A bold Menlo request puts a bold candidate first.
        let req = Descriptor {
            family: Some("Menlo".into()),
            bold: true,
            ..Default::default()
        };
        let list = req.discover_descriptors();
        assert!(!list.is_empty(), "Menlo bold search yields candidates");
        assert!(
            req.score(&list[0]).bold,
            "the bold variant ranks first for a bold request"
        );
    }

    #[test]
    fn discover_faces_first_renders() {
        let req = Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        };
        let face = req.discover_faces().next().expect("a discovered face");
        assert!(
            face.glyph_index('M' as u32).is_some(),
            "the face renders 'M'"
        );
    }

    #[test]
    fn discover_faces_charset_removed() {
        // A codepoint-filtered search still yields the full font (the character
        // set was dropped): the face renders codepoints beyond the search one.
        let req = Descriptor {
            family: Some("Menlo".into()),
            codepoint: 'M' as u32,
            ..Default::default()
        };
        let face = req.discover_faces().next().expect("a discovered face");
        assert!(face.glyph_index('A' as u32).is_some(), "renders 'A'");
        assert!(face.glyph_index('z' as u32).is_some(), "renders 'z'");
    }

    #[test]
    fn discover_faces_lazy_smoke() {
        // The first face of a multi-candidate (monospace) search is usable.
        let req = Descriptor {
            monospace: true,
            ..Default::default()
        };
        let face = req.discover_faces().next().expect("a discovered face");
        assert!(face.glyph_index('M' as u32).is_some());
    }

    #[test]
    fn discover_fallback_cjk() {
        // A CJK ideograph goes straight to the codepoint search on the original
        // (Menlo) face, yielding a Han font that renders it.
        let menlo = Face::new("Menlo", 24.0);
        let req = Descriptor {
            codepoint: 0x4E00,
            ..Default::default()
        };
        let faces = req.discover_fallback_faces(&menlo);
        assert!(!faces.is_empty(), "the CJK gate finds a Han font");
        assert!(
            faces[0].glyph_index(0x4E00).is_some(),
            "the discovered font renders U+4E00"
        );
    }

    #[test]
    fn discover_fallback_general() {
        // A non-CJK request takes the general discovery path.
        let menlo = Face::new("Menlo", 24.0);
        let req = Descriptor {
            family: Some("Menlo".into()),
            ..Default::default()
        };
        let faces = req.discover_fallback_faces(&menlo);
        assert!(!faces.is_empty(), "the general path finds Menlo");
    }
}
