#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underline_default_matches_upstream() {
        assert_eq!(Underline::default(), Underline::None);
    }
}
