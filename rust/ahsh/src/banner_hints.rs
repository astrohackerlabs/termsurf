//! Startup banner product-discovery lines (Issue 26072710436325).
//!
//! Command tokens (`ahweb`, `ahhelp`, `ah`) are painted **cyan** in the banner
//! to match Nu default `color_config.shape_external` (first external command
//! on the prompt), not green.

/// Unstyled product-hint lines for Full/Short banners (after Shift+Tab / mode hint).
pub const PRODUCT_HINT_LINES: &[&str] = &[
    "Type ahweb to browse the web.",
    "Type ahhelp for quick help.",
    "Type ah then Tab to see more commands.",
];

/// ANSI SGR for Nu default `shape_external: cyan` (`Color::Cyan.normal()`).
pub const SHAPE_EXTERNAL_ANSI: &str = "\x1b[36m";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_hints_mention_ahweb_ahhelp_and_tab() {
        let joined = PRODUCT_HINT_LINES.join("\n");
        assert!(joined.contains("ahweb"));
        assert!(joined.contains("browse the web"));
        assert!(joined.contains("ahhelp"));
        assert!(joined.contains("quick help"));
        assert!(joined.contains("ah then Tab") || joined.contains("Tab"));
        assert!(joined.contains("more commands"));
        assert_eq!(PRODUCT_HINT_LINES.len(), 3);
    }

    #[test]
    fn shape_external_ansi_is_cyan_not_green() {
        // nu-ansi-term Color::Cyan.normal() → SGR 36
        assert_eq!(SHAPE_EXTERNAL_ANSI, "\x1b[36m");
        assert_ne!(SHAPE_EXTERNAL_ANSI, "\x1b[32m");
    }
}
