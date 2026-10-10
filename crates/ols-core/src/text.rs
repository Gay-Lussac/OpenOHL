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

/// Natural order of names: runs of digits compare as numbers, the rest
/// without case — `sec2` < `sec10` < `sec100`, `40x002` < `40x010`,
/// `T9` < `t10`. Ties (`sec01` and `sec1`, `a` and `A`) fall back to the
/// plain order, so it is a total order.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a, b);
    while let (Some(p), Some(q)) = (x.chars().next(), y.chars().next()) {
        let o = if p.is_ascii_digit() && q.is_ascii_digit() {
            let n = x.find(|c: char| !c.is_ascii_digit()).unwrap_or(x.len());
            let m = y.find(|c: char| !c.is_ascii_digit()).unwrap_or(y.len());
            let (u, v) = (
                x[..n].trim_start_matches('0'),
                y[..m].trim_start_matches('0'),
            );
            let o = u.len().cmp(&v.len()).then_with(|| u.cmp(v));
            x = &x[n..];
            y = &y[m..];
            o
        } else {
            let o = p.to_lowercase().cmp(q.to_lowercase());
            x = &x[p.len_utf8()..];
            y = &y[q.len_utf8()..];
            o
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    #[test]
    fn natural_order() {
        let mut v = vec![
            "sec110", "sec1", "sec10", "sec100", "sec109", "sec11", "sec2", "40x010", "40x002",
            "T10", "t9", "K7x001", "sec01", "",
        ];
        v.sort_by(|a, b| super::natural_cmp(a, b));
        assert_eq!(
            v,
            [
                "", "40x002", "40x010", "K7x001", "sec01", "sec1", "sec2", "sec10", "sec11",
                "sec100", "sec109", "sec110", "t9", "T10"
            ]
        );
        // Huge numbers: no overflow.
        assert!(
            super::natural_cmp("a99999999999999999999999", "a100000000000000000000000").is_lt()
        );
    }

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
