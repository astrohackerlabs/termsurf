//! Default codepoint tables used by selection logic.

/// Default boundary characters for word selection: ` \t'"│`|:;,()[]{}<>$`
pub(super) const DEFAULT_WORD_BOUNDARIES: &[u32] = &[
    0,           // null
    ' ' as u32,  // space
    '\t' as u32, // tab
    '\'' as u32, // single quote
    '"' as u32,  // double quote
    '│' as u32,  // U+2502 box drawing vertical line
    '`' as u32,  // backtick
    '|' as u32,  // pipe
    ':' as u32,  // colon
    ';' as u32,  // semicolon
    ',' as u32,  // comma
    '(' as u32,  // left paren
    ')' as u32,  // right paren
    '[' as u32,  // left bracket
    ']' as u32,  // right bracket
    '{' as u32,  // left brace
    '}' as u32,  // right brace
    '<' as u32,  // less than
    '>' as u32,  // greater than
    '$' as u32,  // dollar
];

/// Default whitespace characters trimmed from line selections.
pub(super) const DEFAULT_LINE_WHITESPACE: &[u32] = &[0, ' ' as u32, '\t' as u32];

#[cfg(test)]
mod tests {
    use super::*;

    fn has_no_duplicates(values: &[u32]) -> bool {
        for (index, value) in values.iter().enumerate() {
            if values[index + 1..].contains(value) {
                return false;
            }
        }

        true
    }

    #[test]
    fn default_word_boundaries_match_upstream_order() {
        assert_eq!(
            DEFAULT_WORD_BOUNDARIES,
            &[
                0,
                ' ' as u32,
                '\t' as u32,
                '\'' as u32,
                '"' as u32,
                '│' as u32,
                '`' as u32,
                '|' as u32,
                ':' as u32,
                ';' as u32,
                ',' as u32,
                '(' as u32,
                ')' as u32,
                '[' as u32,
                ']' as u32,
                '{' as u32,
                '}' as u32,
                '<' as u32,
                '>' as u32,
                '$' as u32,
            ]
        );
    }

    #[test]
    fn default_line_whitespace_matches_upstream_order() {
        assert_eq!(DEFAULT_LINE_WHITESPACE, &[0, ' ' as u32, '\t' as u32]);
    }

    #[test]
    fn default_line_whitespace_is_word_boundary_subset() {
        assert!(DEFAULT_LINE_WHITESPACE
            .iter()
            .all(|codepoint| DEFAULT_WORD_BOUNDARIES.contains(codepoint)));
    }

    #[test]
    fn default_selection_codepoints_have_no_duplicates() {
        assert!(has_no_duplicates(DEFAULT_WORD_BOUNDARIES));
        assert!(has_no_duplicates(DEFAULT_LINE_WHITESPACE));
    }
}
