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

use ols_core::units::{
    Area, Force, ForcePerLength, Length, MassPerLength, ResistancePerLength, Stress, Temperature,
};
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

impl CableKind {
    /// Temperature coefficient of resistance at 20 °C, 1/K, of the
    /// material that carries the current: 0.00403 for aluminium and its
    /// alloys (the aluminium-conductor kinds, and OPGW, whose outer wires
    /// are aluminium alloy or aluminium-clad), 0.0045 for steel wire;
    /// `None` where the material is unknown (`other`) or there is no
    /// conductor (`adss`).
    pub fn resistance_temperature_coefficient(self) -> Option<f64> {
        match self {
            CableKind::Acsr
            | CableKind::Acss
            | CableKind::Aac
            | CableKind::Aaac
            | CableKind::Acar
            | CableKind::Accc
            | CableKind::Opgw => Some(0.00403),
            CableKind::Steel => Some(0.0045),
            CableKind::Adss | CableKind::Other => None,
        }
    }
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
    /// Number of strands (wires) of this component, e.g. 26 for the
    /// aluminium of a 26/7 ACSR: their diameter follows from the area. Used
    /// for the fully slipped bending stiffness ([`Cable::bending_stiffness_min`]).
    #[serde(default)]
    pub strands: Option<u32>,
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
    /// Electrical and thermal data (optional).
    #[serde(default)]
    pub electrical: Option<Electrical>,
    /// How sag and tension are modelled (default: non-linear).
    #[serde(default)]
    pub model: CableModel,
    /// Linear models: the permanent stretch due to creep as an equivalent
    /// temperature increase, °C.
    #[serde(default)]
    pub creep_temperature_shift: Option<f64>,
    /// Linear model with creep proportional to tension: the creep weather
    /// case tension at which the stretch equals `creep_temperature_shift`.
    #[serde(default)]
    pub creep_shift_tension: Option<Force>,
    /// Bimetallic cables: what the outer strands do above the transition
    /// temperature. None: the line's choice (Criteria › Bimetallic
    /// conductor model, [`BimetallicModel`]), compression by default.
    #[serde(default)]
    pub bimetallic_behavior: Option<BimetallicBehavior>,
    /// `bimetallic_behavior = "compression"`: the largest virtual
    /// compressive stress of the outer strands (their actual stress × outer
    /// area / total area); default [`DEFAULT_MAX_VIRTUAL_COMPRESSION`].
    #[serde(default)]
    pub max_virtual_compression: Option<Stress>,
}

/// The bimetallic conductor model of a line: what the outer strands of its
/// bimetallic cables do above the transition temperature, for the cables
/// whose wire library entry does not say (as older cable files): bird cage
/// (no compression) or compression up to a largest virtual stress (actual
/// stress × outer area / total area).
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BimetallicModel {
    /// Bird cage or compression (default).
    #[serde(default)]
    pub behavior: BimetallicBehavior,
    /// Compression: the largest virtual compressive stress (default
    /// [`DEFAULT_MAX_VIRTUAL_COMPRESSION`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_virtual_compression: Option<Stress>,
}

impl BimetallicModel {
    /// Whether it is the default (compression, default stress).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `cable` as the line models it: its own behaviour when its wire
    /// library entry gives one, else this model's (borrowed when nothing
    /// changes).
    pub fn apply<'a>(&self, cable: &'a Cable) -> std::borrow::Cow<'a, Cable> {
        if cable.core.is_none() || cable.bimetallic_behavior.is_some() || self.is_default() {
            return std::borrow::Cow::Borrowed(cable);
        }
        let mut c = cable.clone();
        c.bimetallic_behavior = Some(self.behavior);
        if c.max_virtual_compression.is_none() {
            c.max_virtual_compression = self.max_virtual_compression;
        }
        std::borrow::Cow::Owned(c)
    }

    /// Checks the stress.
    pub fn validate(&self) -> Result<(), String> {
        if self
            .max_virtual_compression
            .is_some_and(|s| !(s.si().is_finite() && s.si() >= 0.0))
        {
            return Err("bimetallic model: the virtual compressive stress must be ≥ 0".into());
        }
        Ok(())
    }
}

/// What the outer strands of a bimetallic cable (e.g. aluminium over a
/// steel core) do above the transition temperature, where their larger
/// thermal expansion leaves them without tension: a property of the
/// conductor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BimetallicBehavior {
    /// The outer strands go into compression, up to the cable's
    /// `max_virtual_compression`.
    #[default]
    Compression,
    /// The outer strands take no compression (bird cage): the core
    /// carries the whole tension.
    BirdCage,
}

impl BimetallicBehavior {
    /// The name in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            BimetallicBehavior::Compression => "compression",
            BimetallicBehavior::BirdCage => "bird_cage",
        }
    }
}

/// Default largest virtual compressive stress of the outer strands, Pa:
/// 10⁷ psi (68 947.4 MPa), in practice no limit.
pub const DEFAULT_MAX_VIRTUAL_COMPRESSION: f64 = 68_947.4e6;

/// How a cable's sag and tension are modelled (the cable models of line
/// design programs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CableModel {
    /// Separate polynomials for the initial and creep behaviour of the
    /// outer and core materials ([`mechanics`]); a component without them
    /// is linear elastic, without creep.
    #[default]
    Nonlinear,
    /// Linear elastic; permanent stretch due to creep proportional to the
    /// creep weather case tension: the strain of `creep_temperature_shift`
    /// at `creep_shift_tension`, scaled by the tension.
    LinearCreepTension,
    /// Linear elastic; permanent stretch due to creep given as a temperature
    /// increase (`creep_temperature_shift`).
    LinearCreepTemperature,
}

impl CableModel {
    /// Its name in files and lists.
    pub fn as_str(self) -> &'static str {
        match self {
            CableModel::Nonlinear => "nonlinear",
            CableModel::LinearCreepTension => "linear_creep_tension",
            CableModel::LinearCreepTemperature => "linear_creep_temperature",
        }
    }
}

/// A resistance at a temperature.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResistanceAt {
    /// Conductor temperature.
    pub temperature: Temperature,
    /// Resistance per unit length.
    pub resistance: ResistancePerLength,
}

/// Electrical and thermal data of a cable (`[cable.electrical]`), linked to
/// the line's sections through their `cable`. The electrical systems
/// (voltages, phases) are part of the line, not of the cable.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Electrical {
    /// AC resistance at one or more temperatures (linear between them; one
    /// value is scaled with the material's temperature coefficient, see
    /// [`Electrical::ac_resistance_at`]).
    #[serde(default)]
    pub ac_resistance: Vec<ResistanceAt>,
    /// DC resistance at a temperature.
    #[serde(default)]
    pub dc_resistance: Option<ResistanceAt>,
    /// Geometric mean radius (for inductance).
    #[serde(default)]
    pub geometric_mean_radius: Option<Length>,
    /// Emissivity of the surface, 0–1 (thermal rating).
    #[serde(default)]
    pub emissivity: Option<f64>,
    /// Solar absorptivity of the surface, 0–1 (thermal rating).
    #[serde(default)]
    pub solar_absorptivity: Option<f64>,
}

impl Electrical {
    /// AC resistance at `t` of a cable of kind `kind`.
    ///
    /// - Two or more given temperatures: linear through the two nearest
    ///   (extrapolated beyond them along the same line).
    /// - One given temperature \\( T_1 \\): scaled with the temperature
    ///   coefficient \\( \alpha_{20} \\) of the kind's material
    ///   ([`CableKind::resistance_temperature_coefficient`]),
    ///   \\( R(T) = R(T_1) \frac{1 + \alpha_{20}(T - 20)}{1 + \alpha_{20}(T_1 - 20)} \\);
    ///   for a kind without a known coefficient only `t` = \\( T_1 \\)
    ///   has a value (`None` elsewhere).
    /// - No data: `None`.
    pub fn ac_resistance_at(&self, t: Temperature, kind: CableKind) -> Option<ResistancePerLength> {
        let mut pts: Vec<(f64, f64)> = self
            .ac_resistance
            .iter()
            .map(|r| (r.temperature.celsius(), r.resistance.si()))
            .collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let x = t.celsius();
        match pts.len() {
            0 => None,
            1 => {
                let (x1, y1) = pts[0];
                if x == x1 {
                    return Some(ResistancePerLength::from_si(y1));
                }
                let alpha = kind.resistance_temperature_coefficient()?;
                Some(ResistancePerLength::from_si(
                    y1 * (1.0 + alpha * (x - 20.0)) / (1.0 + alpha * (x1 - 20.0)),
                ))
            }
            n => {
                let k = pts.partition_point(|p| p.0 < x).clamp(1, n - 1);
                let ((x0, y0), (x1, y1)) = (pts[k - 1], pts[k]);
                Some(ResistancePerLength::from_si(
                    y0 + (y1 - y0) * (x - x0) / (x1 - x0),
                ))
            }
        }
    }

    fn validate(&self, diameter: Length) -> Result<(), String> {
        let mut temps: Vec<f64> = Vec::new();
        for r in self.ac_resistance.iter().chain(&self.dc_resistance) {
            if !(r.resistance.si().is_finite() && r.resistance.si() > 0.0) {
                return Err("resistances must be positive".into());
            }
        }
        for r in &self.ac_resistance {
            let t = r.temperature.celsius();
            if temps.iter().any(|x| (x - t).abs() < 1e-9) {
                return Err(format!("ac_resistance: temperature {t} °C given twice"));
            }
            temps.push(t);
        }
        if let Some(g) = self.geometric_mean_radius
            && !(g.si() > 0.0 && g.si() <= 0.5 * diameter.si())
        {
            return Err(format!(
                "geometric_mean_radius {:.3} mm must be positive and at most the radius",
                g.millimeters()
            ));
        }
        for (name, v) in [
            ("emissivity", self.emissivity),
            ("solar_absorptivity", self.solar_absorptivity),
        ] {
            if v.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                return Err(format!("{name} must be between 0 and 1"));
            }
        }
        Ok(())
    }
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

    /// Bending stiffness of the cable as a solid bar (strands locked
    /// together), N·m²: \\( EI_{max} = E \, \pi d^4 / 64 \\) with the
    /// composite modulus E and the overall diameter d.
    pub fn bending_stiffness_max(&self) -> f64 {
        let d = self.diameter.si();
        self.modulus().si() * std::f64::consts::PI * d.powi(4) / 64.0
    }

    /// Bending stiffness with the strands slipping on each other, each
    /// bending on its own, N·m²: \\( EI_{min} = \sum_i n_i E_i \, \pi d_i^4 / 64 \\)
    /// over the components, with \\( n_i \\) strands of diameter
    /// \\( d_i = \sqrt{4 A_i / (\pi n_i)} \\). None when a component's
    /// number of strands is not given.
    pub fn bending_stiffness_min(&self) -> Option<f64> {
        self.components()
            .map(|c| {
                let n = f64::from(c.strands?);
                let d2 = 4.0 * c.area.si() / (std::f64::consts::PI * n);
                Some(n * c.modulus.si() * std::f64::consts::PI * d2 * d2 / 64.0)
            })
            .sum()
    }

    /// Composite thermal expansion coefficient, 1/K, weighted by stiffness.
    pub fn thermal_expansion(&self) -> f64 {
        let weighted: f64 = self
            .components()
            .map(|c| c.axial_stiffness().si() * c.thermal_expansion)
            .sum();
        weighted / self.axial_stiffness().si()
    }

    /// Thermal strain of the cable without tension (a slack jumper), `dt`
    /// kelvin from the temperature where its components are unstressed. Both
    /// components share the strain and their forces balance:
    /// \\( \varepsilon = \bar\alpha\, \Delta t \\) with the composite
    /// expansion \\( \bar\alpha = \sum E_iA_i\alpha_i / \sum E_iA_i \\). Warmer,
    /// the outer strands of a bimetallic cable are pushed into compression;
    /// past their limit ([`Cable::outer_compression_limit_with_curves`], as the spans: none for a bird
    /// cage) they carry only that and the core sets the length:
    /// \\( \varepsilon = \alpha_c \Delta t + F_{lim} / E_cA_c \\).
    pub fn free_thermal_strain(&self, dt: f64) -> f64 {
        let composite = self.thermal_expansion() * dt;
        let Some(core) = &self.core else {
            return composite;
        };
        let outer_force =
            self.outer.axial_stiffness().si() * (composite - self.outer.thermal_expansion * dt);
        let limit = self.outer_compression_limit_with_curves();
        if outer_force >= -limit {
            composite
        } else {
            core.thermal_expansion * dt + limit / core.axial_stiffness().si()
        }
    }

    /// The largest compressive load of the outer strands, N, as the line
    /// sections use it: the bimetallic behaviour's limit, and the outer
    /// curves' own `max_compression` when lower (non-linear model).
    pub fn outer_compression_limit_with_curves(&self) -> f64 {
        mechanics::Mechanics::from_cable(self).map_or_else(
            |_| self.outer_compression_limit(),
            |m| m.outer_compression_limit(),
        )
    }

    /// The largest compressive load of the outer strands, N, as the
    /// cable's bimetallic behaviour says: unlimited without a core.
    pub fn outer_compression_limit(&self) -> f64 {
        if self.core.is_none() {
            return f64::INFINITY;
        }
        let virtual_stress = match self.bimetallic_behavior.unwrap_or_default() {
            BimetallicBehavior::BirdCage => return 0.0,
            BimetallicBehavior::Compression => self
                .max_virtual_compression
                .map_or(DEFAULT_MAX_VIRTUAL_COMPRESSION, |s| s.si()),
        };
        // Virtual stress is on the total area.
        virtual_stress.abs() * self.area().si()
    }

    /// Checks that the data is physically plausible. Catches typos and unit
    /// mistakes in library files (e.g. mm² entered as m²).
    pub fn validate(&self) -> Result<(), CableError> {
        let error = |message: String| CableError::Invalid {
            id: self.id.clone(),
            message,
        };
        match self.model {
            CableModel::Nonlinear => {}
            CableModel::LinearCreepTemperature if self.creep_temperature_shift.is_none() => {
                return Err(error(
                    "linear_creep_temperature needs creep_temperature_shift".into(),
                ));
            }
            CableModel::LinearCreepTension
                if self.creep_temperature_shift.is_none()
                    || self.creep_shift_tension.is_none_or(|t| t.si() <= 0.0) =>
            {
                return Err(error(
                    "linear_creep_tension needs creep_temperature_shift and a positive \
                     creep_shift_tension"
                        .into(),
                ));
            }
            _ => {}
        }
        if self
            .max_virtual_compression
            .is_some_and(|s| !(s.si().is_finite() && s.si() >= 0.0))
        {
            return Err(error("max_virtual_compression must be a stress ≥ 0".into()));
        }
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
            if c.strands == Some(0) {
                return Err(error(format!("{label}.strands must be at least 1")));
            }
            if label == "core" && c.strands.is_some() != self.outer.strands.is_some() {
                return Err(error(
                    "strands: give them for the outer strands and the core, or for neither".into(),
                ));
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
        if let Some(e) = &self.electrical {
            e.validate(self.diameter)
                .map_err(|m| error(format!("electrical: {m}")))?;
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

/// File extension of OpenOHL wire libraries (`*.ohlw`, "OHL wire").
///
/// The content is TOML. A dedicated extension (rather than reusing another
/// program's, e.g. another line design program's cable files) avoids confusing different formats.
pub const WIRE_FILE_EXTENSION: &str = "ohlw";

/// Value of the optional `format` header of wire libraries.
pub const WIRE_FILE_FORMAT: &str = "ohl-wire";

/// Latest wire library format version.
pub const WIRE_FILE_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    /// `"ohl-wire"` (optional header identifying the file).
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
        ols_core::files::check_header(
            file.format.as_deref(),
            file.version,
            WIRE_FILE_FORMAT,
            WIRE_FILE_VERSION,
            "wire library",
        )
        .map_err(library_error)?;
        for cable in file.cable {
            cable.validate()?;
            if self.cables.contains_key(&cable.id) {
                return Err(library_error(format!("duplicate cable id {}", cable.id)));
            }
            self.cables.insert(cable.id.clone(), cable);
        }
        Ok(())
    }

    /// Loads a wire library file, or every `*.ohlw` file of a directory.
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

    /// Loads every `*.ohlw` wire library in a directory (not recursive), in
    /// name order.
    pub fn load_dir(dir: &Path) -> Result<Self, CableError> {
        let io_error = |path: &Path, e: std::io::Error| CableError::Library {
            path: path.to_owned(),
            message: e.to_string(),
        };
        std::fs::read_dir(dir).map_err(|e| io_error(dir, e))?;
        let paths = ols_core::files::files_with_extension(dir, WIRE_FILE_EXTENSION);
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

    /// A slack cable's thermal strain uses the same outer compression limit
    /// as the spans: the curves' `max_compression = 0` makes it a bird cage
    /// when warmer (the core alone), whatever the bimetallic behaviour says.
    #[test]
    fn free_thermal_strain_takes_the_curves_compression_limit() {
        let text = r#"
            [[cable]]
            id = "nl"
            name = "non-linear, no outer compression (invented values)"
            kind = "acsr"
            diameter = "28.14 mm"
            mass_per_length = "1.628 kg/m"
            rated_tensile_strength = "138.8 kN"
            [cable.outer]
            area = "402.9 mm²"
            modulus = "55 GPa"
            thermal_expansion = 23.0e-6
            stress_strain = { stress_unit = "MPa", initial = [0.0, 300.0], final_modulus = 550.0, max_compression = 0.0 }
            [cable.core]
            area = "65.7 mm²"
            modulus = "190 GPa"
            thermal_expansion = 11.5e-6
            stress_strain = { stress_unit = "MPa", initial = [0.0, 1900.0], final_modulus = 1900.0 }
        "#;
        let mut library = Library::default();
        library.add_toml(Path::new("t.toml"), text).unwrap();
        let c = library.get("nl").unwrap();
        assert_eq!(c.outer_compression_limit_with_curves(), 0.0);
        assert!((c.free_thermal_strain(60.0) - 11.5e-6 * 60.0).abs() < 1e-15);
        // Colder: the composite expansion.
        assert!((c.free_thermal_strain(-30.0) - c.thermal_expansion() * -30.0).abs() < 1e-15);
        // The linear Drake (no curves): unlimited by default, composite.
        let d = drake();
        assert!((d.free_thermal_strain(60.0) - d.thermal_expansion() * 60.0).abs() < 1e-15);
    }

    /// Drake 26/7: as a solid bar E·πd⁴/64 with the composite modulus
    /// (~73.9 GPa); its strands slipping, Σ n·E·πdᵢ⁴/64 with 26 aluminium
    /// wires of 4.44 mm and 7 steel wires of 3.46 mm (from area and count).
    #[test]
    fn bending_stiffness_solid_and_slipped() {
        let mut c = drake();
        assert_close(c.modulus().si(), 73.93e9, 1e-3);
        assert_close(c.bending_stiffness_max(), 2275.5, 1e-3);
        assert_eq!(c.bending_stiffness_min(), None);
        c.outer.strands = Some(26);
        assert_eq!(
            c.bending_stiffness_min(),
            None,
            "the core's count is missing"
        );
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("or for neither")
        );
        c.core.as_mut().unwrap().strands = Some(7);
        assert_close(c.bending_stiffness_min().unwrap(), 36.65, 1e-3);
        c.outer.strands = Some(0);
        assert!(c.validate().is_err());
    }

    /// A local wire library (`OLS_WIRE_DIR`) read, each cable's bending
    /// stiffness limits printed (local checks only).
    #[test]
    #[ignore = "local data"]
    fn local_library_bending_stiffness() {
        let Ok(dir) = std::env::var("OLS_WIRE_DIR") else {
            return;
        };
        let library = Library::load(Path::new(&dir)).unwrap();
        for c in library.cables.values() {
            println!(
                "{}: EI_max {:.0} N·m², 0.2× {:.0}, EI_min {}",
                c.id,
                c.bending_stiffness_max(),
                0.2 * c.bending_stiffness_max(),
                c.bending_stiffness_min()
                    .map_or("—".into(), |m| format!("{m:.1} N·m²"))
            );
        }
    }

    #[test]
    fn single_ac_resistance_scales_with_the_material() {
        let r = |t: f64, ohm_per_km: f64| ResistanceAt {
            temperature: Temperature::from_celsius(t),
            resistance: ResistancePerLength::from_ohms_per_kilometer(ohm_per_km),
        };
        let one = Electrical {
            ac_resistance: vec![r(25.0, 0.07283)],
            ..Electrical::default()
        };
        let at = |e: &Electrical, c: f64, kind: CableKind| {
            e.ac_resistance_at(Temperature::from_celsius(c), kind)
                .map(|r| r.ohms_per_kilometer())
        };
        // ACSR: aluminium's coefficient.
        let expected = 0.07283 * (1.0 + 0.00403 * 55.0) / (1.0 + 0.00403 * 5.0);
        assert!((at(&one, 75.0, CableKind::Acsr).unwrap() - expected).abs() < 1e-12);
        assert!((at(&one, 25.0, CableKind::Acsr).unwrap() - 0.07283).abs() < 1e-12);
        // Steel wire: steel's coefficient.
        let steel = 0.07283 * (1.0 + 0.0045 * 55.0) / (1.0 + 0.0045 * 5.0);
        assert!((at(&one, 75.0, CableKind::Steel).unwrap() - steel).abs() < 1e-12);
        // OPGW: aluminium's coefficient.
        assert!((at(&one, 75.0, CableKind::Opgw).unwrap() - expected).abs() < 1e-12);
        // Unknown material: only the given temperature.
        for kind in [CableKind::Other, CableKind::Adss] {
            assert!(at(&one, 75.0, kind).is_none());
            assert!((at(&one, 25.0, kind).unwrap() - 0.07283).abs() < 1e-12);
        }
        // Two values: the straight line through them, whatever the kind.
        let two = Electrical {
            ac_resistance: vec![r(25.0, 0.07283), r(75.0, 0.08688)],
            ..Electrical::default()
        };
        for kind in [CableKind::Acsr, CableKind::Other] {
            assert!((at(&two, 50.0, kind).unwrap() - 0.079855).abs() < 1e-12);
            assert!((at(&two, 100.0, kind).unwrap() - 0.093905).abs() < 1e-12);
        }
    }

    #[test]
    fn electrical_data() {
        let text = |extra: &str| {
            format!(
                r#"
                [[cable]]
                id = "aaac-551"
                name = "AAAC 551 (invented test values)"
                kind = "aaac"
                diameter = "30.5 mm"
                mass_per_length = "1.52 kg/m"
                rated_tensile_strength = "162 kN"
                outer = {{ area = "550.6 mm²", modulus = "55 GPa", thermal_expansion = 2.3e-5 }}
                [cable.electrical]
                ac_resistance = [
                    {{ temperature = "20 °C", resistance = "0.0618 Ω/km" }},
                    {{ temperature = "40 °C", resistance = "0.0661 Ω/km" }},
                ]
                dc_resistance = {{ temperature = "20 °C", resistance = "0.0605 ohm/km" }}
                geometric_mean_radius = "11.779 mm"
                emissivity = 0.6
                {extra}
                "#
            )
        };
        let mut lib = Library::default();
        lib.add_toml(Path::new("t.ohlw"), &text("solar_absorptivity = 0.5"))
            .unwrap();
        let e = lib.get("aaac-551").unwrap().electrical.clone().unwrap();
        let at = |c: f64| {
            e.ac_resistance_at(Temperature::from_celsius(c), CableKind::Aaac)
                .unwrap()
                .ohms_per_kilometer()
        };
        assert!((at(30.0) - 0.06395).abs() < 1e-12);
        // Beyond the data: the same straight line.
        assert!((at(80.0) - (0.0661 + 2.0 * 0.0043)).abs() < 1e-12);
        assert_eq!(
            e.dc_resistance.unwrap().resistance.ohms_per_kilometer(),
            0.0605
        );
        assert!((e.geometric_mean_radius.unwrap().millimeters() - 11.779).abs() < 1e-9);
        for bad in ["solar_absorptivity = 1.5", "resistivity = 3"] {
            assert!(
                Library::default()
                    .add_toml(Path::new("t.ohlw"), &text(bad))
                    .is_err(),
                "{bad}"
            );
        }
        // A geometric mean radius larger than the cable: a unit mistake.
        let big = text("").replace("11.779 mm", "11.779 m");
        assert!(
            Library::default()
                .add_toml(Path::new("t.ohlw"), &big)
                .is_err()
        );
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
            std::fs::read_to_string(root.join("libraries/cables/TEMPLATE.ohlw.example")).unwrap();
        let mut library = Library::default();
        library
            .add_toml(Path::new("TEMPLATE.ohlw.example"), &template)
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
            .add_toml(Path::new("doc.ohlw"), code)
            .unwrap();
    }

    #[test]
    fn rejects_bad_ids_and_formats() {
        let ok = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables/acsr-examples.ohlw"),
        )
        .unwrap();
        let bad_id = ok.replace("id = \"acsr-drake-26-7\"", "id = \"acsr drake\"");
        assert!(
            Library::default()
                .add_toml(Path::new("a.ohlw"), &bad_id)
                .unwrap_err()
                .to_string()
                .contains("letters, digits")
        );
        let bad_format = ok.replace("format = \"ohl-wire\"", "format = \"something-else\"");
        assert!(
            Library::default()
                .add_toml(Path::new("b.ohlw"), &bad_format)
                .is_err()
        );
        let future = ok.replace("version = 1", "version = 99");
        assert!(
            Library::default()
                .add_toml(Path::new("c.ohlw"), &future)
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
