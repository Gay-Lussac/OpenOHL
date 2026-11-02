// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bolt libraries (`.ohlb`, format `ohl-bolts`; owner 2026-11-02: "a bolt
//! library … so that it can be easily added and imported/exported … it'll
//! help storing the right data"): the bolts a tower's connections name
//! (`bolts = { count = 2, bolt = "M16-8.8" }`), kept like the
//! cross-section libraries (`Bolts/` next to a structure).
//!
//! ```toml
//! format = "ohl-bolts"
//! version = 1
//!
//! [[bolt]]
//! id = "M16-8.8"
//! name = "M16 grade 8.8"
//! diameter = "16 mm"
//! hole = "17.5 mm"              # optional
//! grade = "8.8"                 # optional, as the standard names it
//! ultimate = "800 MPa"          # optional: the bolt's tensile strength
//! yield = "640 MPa"             # optional
//! shear_capacity = "60 kN"      # optional: per shear plane, threads in it
//! shear_capacity_shank = "75 kN" # optional: per shear plane, threads out
//! tension_capacity = "90 kN"    # optional
//! head_across_flats = "24 mm"   # optional: for the drawings
//! head_height = "10 mm"         # optional
//! nut_height = "13 mm"          # optional
//! grip = "20 mm"                # optional: what it clamps, as sold
//! mass_kg = 0.12                # optional: with its nut and washer
//! description = "…"             # optional
//! ```
//!
//! None of a standard's numbers is built in: the values are the library's.
//! A library shipped as an example holds invented ones.

use std::path::{Path, PathBuf};

use ohl_core::units::{Force, Length, Stress};
use serde::Deserialize;

/// The extension of bolt libraries.
pub const BOLT_FILE_EXTENSION: &str = "ohlb";
/// Their format.
pub const BOLT_FILE_FORMAT: &str = "ohl-bolts";
/// The newest version read.
pub const BOLT_FILE_VERSION: u32 = 1;
/// The folder of bolt libraries beside a structure.
pub const BOLTS_FOLDER: &str = "Bolts";

/// A bolt of a library.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoltType {
    /// Id, unique in the library (a connection names it).
    pub id: String,
    /// Its name (none: the id).
    #[serde(default)]
    pub name: Option<String>,
    /// Free text.
    #[serde(default)]
    pub description: Option<String>,
    /// Diameter.
    pub diameter: Length,
    /// Hole diameter.
    #[serde(default)]
    pub hole: Option<Length>,
    /// Grade, as the standard names it.
    #[serde(default)]
    pub grade: Option<String>,
    /// Ultimate tensile strength.
    #[serde(default)]
    pub ultimate: Option<Stress>,
    /// Yield strength.
    #[serde(default, rename = "yield")]
    pub yield_strength: Option<Stress>,
    /// Shear capacity per shear plane, the threads in the plane.
    #[serde(default)]
    pub shear_capacity: Option<Force>,
    /// Shear capacity per shear plane, the threads out of it.
    #[serde(default)]
    pub shear_capacity_shank: Option<Force>,
    /// Tension capacity.
    #[serde(default)]
    pub tension_capacity: Option<Force>,
    /// Across the head's flats (the drawings).
    #[serde(default)]
    pub head_across_flats: Option<Length>,
    /// The head's height (the drawings).
    #[serde(default)]
    pub head_height: Option<Length>,
    /// The nut's height (the drawings).
    #[serde(default)]
    pub nut_height: Option<Length>,
    /// What it clamps, as sold.
    #[serde(default)]
    pub grip: Option<Length>,
    /// Its mass with its nut and washer, kg.
    #[serde(default)]
    pub mass_kg: Option<f64>,
}

impl BoltType {
    /// Its name, else its id.
    pub fn title(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// The sizes it is drawn with, m: across the flats, the head's and
    /// the nut's height (each when the library gives it).
    pub fn drawn(&self) -> [Option<f64>; 3] {
        [
            self.head_across_flats.map(|l| l.si()),
            self.head_height.map(|l| l.si()),
            self.nut_height.map(|l| l.si()),
        ]
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default, rename = "bolt")]
    bolts: Vec<BoltType>,
}

/// Bolts, from one library file or several.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoltLibrary {
    /// The bolts, in the files' order.
    pub bolts: Vec<BoltType>,
}

impl BoltLibrary {
    /// A library file's text: refused when another format, a newer
    /// version, a field unknown, an id empty or given twice, a diameter
    /// not above 0, a hole smaller than its bolt.
    pub fn parse(text: &str) -> Result<Self, String> {
        let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            f.format.as_deref(),
            f.version,
            BOLT_FILE_FORMAT,
            BOLT_FILE_VERSION,
            "bolt library",
        )?;
        let lib = Self { bolts: f.bolts };
        lib.check()?;
        Ok(lib)
    }

    fn check(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for b in &self.bolts {
            if b.id.trim().is_empty() || !seen.insert(b.id.as_str()) {
                return Err(format!("bolt \"{}\": empty or given twice", b.id));
            }
            if b.diameter.si() <= 0.0 {
                return Err(format!("bolt {}: a diameter above 0", b.id));
            }
            if b.hole.is_some_and(|h| h.si() < b.diameter.si()) {
                return Err(format!("bolt {}: a hole no smaller than the bolt", b.id));
            }
            let sizes = [b.head_across_flats, b.head_height, b.nut_height, b.grip];
            if sizes.iter().flatten().any(|l| l.si() <= 0.0) || b.mass_kg.is_some_and(|m| m <= 0.0)
            {
                return Err(format!("bolt {}: sizes and mass above 0", b.id));
            }
        }
        Ok(())
    }

    /// The library file at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every library file in `dirs` (a folder's `.ohlb` files, or a file),
    /// together; the files that do not read, with why. A bolt id given by
    /// two files: the first file's kept, said.
    pub fn load_all(dirs: &[PathBuf]) -> (Self, Vec<String>) {
        let mut lib = Self::default();
        let mut errors = Vec::new();
        for d in dirs {
            let files = if d.is_file() {
                vec![d.clone()]
            } else {
                ohl_core::files::files_with_extension(d, BOLT_FILE_EXTENSION)
            };
            for f in files {
                match Self::load(&f) {
                    Ok(l) => {
                        for b in l.bolts {
                            if lib.get(&b.id).is_some() {
                                errors.push(format!(
                                    "{}: bolt {} given by another library too (that one kept)",
                                    f.display(),
                                    b.id
                                ));
                            } else {
                                lib.bolts.push(b);
                            }
                        }
                    }
                    Err(e) => errors.push(e),
                }
            }
        }
        (lib, errors)
    }

    /// The bolt libraries of the structures in folder `dir`: those in its
    /// [`BOLTS_FOLDER`] ([`Self::load_all`]; none there: an empty
    /// library).
    pub fn beside(dir: &Path) -> (Self, Vec<String>) {
        Self::load_all(&[dir.join(BOLTS_FOLDER)])
    }

    /// The bolt `id`.
    pub fn get(&self, id: &str) -> Option<&BoltType> {
        self.bolts.iter().find(|b| b.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: &str = "format = \"ohl-bolts\"\nversion = 1\n\
        [[bolt]]\nid = \"M16\"\ndiameter = \"16 mm\"\nhole = \"17.5 mm\"\ngrade = \"8.8\"\n\
        ultimate = \"800 MPa\"\nshear_capacity = \"60 kN\"\nhead_across_flats = \"24 mm\"\n\
        mass_kg = 0.1\n";

    #[test]
    fn a_library_is_read_and_checked() {
        let l = BoltLibrary::parse(LIB).unwrap();
        let b = l.get("M16").unwrap();
        assert!((b.diameter.si() - 0.016).abs() < 1e-12);
        assert!((b.ultimate.unwrap().si() - 800e6).abs() < 1.0);
        assert_eq!(b.title(), "M16");
        assert!((b.drawn()[0].unwrap() - 0.024).abs() < 1e-12);
        for (bad, says) in [
            (
                format!("{LIB}[[bolt]]\nid = \"M16\"\ndiameter = \"16 mm\"\n"),
                "given twice",
            ),
            (
                LIB.replace("hole = \"17.5 mm\"", "hole = \"15 mm\""),
                "no smaller",
            ),
            (
                LIB.replace("diameter = \"16 mm\"", "diameter = \"0 mm\""),
                "above 0",
            ),
            (format!("{LIB}torque = 3\n"), "unknown field"),
            (LIB.replace("version = 1", "version = 2"), "newer"),
            (LIB.replace("ohl-bolts", "ohl-wire"), "ohl-bolts"),
        ] {
            let e = BoltLibrary::parse(&bad).unwrap_err();
            assert!(e.contains(says), "{says}: {e}");
        }
    }

    /// The example library shipped reads.
    #[test]
    fn the_example_library_reads() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/bolts/example-bolts.ohlb");
        let l = BoltLibrary::load(&path).unwrap();
        assert!(l.bolts.len() >= 3);
    }
}
