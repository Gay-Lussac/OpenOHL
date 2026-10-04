// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Small text forms shared by files and exports: CSV fields and `#rrggbb`
//! colours.

/// A CSV field: quoted (with `"` doubled) when it holds a comma, a quote
/// or a line break.
pub fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// `#rrggbb` (either case) as RGB.
pub fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |k: usize| u8::from_str_radix(&h[k..k + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// RGB as `#rrggbb`.
pub fn format_rgb(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_fields_are_quoted_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn colours() {
        assert_eq!(parse_rgb("#A0b0C0"), Some([0xa0, 0xb0, 0xc0]));
        assert_eq!(parse_rgb("a0b0c0"), None);
        assert_eq!(parse_rgb("#aé123"), None);
        assert_eq!(parse_rgb("#zz0000"), None);
        assert_eq!(parse_rgb("#fff"), None);
        assert_eq!(format_rgb([1, 2, 255]), "#0102ff");
    }
}
