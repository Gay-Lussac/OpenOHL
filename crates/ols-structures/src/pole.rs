// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The pole of a pole structure (decision 2 of *Building structures*):
//! named (`pole = "id"`, a `.olsp` file next to the structure, as before)
//! or described in the structure file itself, a `[pole]` table (format
//! version 3). The table has the same three levels as any structure:
//!
//! - **level 1**: no outline (the attachment points only);
//! - **level 2**: the outside lines — diameters (or circumferences) at two
//!   heights at least, and the cross-section shape (round, or a regular
//!   polygon with its number of sides);
//! - **level 3**: the engineering the pole check needs — the material, the
//!   strength (fibre stress, moment capacity or a capacity table), and
//!   its wall thickness (typed, `"solid"`, or a section of the section
//!   library).
//!
//! ```toml
//! [pole]
//! shape = "polygon"         # or "round" (default)
//! sides = 12                # polygon: the number of sides (3 or more)
//! embedment = "3 m"         # depth in the ground (the length; optional)
//!
//! [[pole.outline]]          # from the feet (the ground line) up
//! z = "0 m"                 # local z of the structure
//! diameter = "700 mm"       # or circumference; polygon: across flats
//!
//! [[pole.outline]]
//! z = "27 m"
//! diameter = "300 mm"
//!
//! # Level 3 (the fields of a .olsp file, same names and meaning):
//! material = "steel"        # wood | steel | concrete | other
//! wall = "8 mm"             # or "solid", or section = "<id>" (a polygonal
//!                           # or circular hollow section of the library)
//! elasticity = "210 GPa"
//! density = "7850 kg/m3"
//! drag = 0.8
//! # fibre_stress = "55 MPa" / moment_capacity = "900 kN m" /
//! [[pole.capacity]]
//! height = "0 m"            # above the ground line
//! moment = "900 kN m"
//! ```
//!
//! The pole check (`ols_structures_fe::pole`) reads the table and a
//! `.olsp` file through one conversion into the same pole.

use ols_core::units::{Density, Length, Stress};
use serde::{Deserialize, Deserializer};

/// Distance under which two heights are one, m.
const SAME_HEIGHT: f64 = 1e-3;

/// The materials a pole may be of (as `.olsp` files name them).
pub const POLE_MATERIALS: [&str; 4] = ["wood", "steel", "concrete", "other"];

/// The pole of a structure: a `.olsp` file or the `[pole]` table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "toml::Value")]
pub enum PoleRef {
    /// `pole = "id"`: the `.olsp` file with this id (or this file name)
    /// next to the structure: level 3.
    File(String),
    /// `[pole]`: described in the structure file.
    Inline(Box<InlinePole>),
}

impl TryFrom<toml::Value> for PoleRef {
    type Error = String;

    fn try_from(v: toml::Value) -> Result<Self, String> {
        match v {
            toml::Value::String(s) if !s.trim().is_empty() => Ok(PoleRef::File(s)),
            toml::Value::Table(t) => {
                let p: InlinePole = toml::Value::Table(t)
                    .try_into()
                    .map_err(|e: toml::de::Error| format!("pole: {}", e.message()))?;
                p.check()?;
                Ok(PoleRef::Inline(Box::new(p)))
            }
            _ => Err("pole: the id of a .olsp file, or a [pole] table".into()),
        }
    }
}

impl PoleRef {
    /// The table, when the pole is described in the file.
    pub fn inline(&self) -> Option<&InlinePole> {
        match self {
            PoleRef::Inline(p) => Some(p),
            PoleRef::File(_) => None,
        }
    }
}

/// The cross-section of a pole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoleShape {
    /// Round (a tube, or a solid wood or concrete pole).
    #[default]
    Round,
    /// A regular polygon (`sides`); its diameter is across flats.
    Polygon,
}

impl PoleShape {
    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            PoleShape::Round => "round",
            PoleShape::Polygon => "polygon",
        }
    }
}

/// The word `wall` takes for a pole without a hollow.
pub const SOLID: &str = "solid";

/// A pole's wall (`wall`, in `[pole]` and in `.olsp` files): a thickness
/// (a hollow pole) or the word `"solid"` (no hollow).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoleWall {
    /// The wall thickness of a hollow pole.
    Thickness(Length),
    /// Solid: no hollow.
    Solid,
}

impl PoleWall {
    /// The thickness of a hollow pole (none: solid).
    pub fn thickness(self) -> Option<Length> {
        match self {
            PoleWall::Thickness(t) => Some(t),
            PoleWall::Solid => None,
        }
    }

    /// Reads `wall` as written: a length (text with a unit, or an SI
    /// number) or `"solid"`; anything else refused, naming the key.
    pub fn from_value(v: &toml::Value) -> Result<Self, String> {
        let bad = |what: &str| format!("wall: {what}: a length (e.g. \"8 mm\") or \"{SOLID}\"");
        match v {
            toml::Value::String(s) if s.trim().eq_ignore_ascii_case(SOLID) => Ok(PoleWall::Solid),
            toml::Value::String(s) => s
                .parse::<Length>()
                .map(PoleWall::Thickness)
                .map_err(|_| bad(&format!("\"{s}\""))),
            toml::Value::Float(f) => Ok(PoleWall::Thickness(Length::from_si(*f))),
            toml::Value::Integer(i) => Ok(PoleWall::Thickness(Length::from_si(*i as f64))),
            other => Err(bad(&other.to_string())),
        }
    }
}

impl<'de> Deserialize<'de> for PoleWall {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = toml::Value::deserialize(d)?;
        PoleWall::from_value(&v).map_err(serde::de::Error::custom)
    }
}

/// One height of a pole's outline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoleRing {
    /// Height, local z of the structure.
    pub z: Length,
    /// Diameter (a polygon: across flats).
    #[serde(default)]
    pub diameter: Option<Length>,
    /// Or the circumference (a polygon: its perimeter).
    #[serde(default)]
    pub circumference: Option<Length>,
}

/// A pole described in the structure file (`[pole]`).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InlinePole {
    /// Cross-section shape (default round).
    #[serde(default)]
    pub shape: PoleShape,
    /// A polygon's number of sides.
    #[serde(default)]
    pub sides: Option<u32>,
    /// Depth in the ground (default 0): the pole's length is its height
    /// above the ground plus this.
    #[serde(default)]
    pub embedment: Option<Length>,
    /// The outside lines: diameters by height, rising heights (level 2).
    #[serde(rename = "outline", default)]
    pub outline: Vec<PoleRing>,
    /// `wood`, `steel`, `concrete` or `other` (level 3).
    #[serde(default)]
    pub material: Option<String>,
    /// A hollow section of the section library: its wall (and its
    /// material's elasticity and density, when not given here).
    #[serde(default)]
    pub section: Option<String>,
    /// Wall thickness of a hollow pole, or `"solid"` (none: not given;
    /// from `section` when there is one).
    #[serde(default)]
    pub wall: Option<PoleWall>,
    /// Wood: fibre stress (the moment capacity from the taper).
    #[serde(default)]
    pub fibre_stress: Option<Stress>,
    /// Moment capacity at the ground line (e.g. `"900 kN m"`; read by the
    /// pole check).
    #[serde(default)]
    pub moment_capacity: Option<toml::Value>,
    /// Moment capacity by height (`height`, `moment`; read by the pole
    /// check).
    #[serde(rename = "capacity", default)]
    pub capacity: Vec<toml::Table>,
    /// Young's modulus (P-Delta).
    #[serde(default)]
    pub elasticity: Option<Stress>,
    /// Density (the pole's weight, P-Delta).
    #[serde(default)]
    pub density: Option<Density>,
    /// Shape factor on the wind pressure (default 1).
    #[serde(default)]
    pub drag: Option<f64>,
}

/// Across-flats width of a regular polygon of `sides` with perimeter
/// `perimeter`.
pub fn polygon_width(sides: u32, perimeter: f64) -> f64 {
    let n = f64::from(sides);
    perimeter / (n * (std::f64::consts::PI / n).tan())
}

impl InlinePole {
    /// A polygon's number of sides (none: round).
    pub fn polygon_sides(&self) -> Option<u32> {
        match self.shape {
            PoleShape::Round => None,
            PoleShape::Polygon => self.sides,
        }
    }

    /// The diameter of a ring (across flats for a polygon), m.
    pub fn ring_diameter(&self, r: &PoleRing) -> Option<f64> {
        match (r.diameter, r.circumference) {
            (Some(d), None) => Some(d.si()),
            (None, Some(c)) => Some(match self.polygon_sides() {
                Some(n) => polygon_width(n, c.si()),
                None => c.si() / std::f64::consts::PI,
            }),
            _ => None,
        }
    }

    /// The outline as (local z, diameter), m, in file order.
    pub fn rings(&self) -> Vec<(f64, f64)> {
        self.outline
            .iter()
            .filter_map(|r| Some((r.z.si(), self.ring_diameter(r)?)))
            .collect()
    }

    /// Whether the outline makes a 3D object (level 2): diameters above 0
    /// at two heights at least.
    pub fn is_3d(&self) -> bool {
        let rings = self.rings();
        rings.len() >= 2
            && rings.iter().all(|r| r.1 > 0.0)
            && rings.last().map(|r| r.0).unwrap_or(0.0) - rings[0].0 > SAME_HEIGHT
    }

    /// The diameter at local z (linear between the outline's heights,
    /// held beyond them), m; none without an outline.
    pub fn diameter_at(&self, z: f64) -> Option<f64> {
        let rings = self.rings();
        let (first, last) = (rings.first()?, rings.last()?);
        if z <= first.0 {
            return Some(first.1);
        }
        if z >= last.0 {
            return Some(last.1);
        }
        rings.windows(2).find(|w| z <= w[1].0).map(|w| {
            let t = (z - w[0].0) / (w[1].0 - w[0].0);
            w[0].1 + (w[1].1 - w[0].1) * t
        })
    }

    /// What the pole lacks for the pole check (level 3); empty: nothing.
    pub fn missing_for_design(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.is_3d() {
            out.push("pole: no outline (diameters at two heights at least)".into());
        }
        if self.material.is_none() {
            out.push("pole: no material".into());
        }
        if self.fibre_stress.is_none() && self.moment_capacity.is_none() && self.capacity.is_empty()
        {
            out.push(
                "pole: no strength (fibre stress, moment capacity or a capacity table)".into(),
            );
        }
        if self.wall.is_none() && self.section.is_none() {
            out.push(
                "pole: no wall thickness (its wall thickness, \"solid\", or a section)".into(),
            );
        }
        out
    }

    /// What is checked when read: a polygon's sides, a diameter or a
    /// circumference at each height (above 0), rising heights, the
    /// material's name, positive values.
    pub fn check(&self) -> Result<(), String> {
        match (self.shape, self.sides) {
            (PoleShape::Polygon, Some(n)) if n >= 3 => {}
            (PoleShape::Polygon, _) => return Err("pole: a polygon needs sides (3 or more)".into()),
            (PoleShape::Round, Some(_)) => {
                return Err("pole: sides only with shape = \"polygon\"".into());
            }
            (PoleShape::Round, None) => {}
        }
        for (k, r) in self.outline.iter().enumerate() {
            let d = self.ring_diameter(r).ok_or_else(|| {
                format!(
                    "pole outline {}: give its diameter or its circumference",
                    k + 1
                )
            })?;
            if d.is_nan() || d <= 0.0 {
                return Err(format!("pole outline {}: diameter above 0", k + 1));
            }
        }
        if self.outline.windows(2).any(|w| {
            (w[1].z.si() - w[0].z.si()).is_nan() || w[1].z.si() - w[0].z.si() <= SAME_HEIGHT
        }) {
            return Err("pole outline: heights must rise".into());
        }
        if let Some(m) = &self.material
            && !POLE_MATERIALS.contains(&m.as_str())
        {
            return Err(format!(
                "pole: material \"{m}\": {}",
                POLE_MATERIALS.join(", ")
            ));
        }
        let positive = [
            self.wall.and_then(PoleWall::thickness),
            self.embedment.filter(|e| e.si() != 0.0),
        ]
        .into_iter()
        .flatten()
        .all(|v| v.si() > 0.0)
            && [self.fibre_stress, self.elasticity]
                .into_iter()
                .flatten()
                .all(|v| v.si() > 0.0)
            && self.drag.is_none_or(|d| d >= 0.0)
            && self.density.is_none_or(|d| d.si() >= 0.0);
        if !positive {
            return Err(
                "pole: wall, embedment, stresses above 0; drag and density 0 or more".into(),
            );
        }
        Ok(())
    }
}
