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
//! tension_capacity = "90 kN"    # optional: its ultimate tensile load, F_u × A_s
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
//!
//! **A bolt's `tension_capacity`** (owner 2026-11-19) is its **ultimate
//! tensile load**: its ultimate tensile strength × its stress area,
//! F_u × A_s (Kulak, Fisher & Struik's guide, 17.6.1: the tensile capacity
//! of a fastener is its stress area × its minimum specified tensile
//! strength) — not a design value: each design code takes its share of it
//! (ASCE 10-97 4.3.3: 0.6 of it; SIA 263 (77): 0.9 of it over γ_M2).
//!
//! **Bolt grades set by command** (owner 2026-11-19: "add a command to set
//! it"): a structure's members' bolts take a grade's strengths from the
//! structure file's `[[bolt_grade]]` ([`BoltGrade`], structure file version
//! 21), which OHL-Structure's Design › Bolt grade… writes from
//! [`SIA263_GRADES`] — the strength classes of SIA 263:2013, 3.4.2, Table 2,
//! read in the standard's text. Nothing takes a grade by itself: a bolt
//! without strengths stays without them (its checks said NOT ON THE SAFE
//! SIDE).

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

/// How the bolt tools, the bolt layout form, the plate outlines and the
/// joint layout name the default minimums wherever they show them (owner
/// 2026-11-18: "said everywhere as default minimum, assumed, not a
/// code's").
pub const DEFAULT_MINIMUM_WORDS: &str = "default minimum, assumed, not a code's";

/// The default minimum bolt distances of a structure, in bolt diameters
/// (owner 2026-11-18, third round: "ok but put a default minimum"): what
/// the bolt tools, the bolt layout form, the plate outlines and the joint
/// layout keep where no design code gives minimums — the simple check, a
/// plugin without minimum distances, or one that returns none for the
/// bolt. Never in place of a code's own minimums, and not a code's:
/// assumed, said so wherever shown. A structure file's
/// `[default_bolt_minimums]` (version 18, the file raised only when one is
/// given); every key optional.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultBoltMinimums {
    /// End distance (from where the steel stops to a bolt), bolt
    /// diameters.
    #[serde(default = "DefaultBoltMinimums::end_start")]
    pub end: f64,
    /// Edge distance (from a bolt to the leg's or plate's free edge
    /// across the force), bolt diameters.
    #[serde(default = "DefaultBoltMinimums::edge_start")]
    pub edge: f64,
    /// Spacing (between two bolts), bolt diameters.
    #[serde(default = "DefaultBoltMinimums::spacing_start")]
    pub spacing: f64,
}

impl DefaultBoltMinimums {
    /// The starting end distance: 1.5 bolt diameters (what the plate
    /// outlines and the joints' drawing already assumed).
    pub const END: f64 = 1.5;
    /// The starting edge distance: 1.5 bolt diameters.
    pub const EDGE: f64 = 1.5;
    /// The starting spacing: 3 bolt diameters (what the joints' drawing
    /// already assumed for a pitch not given).
    pub const SPACING: f64 = 3.0;

    fn end_start() -> f64 {
        Self::END
    }

    fn edge_start() -> f64 {
        Self::EDGE
    }

    fn spacing_start() -> f64 {
        Self::SPACING
    }

    /// Each value a number above 0. Err: which is not.
    pub fn validate(&self) -> Result<(), String> {
        for (v, what) in [
            (self.end, "end"),
            (self.edge, "edge"),
            (self.spacing, "spacing"),
        ] {
            if !(v.is_finite() && v > 0.0) {
                return Err(format!(
                    "default_bolt_minimums: {what} must be a number of bolt diameters above 0"
                ));
            }
        }
        Ok(())
    }

    /// The minimums for a bolt of diameter `d`, m: end, spacing, edge.
    pub fn of(&self, d: f64) -> [f64; 3] {
        [self.end * d, self.spacing * d, self.edge * d]
    }

    /// The clause words of each: "1.5 d" (end, spacing, edge).
    pub fn clauses(&self) -> [String; 3] {
        [self.end, self.spacing, self.edge].map(|x| format!("{x} d"))
    }
}

impl Default for DefaultBoltMinimums {
    fn default() -> Self {
        Self {
            end: Self::END,
            edge: Self::EDGE,
            spacing: Self::SPACING,
        }
    }
}

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
    /// Tension capacity: the bolt's **ultimate tensile load**, its
    /// ultimate tensile strength × its stress area (F_u × A_s; owner
    /// 2026-11-19) — not a design value (each code takes its share).
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

/// Where [`SIA263_GRADES`]' values come from.
pub const SIA263_GRADES_SOURCE: &str = "SIA 263:2013, 3.4.2, Table 2";

/// A strength class of bolts as a standard's table gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardGrade {
    /// Its name ("8.8").
    pub grade: &'static str,
    /// Yield strength f_yb, N/mm².
    pub yield_mpa: f64,
    /// Ultimate tensile strength f_ub, N/mm².
    pub ultimate_mpa: f64,
    /// Minimum elongation at rupture, %.
    pub elongation_pct: f64,
    /// Used in Switzerland (Table 2's footnote 1: 4.8, 5.8 and 6.8 are
    /// not, their elongation too low).
    pub used_in_switzerland: bool,
}

/// The strength classes of bolts of SIA 263:2013, 3.4.2, Table 2 ("Limite
/// d'élasticité f_yb, résistance à la traction f_ub et allongement minimal
/// de rupture ε_u des boulons", the classes as SN EN 20898-2 names them),
/// read in the standard's text (its page 18) on 2026-11-19: the grades the
/// bolt grade command offers (owner 2026-11-19). SN EN 20898-2 itself was
/// not at hand.
pub const SIA263_GRADES: [StandardGrade; 7] = [
    StandardGrade {
        grade: "4.6",
        yield_mpa: 240.0,
        ultimate_mpa: 400.0,
        elongation_pct: 22.0,
        used_in_switzerland: true,
    },
    StandardGrade {
        grade: "4.8",
        yield_mpa: 320.0,
        ultimate_mpa: 400.0,
        elongation_pct: 14.0,
        used_in_switzerland: false,
    },
    StandardGrade {
        grade: "5.6",
        yield_mpa: 300.0,
        ultimate_mpa: 500.0,
        elongation_pct: 20.0,
        used_in_switzerland: true,
    },
    StandardGrade {
        grade: "5.8",
        yield_mpa: 400.0,
        ultimate_mpa: 500.0,
        elongation_pct: 10.0,
        used_in_switzerland: false,
    },
    StandardGrade {
        grade: "6.8",
        yield_mpa: 480.0,
        ultimate_mpa: 600.0,
        elongation_pct: 8.0,
        used_in_switzerland: false,
    },
    StandardGrade {
        grade: "8.8",
        yield_mpa: 640.0,
        ultimate_mpa: 800.0,
        elongation_pct: 12.0,
        used_in_switzerland: true,
    },
    StandardGrade {
        grade: "10.9",
        yield_mpa: 900.0,
        ultimate_mpa: 1000.0,
        elongation_pct: 9.0,
        used_in_switzerland: true,
    },
];

/// What [`BoltGrade::set_by`] says for a grade the command set.
pub const SET_BY_COMMAND: &str = "command";

impl StandardGrade {
    /// The grade `name` of [`SIA263_GRADES`].
    pub fn sia263(name: &str) -> Option<Self> {
        SIA263_GRADES
            .iter()
            .find(|g| g.grade == name.trim())
            .copied()
    }

    /// The structure file's record of it, set by the command, its source
    /// named.
    pub fn record(&self) -> BoltGrade {
        BoltGrade {
            grade: self.grade.to_owned(),
            ultimate: Stress::from_si(self.ultimate_mpa * 1e6),
            yield_strength: Some(Stress::from_si(self.yield_mpa * 1e6)),
            source: Some(SIA263_GRADES_SOURCE.to_owned()),
            set_by: Some(SET_BY_COMMAND.to_owned()),
        }
    }
}

/// A bolt grade a structure's members' bolts take (`[[bolt_grade]]`,
/// structure file version 21, owner 2026-11-19): its strengths and where
/// they come from; a member names it (`[[member]] bolt_grade = "8.8"`).
/// Over a library bolt's and the member's own: its ultimate strength and
/// its name; their capacities kept unless given for another grade (then
/// dropped, said).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoltGrade {
    /// Its name, as the standard names it ("8.8"), unique in the file.
    pub grade: String,
    /// Ultimate tensile strength f_ub.
    pub ultimate: Stress,
    /// Yield strength f_yb (none: not given).
    #[serde(default, rename = "yield")]
    pub yield_strength: Option<Stress>,
    /// Where the values come from ("SIA 263:2013, 3.4.2, Table 2").
    #[serde(default)]
    pub source: Option<String>,
    /// Who set it: [`SET_BY_COMMAND`] for the bolt grade command (none:
    /// typed in the file).
    #[serde(default)]
    pub set_by: Option<String>,
}

impl BoltGrade {
    /// A name, its strengths above 0, the yield no higher than the
    /// ultimate. Err: what is not.
    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if self.grade.trim().is_empty() {
            return Err("bolt_grade: a grade name".into());
        }
        if !ok(self.ultimate.si()) {
            return Err(format!("bolt_grade {}: ultimate above 0", self.grade));
        }
        if let Some(y) = self.yield_strength
            && !(ok(y.si()) && y.si() <= self.ultimate.si())
        {
            return Err(format!(
                "bolt_grade {}: yield above 0 and no higher than the ultimate",
                self.grade
            ));
        }
        Ok(())
    }

    /// In words, for the results: "bolt grade 8.8 set by command: f_ub 800
    /// MPa, f_yb 640 MPa (SIA 263:2013, 3.4.2, Table 2)".
    pub fn words(&self) -> String {
        let by = match self.set_by.as_deref() {
            Some(SET_BY_COMMAND) => "set by command".to_owned(),
            Some(x) => format!("set by {x}"),
            None => "given in the file".to_owned(),
        };
        let mut s = format!(
            "bolt grade {} {by}: f_ub {:.0} MPa",
            self.grade,
            self.ultimate.si() / 1e6
        );
        if let Some(y) = self.yield_strength {
            s.push_str(&format!(", f_yb {:.0} MPa", y.si() / 1e6));
        }
        if let Some(src) = &self.source {
            s.push_str(&format!(" ({src})"));
        }
        s
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

    /// SIA 263:2013 Table 2's 8.8 as the command records it; a record
    /// checked.
    #[test]
    fn a_grade_is_recorded_with_its_source() {
        let g = StandardGrade::sia263("8.8").unwrap().record();
        assert!((g.ultimate.si() - 800e6).abs() < 1.0);
        assert!((g.yield_strength.unwrap().si() - 640e6).abs() < 1.0);
        assert_eq!(g.set_by.as_deref(), Some(SET_BY_COMMAND));
        assert_eq!(
            g.words(),
            "bolt grade 8.8 set by command: f_ub 800 MPa, f_yb 640 MPa (SIA 263:2013, 3.4.2, \
             Table 2)"
        );
        g.validate().unwrap();
        assert!(StandardGrade::sia263("12.9").is_none());
        let mut bad = g.clone();
        bad.yield_strength = Some(Stress::from_si(900e6));
        assert!(bad.validate().unwrap_err().contains("no higher"));
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
