//! X11 named color lookup.
//!
//! The embedded `res/rgb.txt` data is the X11 `rgb.txt` color database, copied
//! via Ghostty's vendored source for Roastty parity work. Ghostty documents this
//! data as MIT/X11 licensed; keep that provenance with this lookup and the
//! embedded data.

use super::color::Rgb;

const RGB_TXT: &str = include_str!("res/rgb.txt");

pub(super) fn get(name: &[u8]) -> Option<Rgb> {
    let name = trim_edge_spaces(name);
    let name = std::str::from_utf8(name).ok()?;

    for line in RGB_TXT.lines() {
        if line.is_empty() {
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 13 {
            continue;
        }
        let color_name = line[12..].trim_matches(|ch| ch == ' ' || ch == '\t');
        if color_name.eq_ignore_ascii_case(name) {
            let r = line[0..3].trim().parse::<u8>().ok()?;
            let g = line[4..7].trim().parse::<u8>().ok()?;
            let b = line[8..11].trim().parse::<u8>().ok()?;
            return Some(Rgb::new(r, g, b));
        }
    }

    None
}

fn trim_edge_spaces(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start] == b' ' {
        start += 1;
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_color_lookup_matches_ghostty_examples() {
        assert_eq!(get(b"nosuchcolor"), None);
        assert_eq!(get(b"white"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(get(b"medium spring green"), Some(Rgb::new(0, 250, 154)));
        assert_eq!(get(b"ForestGreen"), Some(Rgb::new(34, 139, 34)));
        assert_eq!(get(b"FoReStGReen"), Some(Rgb::new(34, 139, 34)));
        assert_eq!(get(b"black"), Some(Rgb::new(0, 0, 0)));
        assert_eq!(get(b"red"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(get(b"green"), Some(Rgb::new(0, 255, 0)));
        assert_eq!(get(b"blue"), Some(Rgb::new(0, 0, 255)));
        assert_eq!(get(b"lawngreen"), Some(Rgb::new(124, 252, 0)));
        assert_eq!(get(b"mediumspringgreen"), Some(Rgb::new(0, 250, 154)));
    }

    #[test]
    fn x11_color_lookup_trims_only_edge_spaces() {
        assert_eq!(get(b" red "), Some(Rgb::new(255, 0, 0)));
        assert_eq!(get(b"\tred\t"), None);
        assert_eq!(get(b"\nred\n"), None);
    }
}
