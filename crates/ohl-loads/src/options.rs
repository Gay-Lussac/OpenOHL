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

/// Where the wind on the structure bodies comes from (`[case.structure]
/// direction`, version 4), looking ahead along the line: `"right"`,
/// `"left"`, `"ahead"`, `"back"`, or an angle in degrees.
///
/// **Convention** (the structure's axes: +x to the right of the line,
/// +y ahead, z up): the angle θ is the direction the wind comes **from**,
/// 0° from the right, counted towards ahead — 90° from ahead, 180° from
/// the left, 270° from behind. The wind blows towards
/// (−cos θ, −sin θ). The names are those of OHL-Structure's wind on the
/// body (`WindFrom`): right 0°, ahead 90°, left 180°, back 270°.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindDirection {
    /// From the right (blowing to −x), 0°.
    Right,
    /// From ahead (blowing to −y), 90°.
    Ahead,
    /// From the left (blowing to +x), 180°.
    Left,
    /// From behind (blowing to +y), 270°.
    Back,
    /// From the angle given, degrees (0 from the right, towards ahead).
    Angle(f64),
}

impl WindDirection {
    /// The angle the wind comes from, degrees (0 from the right, 90 from
    /// ahead).
    pub fn degrees(self) -> f64 {
        match self {
            Self::Right => 0.0,
            Self::Ahead => 90.0,
            Self::Left => 180.0,
            Self::Back => 270.0,
            Self::Angle(a) => a,
        }
    }

    /// The direction a wind from the weather case's side gives.
    pub fn of_side(side: ohl_weather::WindSide) -> Self {
        match side {
            ohl_weather::WindSide::Right => Self::Right,
            ohl_weather::WindSide::Left => Self::Left,
        }
    }

    /// Where it blows to, a unit vector (x right of the line, y ahead):
    /// exact for the four names.
    pub fn towards(self) -> [f64; 2] {
        match self {
            Self::Right => [-1.0, 0.0],
            Self::Ahead => [0.0, -1.0],
            Self::Left => [1.0, 0.0],
            Self::Back => [0.0, 1.0],
            Self::Angle(a) => {
                let r = a.to_radians();
                [-r.cos(), -r.sin()]
            }
        }
    }

    /// As written in files: the name, or the angle.
    pub fn text(self) -> String {
        match self {
            Self::Right => "\"right\"".into(),
            Self::Ahead => "\"ahead\"".into(),
            Self::Left => "\"left\"".into(),
            Self::Back => "\"back\"".into(),
            Self::Angle(a) => format!("{a:?}"),
        }
    }

    /// In words: `from the right`, `from 30° (0° from the right, towards
    /// ahead)`.
    pub fn words(self) -> String {
        match self {
            Self::Right => "from the right".into(),
            Self::Ahead => "from ahead".into(),
            Self::Left => "from the left".into(),
            Self::Back => "from behind".into(),
            Self::Angle(a) => format!("from {a}° (0° from the right, 90° from ahead)"),
        }
    }
}

impl<'de> Deserialize<'de> for WindDirection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(f64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Number(a) if a.is_finite() => Ok(Self::Angle(a)),
            Raw::Number(a) => Err(serde::de::Error::custom(format!(
                "direction {a}: an angle in degrees"
            ))),
            Raw::Text(t) => match t.as_str() {
                "right" => Ok(Self::Right),
                "ahead" => Ok(Self::Ahead),
                "left" => Ok(Self::Left),
                "back" => Ok(Self::Back),
                other => other
                    .parse::<ohl_core::units::Angle>()
                    .map(|a| Self::Angle(a.degrees()))
                    .map_err(|_| {
                        serde::de::Error::custom(format!(
                            "direction \"{other}\": \"left\", \"right\", \"ahead\", \"back\" or \
                             an angle in degrees (0 from the right, 90 from ahead)"
                        ))
                    }),
            },
        }
    }
}

/// The ice on the structure bodies of a load case (`[case.structure]
/// ice`, version 4): `true` — each structure's local ice
/// ([`ohl_line::local::LocalConditions::structure_ice`]; none where it
/// gives none) — or a thickness for every structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StructureIce {
    /// Each structure's local `structure_ice`.
    Local,
    /// This radial thickness on every structure, m.
    Thickness(f64),
}

impl<'de> Deserialize<'de> for StructureIce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Thickness(ohl_core::units::Length),
        }
        match Raw::deserialize(d)? {
            Raw::Flag(true) => Ok(Self::Local),
            Raw::Flag(false) => Err(serde::de::Error::custom(
                "ice = false: leave ice out instead (true: the structures' local ice, or a \
                 thickness)",
            )),
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
/// direction = 90          # "left", "right", "ahead", "back" or degrees
/// ice = true              # the structures' local ice; or "20 mm"
/// ice_density = "900 kg/m³"   # default: the weather case's
/// snow = true             # the structures' snow on the ground
/// snow_shape = 0.8        # μ (default 0.8, SIA 261 5.3.4)
/// snow_exposure = 1.0     # C_e (default 1.0, SIA 261 5.2.4)
/// snow_thermal = 1.0      # C_T (default 1.0, SIA 261 5.2.5)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StructureOptions {
    /// Where the wind on the bodies comes from (none: the side of the
    /// wires' wind, [`crate::LoadCase::wind_from`] or the weather case's).
    pub direction: Option<WindDirection>,
    /// Ice on the structures (none: no ice on them).
    pub ice: Option<StructureIce>,
    /// The ice's density, kg/m³ (none: the weather case's; refused when
    /// it gives none).
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
    /// `snow`, the density only with ice.
    pub fn check(&self) -> Result<(), String> {
        if let Some(WindDirection::Angle(a)) = self.direction
            && !a.is_finite()
        {
            return Err(format!("structure direction {a}°: a number"));
        }
        if let Some(StructureIce::Thickness(t)) = self.ice
            && !(t.is_finite() && t >= 0.0)
        {
            return Err(format!("structure ice {t} m (0 or more)"));
        }
        if let Some(r) = self.ice_density {
            if self.ice.is_none() {
                return Err("structure ice_density without ice".into());
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
