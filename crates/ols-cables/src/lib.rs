// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cables and conductors: properties, composite behaviour and libraries.
//!
//! A cable is made of an **outer** component (e.g. the aluminium layers of
//! an ACSR) and an optional **core** (e.g. the steel core). Each component
//! has its own area, elastic modulus and thermal expansion; the composite
//! behaviour follows from equal strain in both components:
//!
//! - axial stiffness: \\( EA = E_o A_o + E_c A_c \\)
//! - thermal expansion: \\( \alpha = (E_o A_o \alpha_o + E_c A_c \alpha_c) / EA \\)
//!
//! This is the linear-elastic model; components may also carry non-linear
//! stress-strain and creep curves (see [`mechanics`]).
//!
//! Libraries are TOML files where values carry their units:
//!
//! ```toml
//! [[cable]]
//! id = "acsr-drake-26-7"
//! name = "Drake 795 kcmil 26/7 ACSR"
//! kind = "acsr"
//! diameter = "1.108 in"
//! mass_per_length = "1.094 lbm/ft"  # 1094 lb per 1000 ft
//! rated_tensile_strength = "31200 lbf"
//! outer = { area = "0.6245 in²", modulus = "55 GPa", thermal_expansion = 23.0e-6 }
//! core = { area = "0.1019 in²", modulus = "190 GPa", thermal_expansion = 11.5e-6 }
//! ```

pub mod mechanics;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ols_core::units::{Area, Force, ForcePerLength, Length, MassPerLength, Stress};
use serde::Deserialize;

/// Family of cable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CableKind {
    /// Aluminium conductor, steel reinforced.
    Acsr,
    /// Aluminium conductor, steel supported (annealed aluminium).
    Acss,
    /// All-aluminium conductor.
    Aac,
    /// All-aluminium-alloy conductor.
    Aaac,
    /// Aluminium conductor, alloy reinforced.
    Acar,
    /// Aluminium conductor, composite core.
    Accc,
    /// Optical ground wire.
    Opgw,
    /// All-dielectric self-supporting optical cable.
    Adss,
    /// Steel wire (shield wire, guy).
    Steel,
    /// Anything else.
    Other,
}

/// One material component of a cable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    /// Cross-sectional area.
    pub area: Area,
    /// Elastic (final) modulus.
    pub modulus: Stress,
    /// Linear thermal expansion coefficient, 1/K.
    pub thermal_expansion: f64,
    /// Non-linear stress-strain and creep curves (none = linear with `modulus`).
    #[serde(default)]
    pub stress_strain: Option<mechanics::StressStrainData>,
}

impl Component {
    /// Axial stiffness \\( E A \\).
    pub fn axial_stiffness(&self) -> Force {
        self.modulus * self.area
    }
}

/// A cable or conductor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cable {
    /// Unique identifier within the libraries, e.g. `"acsr-drake-26-7"`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Family.
    pub kind: CableKind,
    /// Where the data comes from (data sheet, standard, …).
    #[serde(default)]
    pub source: Option<String>,
    /// Overall diameter.
    pub diameter: Length,
    /// Mass per unit length.
    pub mass_per_length: MassPerLength,
    /// Rated tensile strength (RTS / RBS / UTS).
    pub rated_tensile_strength: Force,
    /// Outer component (whole cable for homogeneous cables).
    pub outer: Component,
    /// Core component, for bimetallic and composite cables.
    #[serde(default)]
    pub core: Option<Component>,
}

impl Cable {
    fn components(&self) -> impl Iterator<Item = &Component> {
        std::iter::once(&self.outer).chain(self.core.as_ref())
    }

    /// Total cross-sectional area.
    pub fn area(&self) -> Area {
        self.components().map(|c| c.area).sum()
    }

    /// Weight per unit length under standard gravity.
    pub fn weight(&self) -> ForcePerLength {
        self.mass_per_length.weight()
    }

    /// Composite axial stiffness \\( EA = \sum E_i A_i \\).
    pub fn axial_stiffness(&self) -> Force {
        self.components().map(Component::axial_stiffness).sum()
    }

    /// Composite elastic modulus \\( EA / A \\).
    pub fn modulus(&self) -> Stress {
        Stress::from_si(self.axial_stiffness().si() / self.area().si())
    }

    /// Composite thermal expansion coefficient, 1/K, weighted by stiffness.
    pub fn thermal_expansion(&self) -> f64 {
        let weighted: f64 = self
            .components()
            .map(|c| c.axial_stiffness().si() * c.thermal_expansion)
            .sum();
        weighted / self.axial_stiffness().si()
    }

    /// Checks that the data is physically plausible. Catches typos and unit
    /// mistakes in library files (e.g. mm² entered as m²).
    pub fn validate(&self) -> Result<(), CableError> {
        let error = |message: String| CableError::Invalid {
            id: self.id.clone(),
            message,
        };
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(error(format!(
                "id \"{}\" must use only letters, digits, - and _",
                self.id
            )));
        }
        let positive = [
            ("diameter", self.diameter.si()),
            ("mass_per_length", self.mass_per_length.si()),
            ("rated_tensile_strength", self.rated_tensile_strength.si()),
        ];
        for (name, value) in positive {
            if !(value.is_finite() && value > 0.0) {
                return Err(error(format!("{name} must be positive, got {value}")));
            }
        }
        for (label, c) in [("outer", Some(&self.outer)), ("core", self.core.as_ref())] {
            let Some(c) = c else { continue };
            if !(c.area.si().is_finite() && c.area.si() > 0.0) {
                return Err(error(format!("{label}.area must be positive")));
            }
            let gpa = c.modulus.gigapascals();
            if !(1.0..=500.0).contains(&gpa) {
                return Err(error(format!(
                    "{label}.modulus = {gpa} GPa is outside 1–500 GPa (unit mistake?)"
                )));
            }
            if !(0.0..=50e-6).contains(&c.thermal_expansion) {
                return Err(error(format!(
                    "{label}.thermal_expansion = {} 1/K is outside 0–50e-6 (unit mistake?)",
                    c.thermal_expansion
                )));
            }
        }
        let circle = std::f64::consts::FRAC_PI_4 * self.diameter.si() * self.diameter.si();
        if self.area().si() > circle {
            return Err(error(format!(
                "area {:.1} mm² exceeds the area of the {:.2} mm diameter circle ({:.1} mm²)",
                self.area().square_millimeters(),
                self.diameter.millimeters(),
                circle * 1e6
            )));
        }
        mechanics::Mechanics::from_cable(self)?;
        Ok(())
    }
}

/// Errors from cable data and libraries.
#[derive(Debug, Clone, PartialEq)]
pub enum CableError {
    /// A cable's data is not plausible.
    Invalid {
        /// Cable id.
        id: String,
        /// What is wrong.
        message: String,
    },
    /// A library file could not be read or parsed.
    Library {
        /// File concerned.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
}

impl fmt::Display for CableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { id, message } => write!(f, "cable {id}: {message}"),
            Self::Library { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for CableError {}

/// File extension of OLS CAD wire libraries (`*.olsw`, "OLS wire").
///
/// The content is TOML. A dedicated extension (rather than reusing another
/// program's, e.g. PLS-CADD's `.wir`) avoids confusing different formats.
pub const WIRE_FILE_EXTENSION: &str = "olsw";

/// Value of the optional `format` header of wire libraries.
pub const WIRE_FILE_FORMAT: &str = "ols-wire";

/// Latest wire library format version.
pub const WIRE_FILE_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    /// `"ols-wire"` (optional header identifying the file).
    #[serde(default)]
    format: Option<String>,
    /// Format version (optional; 1).
    #[serde(default)]
    version: Option<u32>,
    cable: Vec<Cable>,
}

/// A set of cables loaded from library files, indexed by id.
#[derive(Debug, Clone, Default)]
pub struct Library {
    cables: BTreeMap<String, Cable>,
}

impl Library {
    /// Parses one library document and adds its cables. Every cable is
    /// validated; duplicate ids are rejected.
    pub fn add_toml(&mut self, path: &Path, text: &str) -> Result<(), CableError> {
        let library_error = |message: String| CableError::Library {
            path: path.to_owned(),
            message,
        };
        let file: LibraryFile = toml::from_str(text).map_err(|e| library_error(e.to_string()))?;
        if let Some(format) = file.format.as_deref().filter(|f| *f != WIRE_FILE_FORMAT) {
            return Err(library_error(format!(
                "format \"{format}\" is not \"{WIRE_FILE_FORMAT}\""
            )));
        }
        if let Some(version) = file.version.filter(|v| *v > WIRE_FILE_VERSION) {
            return Err(library_error(format!(
                "wire library version {version} is newer than supported ({WIRE_FILE_VERSION}); update OLS CAD"
            )));
        }
        for cable in file.cable {
            cable.validate()?;
            if self.cables.contains_key(&cable.id) {
                return Err(library_error(format!("duplicate cable id {}", cable.id)));
            }
            self.cables.insert(cable.id.clone(), cable);
        }
        Ok(())
    }

    /// Loads a wire library file, or every `*.olsw` file of a directory.
    pub fn load(path: &Path) -> Result<Self, CableError> {
        if path.is_dir() {
            return Self::load_dir(path);
        }
        let text = std::fs::read_to_string(path).map_err(|e| CableError::Library {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
        let mut library = Self::default();
        library.add_toml(path, &text)?;
        Ok(library)
    }

    /// Loads every `*.olsw` wire library in a directory (not recursive), in
    /// name order.
    pub fn load_dir(dir: &Path) -> Result<Self, CableError> {
        let io_error = |path: &Path, e: std::io::Error| CableError::Library {
            path: path.to_owned(),
            message: e.to_string(),
        };
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| io_error(dir, e))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == WIRE_FILE_EXTENSION))
            .collect();
        paths.sort();
        let mut library = Self::default();
        for path in paths {
            let text = std::fs::read_to_string(&path).map_err(|e| io_error(&path, e))?;
            library.add_toml(&path, &text)?;
        }
        Ok(library)
    }

    /// Looks up a cable by id.
    pub fn get(&self, id: &str) -> Option<&Cable> {
        self.cables.get(id)
    }

    /// All cables, sorted by id.
    pub fn iter(&self) -> impl Iterator<Item = &Cable> {
        self.cables.values()
    }

    /// Number of cables.
    pub fn len(&self) -> usize {
        self.cables.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.cables.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, rel_tol: f64) {
        assert!(
            ((actual - expected) / expected).abs() <= rel_tol,
            "actual {actual}, expected {expected}"
        );
    }

    fn drake() -> Cable {
        let text = r#"
            [[cable]]
            id = "drake"
            name = "Drake"
            kind = "acsr"
            diameter = "28.14 mm"
            mass_per_length = "1.628 kg/m"
            rated_tensile_strength = "138.8 kN"
            outer = { area = "402.9 mm²", modulus = "55 GPa", thermal_expansion = 23.0e-6 }
            core = { area = "65.7 mm²", modulus = "190 GPa", thermal_expansion = 11.5e-6 }
        "#;
        let mut library = Library::default();
        library.add_toml(Path::new("t.toml"), text).unwrap();
        library.get("drake").unwrap().clone()
    }

    #[test]
    fn composite_properties() {
        let c = drake();
        assert_close(c.area().square_millimeters(), 468.6, 1e-12);
        // EA = 55e9·402.9e-6 + 190e9·65.7e-6 = 22.1595e6 + 12.483e6 N
        assert_close(c.axial_stiffness().newtons(), 34.6425e6, 1e-12);
        assert_close(c.modulus().gigapascals(), 34.6425e6 / 468.6e-6 / 1e9, 1e-12);
        // α = (22.1595·23.0 + 12.483·11.5) / 34.6425 · 1e-6
        assert_close(
            c.thermal_expansion(),
            (22.1595 * 23.0 + 12.483 * 11.5) / 34.6425 * 1e-6,
            1e-12,
        );
        assert_close(c.weight().newtons_per_meter(), 1.628 * 9.806_65, 1e-12);
    }

    #[test]
    fn homogeneous_cable_uses_outer_only() {
        let mut c = drake();
        c.core = None;
        assert_close(c.modulus().gigapascals(), 55.0, 1e-12);
        assert_close(c.thermal_expansion(), 23.0e-6, 1e-12);
    }

    #[test]
    fn validation_catches_unit_mistakes() {
        let mut c = drake();
        c.outer.modulus = Stress::from_megapascals(55.0); // MPa instead of GPa
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("unit mistake")
        );

        let mut c = drake();
        c.outer.thermal_expansion = 23.0; // forgot the e-6
        assert!(c.validate().is_err());

        let mut c = drake();
        c.outer.area = Area::from_square_meters(402.9e-3); // 1000x too large
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("exceeds the area")
        );
    }

    #[test]
    fn rejects_duplicates_and_unknown_fields() {
        let one = r#"
            [[cable]]
            id = "x"
            name = "x"
            kind = "aac"
            diameter = "10 mm"
            mass_per_length = "0.2 kg/m"
            rated_tensile_strength = "10 kN"
            outer = { area = "50 mm²", modulus = "60 GPa", thermal_expansion = 23e-6 }
        "#;
        let mut library = Library::default();
        library.add_toml(Path::new("a.toml"), one).unwrap();
        assert!(library.add_toml(Path::new("b.toml"), one).is_err());
        let unknown = one.replace("kind = \"aac\"", "kind = \"aac\"\nunknown = 1");
        assert!(
            Library::default()
                .add_toml(Path::new("c.toml"), &unknown)
                .is_err()
        );
    }

    #[test]
    fn template_and_documented_example_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template =
            std::fs::read_to_string(root.join("libraries/cables/TEMPLATE.olsw.example")).unwrap();
        let mut library = Library::default();
        library
            .add_toml(Path::new("TEMPLATE.olsw.example"), &template)
            .unwrap();
        let cable = library.get("acsr-example-26-7").unwrap();
        assert!(cable.outer.stress_strain.is_some());
        // The minimal example of docs/user-guide/wire-library.md.
        let doc = std::fs::read_to_string(root.join("docs/user-guide/wire-library.md")).unwrap();
        let start = doc.find("## Minimal example").unwrap();
        let block = &doc[start..];
        let code = &block[block.find("```toml\n").unwrap() + 8..];
        let code = &code[..code.find("```").unwrap()];
        Library::default()
            .add_toml(Path::new("doc.olsw"), code)
            .unwrap();
    }

    #[test]
    fn rejects_bad_ids_and_formats() {
        let ok = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables/acsr-examples.olsw"),
        )
        .unwrap();
        let bad_id = ok.replace("id = \"acsr-drake-26-7\"", "id = \"acsr drake\"");
        assert!(
            Library::default()
                .add_toml(Path::new("a.olsw"), &bad_id)
                .unwrap_err()
                .to_string()
                .contains("letters, digits")
        );
        let bad_format = ok.replace("format = \"ols-wire\"", "format = \"something-else\"");
        assert!(
            Library::default()
                .add_toml(Path::new("b.olsw"), &bad_format)
                .is_err()
        );
        let future = ok.replace("version = 1", "version = 99");
        assert!(
            Library::default()
                .add_toml(Path::new("c.olsw"), &future)
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
    }

    #[test]
    fn repository_libraries_are_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables");
        let library = Library::load_dir(&dir).unwrap();
        assert!(!library.is_empty());
        for cable in library.iter() {
            assert!(cable.source.is_some(), "cable {} has no source", cable.id);
        }
    }
}
