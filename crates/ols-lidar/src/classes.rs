// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Point class schemes (`*.olsc`).
//!
//! What a class number means depends on who classified the data: the ASPRS
//! LAS standard ([`ClassScheme::asprs`]) or a utility's or surveyor's own
//! scheme. A scheme file names every class and gives it **roles** — free
//! words such as `ground`, `vegetation`, `building`, `conductor` — and
//! tools pick classes by role ([`ClassScheme::codes_with_role`]), so the
//! same tools work with any scheme. A class may have several roles (an
//! asphalt road is `ground` and `road`); roles are not a fixed list.
//!
//! ```toml
//! format = "ols-lidar-classes"
//! version = 1
//! id = "my-survey"
//! name = "Survey company X, project Y"
//!
//! [[class]]
//! code = 1
//! name = "Terrain"
//! roles = ["ground"]
//! color = "#96805f"   # display colour (optional; else from the roles)
//! ```
//!
//! Roles used by OLS CAD so far: `ground` (terrain surface). Suggested for
//! clearance rules and future tools: `vegetation`, `building`, `water`,
//! `road`, `railway`, `bridge`, `structure`, `insulator`, `conductor`,
//! `shield_wire`, `load_object`, `other_line_structure`,
//! `other_line_conductor`, `object`, `noise`, `temporary`,
//! `unclassified`.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// File extension of class schemes.
pub const CLASS_FILE_EXTENSION: &str = "olsc";
/// Format identifier.
pub const CLASS_FILE_FORMAT: &str = "ols-lidar-classes";

const ASPRS: &str = include_str!("../../../libraries/lidar-classes/asprs-las-1.4.olsc");

/// One class of a scheme.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDef {
    /// Class number in the point cloud.
    pub code: u8,
    /// Name.
    pub name: String,
    /// Roles (free words, lower case).
    #[serde(default)]
    pub roles: Vec<String>,
    /// Notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Display colour `#rrggbb` (none: from the roles).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl ClassDef {
    /// The display colour given, as RGB.
    pub fn rgb(&self) -> Option<[u8; 3]> {
        self.color.as_deref().and_then(parse_rgb)
    }
}

/// `#rrggbb` as RGB.
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

/// A class scheme.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassScheme {
    /// `"ols-lidar-classes"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Where the scheme comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Classes.
    #[serde(rename = "class", default)]
    pub classes: Vec<ClassDef>,
}

impl ClassScheme {
    /// Parses and checks a scheme.
    pub fn parse(text: &str) -> Result<Self, String> {
        let scheme: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        if let Some(f) = scheme.format.as_deref().filter(|f| *f != CLASS_FILE_FORMAT) {
            return Err(format!("format \"{f}\" is not \"{CLASS_FILE_FORMAT}\""));
        }
        if scheme.version.is_some_and(|v| v > 1) {
            return Err("class scheme version newer than supported (1)".into());
        }
        let mut codes = BTreeSet::new();
        for c in &scheme.classes {
            if !codes.insert(c.code) {
                return Err(format!("class {} is defined twice", c.code));
            }
            if let Some(col) = &c.color
                && parse_rgb(col).is_none()
            {
                return Err(format!("class {}: color \"{col}\" is not #rrggbb", c.code));
            }
            if let Some(r) = c.roles.iter().find(|r| {
                r.is_empty()
                    || !r
                        .chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
            }) {
                return Err(format!(
                    "class {}: role \"{r}\" must be lower-case letters, digits and _",
                    c.code
                ));
            }
        }
        Ok(scheme)
    }

    /// Reads a scheme file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The scheme as a file (`.olsc`).
    pub fn to_toml(&self) -> String {
        let mut s = self.clone();
        s.format = Some(CLASS_FILE_FORMAT.into());
        s.version = Some(1);
        toml::to_string(&s).unwrap_or_default()
    }

    /// The ASPRS LAS 1.4 standard classes (`libraries/lidar-classes`).
    pub fn asprs() -> Self {
        Self::parse(ASPRS).expect("built-in scheme is valid")
    }

    /// The class with this number.
    pub fn class(&self, code: u8) -> Option<&ClassDef> {
        self.classes.iter().find(|c| c.code == code)
    }

    /// Class numbers having `role`, ascending.
    pub fn codes_with_role(&self, role: &str) -> Vec<u8> {
        let mut codes: Vec<u8> = self
            .classes
            .iter()
            .filter(|c| c.roles.iter().any(|r| r == role))
            .map(|c| c.code)
            .collect();
        codes.sort_unstable();
        codes
    }

    /// All roles used.
    pub fn roles(&self) -> BTreeSet<&str> {
        self.classes
            .iter()
            .flat_map(|c| c.roles.iter().map(String::as_str))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_with_other_characters_are_refused() {
        assert_eq!(parse_rgb("#aé123"), None);
        assert_eq!(parse_rgb("#zz0000"), None);
        assert_eq!(parse_rgb("#A0b0C0"), Some([0xa0, 0xb0, 0xc0]));
    }

    #[test]
    fn colours_are_read_checked_and_written() {
        let text = "id = \"x\"\nname = \"X\"\n[[class]]\ncode = 3\nname = \"Tree\"\nroles = [\"vegetation\"]\ncolor = \"#40a050\"\n[[class]]\ncode = 4\nname = \"Other\"\n";
        let s = ClassScheme::parse(text).unwrap();
        assert_eq!(s.class(3).unwrap().rgb(), Some([0x40, 0xa0, 0x50]));
        assert_eq!(s.class(4).unwrap().rgb(), None);
        assert_eq!(ClassScheme::parse(&s.to_toml()).unwrap(), {
            let mut t = s.clone();
            t.format = Some(CLASS_FILE_FORMAT.into());
            t.version = Some(1);
            t
        });
        assert!(ClassScheme::parse(&text.replace("#40a050", "green")).is_err());
        assert_eq!(format_rgb([1, 2, 255]), "#0102ff");
    }

    #[test]
    fn asprs_scheme_has_ground_and_wires() {
        let s = ClassScheme::asprs();
        assert_eq!(s.codes_with_role("ground"), vec![2, 9, 11]);
        assert_eq!(s.codes_with_role("conductor"), vec![14]);
        assert_eq!(s.class(6).unwrap().name, "Building");
        assert!(s.roles().contains("vegetation"));
    }

    #[test]
    fn any_scheme_and_roles() {
        let s = ClassScheme::parse(
            "id = \"x\"\nname = \"X\"\n[[class]]\ncode = 72\nname = \"Leiter\"\nroles = [\"conductor\", \"own_line\"]\n[[class]]\ncode = 1\nname = \"Boden\"\nroles = [\"ground\"]\n",
        )
        .unwrap();
        assert_eq!(s.codes_with_role("own_line"), vec![72]);
        assert_eq!(s.codes_with_role("ground"), vec![1]);
        assert!(s.codes_with_role("water").is_empty());
        assert!(ClassScheme::parse("id = \"x\"\nname = \"X\"\n[[class]]\ncode = 1\nname = \"a\"\n[[class]]\ncode = 1\nname = \"b\"\n").is_err());
        assert!(ClassScheme::parse("id = \"x\"\nname = \"X\"\n[[class]]\ncode = 1\nname = \"a\"\nroles = [\"Ground\"]\n").is_err());
    }
}
