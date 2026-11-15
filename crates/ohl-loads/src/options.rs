// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The load case options of file version 4 (owner 2026-11-03): removed
//! conductors, the structure wind's direction, ice and snow on the
//! structures.

use serde::Deserialize;

/// A load case's **removed conductor** (`[[case.removed]]`, version 4;
/// owner 2026-11-03: "a removed conductor option and broken is only used
/// for actually broke cables"): the wire of section `section` is not
/// there in that case — a construction state, e.g. a tension mast with
/// some phases not strung yet. Every span of the section is left out of
/// the solve: its loads on every structure are 0, the other sections
/// solve as usual. `set`, when given, is a check that the section is the
/// wire meant: one of its points must be of that set.
///
/// ```toml
/// [[case.removed]]
/// section = "S3"
/// set = 2        # optional
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovedWire {
    /// The section, by id.
    pub section: String,
    /// The set of the wire (a check; none: not checked).
    #[serde(default)]
    pub set: Option<u32>,
}

impl RemovedWire {
    /// In words: `S3 set 2 removed`.
    pub fn describe(&self) -> String {
        let set = self.set.map_or(String::new(), |s| format!(" set {s}"));
        format!("{}{set} removed", self.section)
    }

    /// Checks it against `line`: the section there, of the set given at
    /// one of its points.
    pub fn check(&self, line: &ohl_line::Line) -> Result<(), String> {
        let s = line
            .sections
            .iter()
            .find(|s| s.id == self.section)
            .ok_or_else(|| format!("removed wire: no section {}", self.section))?;
        if let Some(set) = self.set
            && !s.points.iter().any(|p| p.set == set)
        {
            return Err(format!(
                "removed wire: section {} has no point of set {set}",
                self.section
            ));
        }
        Ok(())
    }
}

/// Where the wind comes from (one type for the program, in its
/// convention since 2026-11-15: from; 0° from ahead, 90° from the right,
/// clockwise — [`ohl_weather::WindDirection`]). `[case.structure]
/// direction` is in the structure's axes (x to the right of the line, y
/// ahead); an angle read from a load case file up to version 6 (0° from
/// the right, counted towards ahead) is kept as read
/// ([`WindDirection::OldAngle`]), its forces the old ones bit for bit.
pub use ohl_weather::WindDirection;

/// The ice on the structure bodies of a load case (`[case.structure]
/// ice`, version 4): `true` — each structure's local ice
/// ([`ohl_line::local::LocalConditions::structure_ice`]; none where it
/// gives none) —, a thickness for every structure, or `false` (also
/// written `"none"`, 2026-11-04): no ice on the structures, even where the
/// case takes the local conditions. Not given: none, except that a case
/// taking the local conditions whose weather case has ice puts each
/// structure's local ice on it (owner 2026-11-04: "yes apply local ice
/// too"; [`crate::LoadCase::structure_ice_at`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StructureIce {
    /// Each structure's local `structure_ice`.
    Local,
    /// This radial thickness on every structure, m.
    Thickness(f64),
    /// No ice on the structures (`ice = false` or `ice = "none"`), the
    /// local ice not taken either.
    NoIce,
    /// The weather case's ice on the wires, its radial thickness on every
    /// structure (`ice = "weather"`, version 6; owner 2026-11-15: "a factor
    /// as for the wind to change the value compared to the conductor
    /// weather cases", with [`StructureOptions::ice_factor`]).
    Weather,
}

impl<'de> Deserialize<'de> for StructureIce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Text(String),
            Thickness(ohl_core::units::Length),
        }
        match Raw::deserialize(d)? {
            Raw::Flag(true) => Ok(Self::Local),
            Raw::Flag(false) => Ok(Self::NoIce),
            Raw::Text(t) if t.trim() == "none" => Ok(Self::NoIce),
            Raw::Text(t) if t.trim() == "weather" => Ok(Self::Weather),
            Raw::Text(t) => t
                .parse::<ohl_core::units::Length>()
                .map(|l| Self::Thickness(ohl_core::units::Quantity::si(l)))
                .map_err(|_| {
                    serde::de::Error::custom(format!(
                        "ice \"{t}\": true (the structures' local ice), \"weather\" (the weather \
                         case's ice on the wires), a thickness, or false (\"none\": no ice on \
                         the structures)"
                    ))
                }),
            Raw::Thickness(t) => Ok(Self::Thickness(ohl_core::units::Quantity::si(t))),
        }
    }
}

/// SIA 261:2014 5.3.4: the shape coefficient μ where the snow cannot
/// slide off (read from the standard by the owner's session, 2026-11-03),
/// taken for a structure's horizontal surfaces (an assumption for a
/// pylon, said in the theory page).
pub const DEFAULT_SNOW_SHAPE: f64 = 0.8;

/// SIA 261:2014 5.2.4 and 5.2.5: the exposure coefficient C_e for normal
/// exposure and the thermal coefficient C_T, both 1.0.
pub const DEFAULT_SNOW_EXPOSURE: f64 = 1.0;

/// See [`DEFAULT_SNOW_EXPOSURE`].
pub const DEFAULT_SNOW_THERMAL: f64 = 1.0;

/// The snow load on a horizontal surface by SIA 261:2014 (9),
/// q_k = μ · C_e · C_T · s_k, N/m² (s_k the snow on the ground, N/m²).
pub fn snow_load(s_k: f64, shape: f64, exposure: f64, thermal: f64) -> f64 {
    shape * exposure * thermal * s_k
}

/// A load case's version-4 values for the structure bodies (the
/// `[case.structure]` table beside `wind_pressure` and `wind_factor`,
/// which stay in [`crate::StructureWind`]).
///
/// ```toml
/// [case.structure]
/// direction = 0           # "left", "right", "ahead", "back" or degrees
///                         # from (version 7: 0 from ahead, 90 from the
///                         # right; up to 6: 0 from the right, 90 from
///                         # ahead)
/// ice = true              # the structures' local ice; or "20 mm"; or
///                         # false: none (not even the local ice)
///                         # or "weather": the weather case's ice on
///                         # the wires (version 6)
/// ice_factor = 0.5        # on the ice thickness, whichever (version 6)
/// ice_density = "900 kg/m³"   # default: the weather case's
/// snow = true             # the structures' snow on the ground
/// snow_shape = 0.8        # μ (default 0.8, SIA 261 5.3.4)
/// snow_exposure = 1.0     # C_e (default 1.0, SIA 261 5.2.4)
/// snow_thermal = 1.0      # C_T (default 1.0, SIA 261 5.2.5)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StructureOptions {
    /// Where the wind on the bodies comes from (none: the direction the
    /// wires take in the case — the case's `wind_direction`, else the
    /// weather case's at an angle, else across from the side,
    /// [`crate::LoadCase::wind_from`] or the weather case's;
    /// [`crate::LoadCase::structure_wind`]).
    pub direction: Option<WindDirection>,
    /// Ice on the structures (none: no ice on them, except each
    /// structure's local ice where the case takes the local conditions and
    /// its weather case has ice, [`crate::LoadCase::structure_ice_at`]).
    pub ice: Option<StructureIce>,
    /// A factor on the ice's thickness on the structures, whichever it is
    /// — the weather case's, the local, the one given (version 6; owner
    /// 2026-11-15; none: 1). E.g. 0.5 with `ice = "weather"`: half the
    /// wires' thickness on the bodies.
    pub ice_factor: Option<f64>,
    /// The ice's density, kg/m³ (none: the weather case's; refused when
    /// it gives none and ice is put on a structure — except with `ice =
    /// "weather"`, which takes the wires' ice, glaze ice where the weather
    /// case gives no density, as on the wires).
    pub ice_density: Option<f64>,
    /// Snow on the structures' horizontal surfaces, from their snow on
    /// the ground ([`ohl_line::local::LocalConditions::snow`]).
    pub snow: bool,
    /// μ (none: [`DEFAULT_SNOW_SHAPE`]).
    pub snow_shape: Option<f64>,
    /// C_e (none: [`DEFAULT_SNOW_EXPOSURE`]).
    pub snow_exposure: Option<f64>,
    /// C_T (none: [`DEFAULT_SNOW_THERMAL`]).
    pub snow_thermal: Option<f64>,
}

impl StructureOptions {
    /// Nothing given.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// μ · C_e · C_T, the defaults where not given.
    pub fn snow_factor(&self) -> f64 {
        self.snow_shape.unwrap_or(DEFAULT_SNOW_SHAPE)
            * self.snow_exposure.unwrap_or(DEFAULT_SNOW_EXPOSURE)
            * self.snow_thermal.unwrap_or(DEFAULT_SNOW_THERMAL)
    }

    /// Checks the values: a direction a number, the ice 0 or more, the
    /// density above 0, the snow coefficients 0 or more and only with
    /// `snow`, the density not with `ice = false` (without `ice` it is
    /// the density of the local ice a case taking the local conditions
    /// puts on the structures).
    pub fn check(&self) -> Result<(), String> {
        if let Some(WindDirection::Angle(a) | WindDirection::OldAngle(a)) = self.direction
            && !a.is_finite()
        {
            return Err(format!("structure direction {a}°: a number"));
        }
        if let Some(StructureIce::Thickness(t)) = self.ice
            && !(t.is_finite() && t >= 0.0)
        {
            return Err(format!("structure ice {t} m (0 or more)"));
        }
        if let Some(f) = self.ice_factor {
            if self.ice == Some(StructureIce::NoIce) {
                return Err("structure ice_factor with ice = false".into());
            }
            if !(f.is_finite() && (0.0..=100.0).contains(&f)) {
                return Err(format!("structure ice_factor {f} (0 to 100)"));
            }
        }
        if let Some(r) = self.ice_density {
            if self.ice == Some(StructureIce::NoIce) {
                return Err("structure ice_density with ice = false".into());
            }
            if !(r.is_finite() && r > 0.0) {
                return Err(format!("structure ice_density {r} kg/m³ (above 0)"));
            }
        }
        for (name, v) in [
            ("snow_shape", self.snow_shape),
            ("snow_exposure", self.snow_exposure),
            ("snow_thermal", self.snow_thermal),
        ] {
            if let Some(v) = v {
                if !self.snow {
                    return Err(format!("structure {name} without snow = true"));
                }
                if !(v.is_finite() && v >= 0.0) {
                    return Err(format!("structure {name} {v} (0 or more)"));
                }
            }
        }
        Ok(())
    }
}

/// The ice on one structure in a load case: radial thickness t, m, and
/// density ρ, kg/m³.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IceOn {
    /// Radial thickness, m.
    pub thickness: f64,
    /// Density, kg/m³.
    pub density: f64,
}

impl IceOn {
    /// Its weight per metre on an outline of perimeter `p` (m):
    /// ρ·g·(P·t + π·t²), the ice all round the outline (an angle's inside faces
    /// too) — the rule of OHL-Structure's ice on members (owner
    /// 2026-10-25, conservative).
    pub fn weight_per_metre(&self, perimeter: f64) -> f64 {
        let t = self.thickness;
        self.density
            * ohl_core::units::STANDARD_GRAVITY
            * (perimeter * t + std::f64::consts::PI * t * t)
    }
}

/// The snow on one structure in a load case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnowOn {
    /// The snow on the ground s_k, N/m².
    pub ground: f64,
    /// μ · C_e · C_T.
    pub factor: f64,
}

impl SnowOn {
    /// q_k = μ C_e C_T s_k, N/m² ([`snow_load`]).
    pub fn load(&self) -> f64 {
        self.factor * self.ground
    }
}
