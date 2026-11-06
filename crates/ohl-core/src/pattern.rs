// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Name patterns with wildcards, for choosing things by name (weather
//! cases, structures, …): `*` any text, `?` one character,
//! case-insensitive.

/// Whether `text` matches one of `patterns`; an empty list matches every
/// text (a rule without cases applies to all of them).
pub fn matches_list(patterns: &[String], text: &str) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| matches(p, text))
}

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
    use super::{matches, matches_list};

    #[test]
    fn an_empty_list_matches_every_text() {
        assert!(matches_list(&[], "any case"));
        let list = ["ice*".to_owned()];
        assert!(matches_list(&list, "Ice heavy"));
        assert!(!matches_list(&list, "high wind"));
    }

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
