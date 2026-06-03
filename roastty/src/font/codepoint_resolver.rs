//! Resolves a codepoint to the font face that should render it.
//!
//! Faithful port of the core resolution chain of upstream
//! `font/CodepointResolver.zig`. The resolver sits on top of a [`Collection`]
//! and adds style-disabled fallback, presentation defaults, and the
//! regular-style fallback chain. The sprite face, codepoint overrides, the UCD
//! emoji-presentation default, and discovery-based fallback are deferred to
//! later experiments.

use crate::font::collection::{Collection, Index, PresentationMode};
use crate::font::{Presentation, Style};

/// Resolves codepoints to face indices over a [`Collection`].
pub(crate) struct CodepointResolver {
    /// The collection of faces this resolver searches.
    collection: Collection,
    /// Whether each style is enabled, indexed by `Style as usize`. A disabled
    /// non-regular style falls back to regular. Faithful port of upstream's
    /// `StyleStatus` (`EnumArray(Style, bool)`).
    styles: [bool; 4],
}

impl CodepointResolver {
    /// Create a resolver over `collection` with all styles enabled.
    pub(crate) fn new(collection: Collection) -> CodepointResolver {
        CodepointResolver {
            collection,
            styles: [true; 4],
        }
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

    /// Resolve `cp` (in `style`, with optional explicit presentation `p`) to a
    /// face [`Index`], or `None`. Faithful port of upstream `getIndex`'s core
    /// chain (sprite, codepoint overrides, the UCD presentation default, and
    /// discovery are deferred).
    pub(crate) fn get_index(
        &self,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
    ) -> Option<Index> {
        // A disabled non-regular style falls back to regular.
        if style != Style::Regular && !self.styles[style as usize] {
            return self.get_index(cp, Style::Regular, p);
        }

        // (Codepoint overrides and the sprite face check are deferred here.)

        // Build the presentation mode. With an explicit presentation we use it;
        // otherwise we'd consult the Unicode Character Database for the default
        // (deferred — `None` defaults to text for now).
        let p_mode = match p {
            Some(v) => PresentationMode::Explicit(v),
            None => PresentationMode::Default(Presentation::Text),
        };

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

        // (Discovery-based fallback is deferred here.)

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
    fn resolve_basic() {
        let r = menlo_resolver();
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
        let r = menlo_resolver();
        // A Private-Use codepoint Menlo lacks; discovery is deferred -> None.
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
        let r = CodepointResolver::new(c);
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
}
