//! Text shaping — the value types.
//!
//! Faithful port of upstream `font/shape.zig`, the shaper's output contract.
//! The shaper turns a run of terminal cells into positioned glyphs ([`Cell`]s);
//! this module defines that output ([`Cell`]), the clustered input
//! ([`Codepoint`]), OpenType feature parsing and [`Options`], and the
//! special-font fast path used by sprite/box-drawing runs.

/// A single shaped glyph to render, output by the shaper. Only cells with a
/// glyph to render are present. Faithful port of upstream `shape.Cell`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Cell {
    /// The X position of this cell relative to the run's offset. Runs are always
    /// within a single row, so the caller reconstructs the full position from the
    /// run offset, the row's Y, and this X.
    pub x: u16,
    /// An additional offset to apply when rendering.
    pub x_offset: i16,
    /// An additional offset to apply when rendering.
    pub y_offset: i16,
    /// The glyph index for this cell (valid for the run's font).
    pub glyph_index: u32,
}

/// One input codepoint paired with its cluster (the source cell), the shaper's
/// input contract. Mirrors upstream's `RunState.codepoints` entries, fed by
/// `addCodepoint(cp, cluster)`: the caller (the run iterator) supplies the
/// cluster, grouping a grapheme's codepoints into one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Codepoint {
    /// The Unicode scalar to shape.
    pub codepoint: u32,
    /// The terminal cell this codepoint belongs to. Drives the shaped `Cell.x`.
    pub cluster: u32,
}

/// An OpenType feature setting: a 4-byte tag and a numeric value (`0`/`1` for
/// boolean features; higher for alternates such as `cv01`). Faithful port of
/// upstream `shaper.Feature`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Feature {
    /// The 4-byte ASCII feature tag (e.g. `b"liga"`).
    pub tag: [u8; 4],
    /// The feature value (boolean features use `0`/`1`).
    pub value: u32,
}

/// The OpenType features hardcoded on by default. Users can disable them (e.g.
/// `-liga`). Faithful port of upstream `shape.default_features`.
pub(crate) fn default_features() -> Vec<Feature> {
    vec![Feature {
        tag: *b"liga",
        value: 1,
    }]
}

/// The states of the feature-string parser. Faithful port of upstream
/// `Feature.fromReader`'s `state:` switch.
enum FeatureState {
    /// Initial: skip leading whitespace, read an optional `+`/`-` and the tag.
    Start,
    /// Reading the 4-byte tag.
    Tag,
    /// The gap between the tag and the value.
    Space,
    /// Reading an integer value.
    Int,
    /// Reading an `on`/`off` keyword value.
    Bool,
    /// A complete value; skip trailing whitespace until the delimiter.
    Done,
    /// Unrecoverable syntax error; fast-forward to the boundary.
    Err,
}

/// Read one byte at `*pos`, advancing. End-of-input is reported as `,` (the
/// feature delimiter) **without** advancing — mirroring upstream's
/// `reader.readByte() catch ','`.
fn feature_read_byte(bytes: &[u8], pos: &mut usize) -> u8 {
    if *pos < bytes.len() {
        let b = bytes[*pos];
        *pos += 1;
        b
    } else {
        b','
    }
}

/// Advance `*pos` to just past the next `,` (or to end-of-input). Mirrors
/// upstream's `skipUntilDelimiterOrEof(',')`.
fn feature_skip_to_boundary(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() {
        let b = bytes[*pos];
        *pos += 1;
        if b == b',' {
            break;
        }
    }
}

impl Feature {
    /// Parse a single OpenType feature setting from `s`, in a subset of HarfBuzz's
    /// feature-string syntax (`"kern"`, `"+kern"`, `"-kern"`, `"kern on"`,
    /// `"kern off"`, `"aalt=2"`, …). Returns `None` for invalid syntax. Faithful
    /// port of upstream `Feature.fromString`.
    pub(crate) fn from_str(s: &str) -> Option<Feature> {
        let mut pos = 0;
        Feature::parse_one(s.as_bytes(), &mut pos)
    }

    /// Parse one feature from `bytes` starting at `*pos`, advancing `*pos` past it
    /// (and its trailing `,` if present). Returns `None` on invalid syntax,
    /// advancing through the next `,` so further features can be read. Faithful
    /// port of upstream `Feature.fromReader`.
    fn parse_one(bytes: &[u8], pos: &mut usize) -> Option<Feature> {
        let mut tag = [0u8; 4];
        let mut tag_len = 0usize;
        let mut value: Option<u32> = None;
        let mut state = FeatureState::Start;

        'sm: loop {
            match state {
                FeatureState::Start => loop {
                    match feature_read_byte(bytes, pos) {
                        b' ' | b'\t' => continue,
                        b',' => return None,
                        b'+' => {
                            value = Some(1);
                            state = FeatureState::Tag;
                            continue 'sm;
                        }
                        b'-' => {
                            value = Some(0);
                            state = FeatureState::Tag;
                            continue 'sm;
                        }
                        b'"' | b'\'' => {
                            state = FeatureState::Tag;
                            continue 'sm;
                        }
                        byte => {
                            tag[0] = byte;
                            tag_len = 1;
                            state = FeatureState::Tag;
                            continue 'sm;
                        }
                    }
                },
                FeatureState::Tag => loop {
                    match feature_read_byte(bytes, pos) {
                        b',' => return None,
                        b'"' | b'\'' => continue,
                        byte => {
                            tag[tag_len] = byte;
                            tag_len += 1;
                            if tag_len == 4 {
                                state = FeatureState::Space;
                                continue 'sm;
                            }
                        }
                    }
                },
                FeatureState::Space => loop {
                    match feature_read_byte(bytes, pos) {
                        b' ' | b'\t' | b'"' | b'\'' => continue,
                        // An `=` is allowed (and ignored) only without a prefix
                        // value; with a `+`/`-` value already set it is an error.
                        b'=' => {
                            if value.is_some() {
                                state = FeatureState::Err;
                                continue 'sm;
                            }
                        }
                        // Only a tag turns the feature on.
                        b',' => {
                            if value.is_none() {
                                value = Some(1);
                            }
                            break 'sm;
                        }
                        byte @ b'0'..=b'9' => {
                            if value.is_some() {
                                state = FeatureState::Err;
                                continue 'sm;
                            }
                            value = Some((byte - b'0') as u32);
                            state = FeatureState::Int;
                            continue 'sm;
                        }
                        b'o' | b'O' => {
                            if value.is_some() {
                                state = FeatureState::Err;
                                continue 'sm;
                            }
                            state = FeatureState::Bool;
                            continue 'sm;
                        }
                        _ => {
                            state = FeatureState::Err;
                            continue 'sm;
                        }
                    }
                },
                FeatureState::Int => loop {
                    match feature_read_byte(bytes, pos) {
                        b',' => break 'sm,
                        byte @ b'0'..=b'9' => {
                            match value
                                .unwrap()
                                .checked_mul(10)
                                .and_then(|v| v.checked_add((byte - b'0') as u32))
                            {
                                Some(v) => value = Some(v),
                                None => {
                                    state = FeatureState::Err;
                                    continue 'sm;
                                }
                            }
                        }
                        _ => {
                            state = FeatureState::Err;
                            continue 'sm;
                        }
                    }
                },
                FeatureState::Bool => loop {
                    match feature_read_byte(bytes, pos) {
                        b',' => return None,
                        b'n' | b'N' => {
                            // "ofn": a value already set (the first `f`) is an error.
                            if value.is_some() {
                                state = FeatureState::Err;
                                continue 'sm;
                            }
                            value = Some(1);
                            state = FeatureState::Done;
                            continue 'sm;
                        }
                        b'f' | b'F' => {
                            // First `f` sets the value; the second `f` finishes.
                            if value.is_none() {
                                value = Some(0);
                            } else {
                                state = FeatureState::Done;
                                continue 'sm;
                            }
                        }
                        _ => {
                            state = FeatureState::Err;
                            continue 'sm;
                        }
                    }
                },
                FeatureState::Done => loop {
                    match feature_read_byte(bytes, pos) {
                        b' ' | b'\t' => continue,
                        b',' => break 'sm,
                        _ => {
                            state = FeatureState::Err;
                            continue 'sm;
                        }
                    }
                },
                FeatureState::Err => {
                    feature_skip_to_boundary(bytes, pos);
                    return None;
                }
            }
        }

        // A valid feature has a complete tag and a resolved value.
        if tag_len == 4 {
            value.map(|value| Feature { tag, value })
        } else {
            None
        }
    }
}

/// Parse a comma-separated list of feature settings, dropping invalid entries.
/// Faithful port of upstream `FeatureList.fromString`.
pub(crate) fn parse_features(s: &str) -> Vec<Feature> {
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < bytes.len() {
        if let Some(f) = Feature::parse_one(bytes, &mut pos) {
            out.push(f);
        }
    }
    out
}

/// Shape a run with a special (sprite) font, whose glyph ids are the codepoints
/// themselves — the fast path that skips CoreText shaping. Each input codepoint
/// becomes a [`Cell`] (`glyph_index == codepoint`, `x == cluster`, zero offsets);
/// `codepoint == 0` entries are skipped (they only pad the UTF-16 string a real
/// shaping pass builds for CoreText). Faithful port of upstream `Shaper.shape`'s
/// special-font branch.
pub(crate) fn shape_special(run: &[Codepoint]) -> Vec<Cell> {
    run.iter()
        .filter(|cp| cp.codepoint != 0)
        .map(|cp| Cell {
            // A cluster is a terminal-cell column, always within `u16`. The
            // checked conversion mirrors upstream's `@intCast` (panic on overflow)
            // rather than silently truncating.
            x: u16::try_from(cp.cluster).expect("a shaped cluster must fit Cell.x (u16)"),
            x_offset: 0,
            y_offset: 0,
            glyph_index: cp.codepoint,
        })
        .collect()
}

/// Options controlling shaping. Faithful port of upstream `shape.Options`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Options {
    /// Font features to apply when shaping (e.g. `"liga"`, `"calt"`). Applied
    /// globally for now (upstream notes this may move to the face later).
    pub features: Vec<String>,
}

impl Options {
    /// The full feature list to apply when shaping: the [`default_features`]
    /// followed by the parsed user [`features`](Self::features) (each entry a
    /// comma-separated list). Faithful port of upstream's `Shaper.init` assembly
    /// (`feats_df = default_features ++ parsed`) — defaults first, so a trailing
    /// user setting (e.g. `-liga`) overrides a default (CoreText uses the last
    /// setting for a duplicated tag).
    pub(crate) fn merged_features(&self) -> Vec<Feature> {
        let mut out = default_features();
        for s in &self.features {
            out.extend(parse_features(s));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_defaults() {
        let c = Cell::default();
        assert_eq!(c.x, 0);
        assert_eq!(c.x_offset, 0);
        assert_eq!(c.y_offset, 0);
        assert_eq!(c.glyph_index, 0);
    }

    #[test]
    fn cell_construction() {
        // The set fields are kept and the offsets zero-default.
        let c = Cell {
            x: 3,
            glyph_index: 42,
            ..Default::default()
        };
        assert_eq!(c.x, 3);
        assert_eq!(c.glyph_index, 42);
        assert_eq!(c.x_offset, 0);
        assert_eq!(c.y_offset, 0);

        // The offsets are signed and hold negatives.
        let c = Cell {
            x: 1,
            x_offset: -2,
            y_offset: -5,
            glyph_index: 7,
        };
        assert_eq!(c.x_offset, -2);
        assert_eq!(c.y_offset, -5);
    }

    #[test]
    fn cell_eq_copy() {
        let a = Cell {
            x: 2,
            x_offset: 1,
            y_offset: -1,
            glyph_index: 9,
        };
        let b = a; // Copy
        assert_eq!(a, b);
        let mut c = a;
        c.glyph_index = 10;
        assert_ne!(a, c, "a differing glyph index is unequal");
    }

    #[test]
    fn default_features_is_liga() {
        assert_eq!(
            default_features(),
            vec![Feature {
                tag: *b"liga",
                value: 1
            }]
        );
    }

    #[test]
    fn feature_from_string_boolean_on() {
        let kern_on = Feature {
            tag: *b"kern",
            value: 1,
        };
        for s in [
            "kern",
            "kern, ",
            "kern on",
            "kern on, ",
            "+kern",
            "+kern, ",
            "\"kern\" = 1",
            "\"kern\" = 1, ",
        ] {
            assert_eq!(Feature::from_str(s), Some(kern_on), "parsing {s:?}");
        }
    }

    #[test]
    fn feature_from_string_boolean_off() {
        let kern_off = Feature {
            tag: *b"kern",
            value: 0,
        };
        for s in [
            "kern off",
            "kern off, ",
            "-'kern'",
            "-'kern', ",
            "\"kern\" = 0",
            "\"kern\" = 0, ",
        ] {
            assert_eq!(Feature::from_str(s), Some(kern_off), "parsing {s:?}");
        }
    }

    #[test]
    fn feature_from_string_numeric() {
        let aalt_2 = Feature {
            tag: *b"aalt",
            value: 2,
        };
        for s in ["aalt=2", "aalt=2, ", "'aalt' 2", "'aalt' 2, "] {
            assert_eq!(Feature::from_str(s), Some(aalt_2), "parsing {s:?}");
        }
    }

    #[test]
    fn feature_from_string_invalid() {
        for s in [
            "aalt=2x",   // bad number
            "toolong",   // tag too long
            "sht",       // tag too short
            "-kern 1",   // redundant/conflicting
            "-kern on",  // redundant/conflicting
            "aalt=o,",   // bad keyword
            "aalt=ofn,", // bad keyword
        ] {
            assert_eq!(Feature::from_str(s), None, "parsing {s:?}");
        }
    }

    #[test]
    fn merged_features_defaults_then_user() {
        let liga1 = Feature {
            tag: *b"liga",
            value: 1,
        };
        // Empty options → just the defaults.
        assert_eq!(Options::default().merged_features(), vec![liga1]);
        // Defaults first, then the parsed user features in order.
        let opts = Options {
            features: vec!["-liga".into(), "kern=2".into()],
        };
        assert_eq!(
            opts.merged_features(),
            vec![
                liga1,
                Feature {
                    tag: *b"liga",
                    value: 0
                },
                Feature {
                    tag: *b"kern",
                    value: 2
                },
            ]
        );
        // A single comma-separated entry parses to multiple features.
        let opts = Options {
            features: vec!["calt, -dlig".into()],
        };
        assert_eq!(
            opts.merged_features(),
            vec![
                liga1,
                Feature {
                    tag: *b"calt",
                    value: 1
                },
                Feature {
                    tag: *b"dlig",
                    value: 0
                },
            ]
        );
    }

    #[test]
    fn feature_from_string_overflow() {
        assert_eq!(
            Feature::from_str("aalt=4294967295"),
            Some(Feature {
                tag: *b"aalt",
                value: u32::MAX
            }),
        );
        assert_eq!(Feature::from_str("aalt=4294967296"), None, "overflow");
    }

    #[test]
    fn feature_list_from_string() {
        let s = concat!(
            "  kern, kern on , +kern, \"kern\"  = 1,", // 4× kern=1
            "kern    off, -'kern' , \"kern\"=0,",      // 3× kern=0
            "aalt=2,  'aalt'\t2,",                     // 2× aalt=2
            "aalt=2x, toolong, sht, -kern 1, -kern on, aalt=o, aalt=ofn,", // invalid
            "last",                                    // last=1
        );
        let kern1 = Feature {
            tag: *b"kern",
            value: 1,
        };
        let kern0 = Feature {
            tag: *b"kern",
            value: 0,
        };
        let aalt2 = Feature {
            tag: *b"aalt",
            value: 2,
        };
        let last = Feature {
            tag: *b"last",
            value: 1,
        };
        let expected = vec![
            kern1, kern1, kern1, kern1, kern0, kern0, kern0, aalt2, aalt2, last,
        ];
        assert_eq!(parse_features(s), expected);
    }

    #[test]
    fn shape_special_codepoint_is_glyph() {
        // Box-drawing scalars: each becomes a cell whose glyph id is the codepoint
        // and whose x is the cluster, with zero offsets.
        let run = [
            Codepoint {
                codepoint: 0x2500,
                cluster: 0,
            },
            Codepoint {
                codepoint: 0x2502,
                cluster: 1,
            },
            Codepoint {
                codepoint: 0x256C,
                cluster: 2,
            },
        ];
        let cells = shape_special(&run);
        assert_eq!(cells.len(), 3);
        for (i, cp) in run.iter().enumerate() {
            assert_eq!(
                cells[i],
                Cell {
                    x: cp.cluster as u16,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: cp.codepoint,
                }
            );
        }
    }

    #[test]
    fn shape_special_skips_zero() {
        // A `codepoint == 0` entry is skipped.
        let run = [
            Codepoint {
                codepoint: 0,
                cluster: 0,
            },
            Codepoint {
                codepoint: 'A' as u32,
                cluster: 1,
            },
        ];
        let cells = shape_special(&run);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].glyph_index, 'A' as u32);
        assert_eq!(cells[0].x, 1);
    }

    #[test]
    fn shape_special_high_plane() {
        // A supplementary-plane sprite scalar survives in the u32 glyph_index.
        let run = [Codepoint {
            codepoint: 0x1FB70,
            cluster: 0,
        }];
        let cells = shape_special(&run);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].glyph_index, 0x1FB70);
        assert_eq!(cells[0].x, 0);
    }

    #[test]
    fn shape_special_empty() {
        assert!(shape_special(&[]).is_empty());
    }

    #[test]
    fn options_default_empty() {
        assert!(Options::default().features.is_empty());
        let o = Options {
            features: vec!["liga".to_string(), "calt".to_string()],
        };
        assert_eq!(o.features, vec!["liga".to_string(), "calt".to_string()]);
    }
}
