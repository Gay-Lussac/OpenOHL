// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Name patterns with wildcards, for choosing things by name (weather
//! cases, structures, …): `*` any text, `?` one character,
//! case-insensitive.

/// Whether `text` matches `pattern`.
pub fn matches(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (
        pattern.to_lowercase().chars().collect(),
        text.to_lowercase().chars().collect(),
    );
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn wildcards() {
        assert!(matches("con_*", "con_0042-013-1"));
        assert!(matches("T?", "t4"));
        assert!(!matches("T?", "T12"));
        assert!(matches("*ice*", "glaze ICE + wind"));
        assert!(!matches("max", "max temperature"));
        assert!(matches("*", ""));
    }
}
