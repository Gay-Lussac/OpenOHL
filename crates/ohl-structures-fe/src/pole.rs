// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Poles (`*.ohlp`, one pole per file) and the pole check.
//!
//! A pole is a tapered shaft: its diameter (or circumference) at the
//! ground line and at the top, the length above ground being `length −
//! embedment`. Its strength is the user's data: for wood the fibre stress
//! (the section modulus πD³/32 at each height gives the moment capacity),
//! for steel and concrete the moment capacity at the ground line, or a
//! table of capacities by height.
//!
//! ```toml
//! format = "ohl-pole"
//! version = 1
//! id = "wood-14m"
//! name = "Wood pole 14 m (invented)"
//! material = "wood"              # wood | steel | concrete | other
//! length = "14 m"
//! embedment = "2 m"
//! top_diameter = "190 mm"        # or top_circumference
//! ground_diameter = "290 mm"     # or ground_circumference (ground line)
//! wall = "6 mm"                  # or "solid"; optional (none: solid)
//! fibre_stress = "55 MPa"        # wood: moment capacity from the taper
//! # yield = "345 MPa"            # steel, hollow, no capacity given: the
//! #                              #   shaft by ASCE/SEI 48-19 (crate::asce48)
//! # bend_radius = "20 mm"        # a polygon's inside bend radius (none: sharp)
//! # code = "sia263-2013"         # the steel shaft's code: asce48-19 (default)
//! #                              #   or sia263-2013 (crate::sia263)
//! # strength_class = "C24"       # wood, solid, no capacity: SIA 265
//! # moisture_class = 3           #   (crate::sia265; moisture 1-3, default 3)
//! moment_capacity = "180 kN m"   # or: at the ground line
//! elasticity = "11 GPa"          # optional: P-Delta amplification
//! density = "650 kg/m3"          # optional: the pole's weight (P-Delta)
//! drag = 1.0                     # shape factor on the wind pressure
//!
//! [[capacity]]                   # or: moment capacity by height
//! height = "0 m"
//! moment = "180 kN m"
//! ```
//!
//! **In the structure file**: a structure may describe its pole itself
//! (`[pole]`, [`ohl_structures::pole`]): its outline by height, round or
//! polygonal, and the same level 3 fields. [`structure_pole`] reads
//! either into the same [`Pole`] ([`Pole::from_inline`]); a two-height
//! outline from the ground line gives exactly the `.ohlp` taper.
//!
//! **Check** ([`check_pole`]): at heights from the ground line to the top,
//! the bending moment of the factored loads above (r × F about the pole
//! axis, both directions, their resultant) plus the wind on the pole
//! (pressure × drag × diameter, integrated over the taper), times the
//! P-Delta amplification of the ground-line moment (a tapered frame model
//! of the pole, [`solve`](mod@crate::solve)), against the strength factor times the
//! capacity there. Usage: the largest ratio.

use std::path::Path;

use ohl_core::budget::Budget;
use ohl_core::units::{Density, Length, Stress};
use ohl_sections::SectionLibrary;
use ohl_structures::{InlinePole, PoleRef, PoleWall, StructureType};
use serde::Deserialize;

use crate::model::{Analysis, Member, MemberKind, Model, NodalLoad, Node, Section};
use crate::solve::{FeError, Loads, Options, solve};
use crate::units::Moment;

/// `format` of a pole file.
pub const POLE_FILE_FORMAT: &str = "ohl-pole";
/// Newest format version read.
pub const POLE_FILE_VERSION: u32 = 1;
/// File extension of poles.
pub const POLE_FILE_EXTENSION: &str = "ohlp";

/// Pole material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoleMaterial {
    /// Wood: capacity from the fibre stress and the taper.
    Wood,
    /// Steel.
    Steel,
    /// Concrete.
    Concrete,
    /// Anything else.
    Other,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapacityFile {
    height: Length,
    moment: Moment,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PoleFile {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    id: String,
    #[serde(default)]
    name: String,
    material: PoleMaterial,
    #[serde(default)]
    source: Option<String>,
    length: Length,
    #[serde(default)]
    embedment: Length,
    #[serde(default)]
    top_diameter: Option<Length>,
    #[serde(default)]
    top_circumference: Option<Length>,
    #[serde(default)]
    ground_diameter: Option<Length>,
    #[serde(default)]
    ground_circumference: Option<Length>,
    #[serde(default)]
    wall: Option<PoleWall>,
    #[serde(default)]
    fibre_stress: Option<Stress>,
    #[serde(default, rename = "yield")]
    yield_strength: Option<Stress>,
    #[serde(default)]
    bend_radius: Option<Length>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    strength_class: Option<String>,
    #[serde(default)]
    moisture_class: Option<u8>,
    #[serde(default)]
    moment_capacity: Option<Moment>,
    #[serde(default)]
    elasticity: Option<Stress>,
    #[serde(default)]
    density: Density,
    #[serde(default)]
    drag: Option<f64>,
    #[serde(rename = "capacity", default)]
    capacity: Vec<CapacityFile>,
}

/// A pole, SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct Pole {
    /// Unique id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Material.
    pub material: PoleMaterial,
    /// Where the data comes from.
    pub source: Option<String>,
    /// Total length, m.
    pub length: f64,
    /// Depth in the ground, m.
    pub embedment: f64,
    /// Diameter at the top, m.
    pub top_diameter: f64,
    /// Diameter at the ground line, m.
    pub ground_diameter: f64,
    /// Wall thickness of a hollow pole, m (none: solid).
    pub wall: Option<f64>,
    /// Wood: fibre stress, Pa.
    pub fibre_stress: Option<f64>,
    /// Steel: specified minimum yield stress, Pa — a hollow steel shaft
    /// without a given capacity is checked by ASCE/SEI 48-19
    /// ([`crate::asce48`]).
    pub yield_strength: Option<f64>,
    /// Inside bend radius of a polygonal shaft, m (none: sharp corners).
    pub bend_radius: Option<f64>,
    /// The code a hollow steel shaft is checked by ([`PoleCode`]).
    pub code: PoleCode,
    /// Wood: the strength class of SIA 265 (Table 6, e.g. "C24") — a
    /// solid wood pole with it and no capacity given is checked by SIA
    /// 265 ([`crate::sia265`]).
    pub strength_class: Option<String>,
    /// Its moisture class (SIA 265 Table 4: 1, 2 or 3; none: 3, on the
    /// safe side for a pole outdoors).
    pub moisture_class: Option<u8>,
    /// Moment capacity at the ground line, N·m.
    pub moment_capacity: Option<f64>,
    /// Moment capacity by height above ground: (m, N·m), rising heights.
    pub capacity: Vec<(f64, f64)>,
    /// Young's modulus, Pa (none: no P-Delta).
    pub elasticity: Option<f64>,
    /// Density, kg/m³.
    pub density: f64,
    /// Shape factor on the wind pressure.
    pub drag: f64,
    /// A regular polygon's number of sides (none: round); its diameters
    /// are across flats.
    pub sides: Option<u32>,
    /// The diameter by height above ground (m, m), rising heights, when
    /// it is not the linear taper from the ground line to the top (empty:
    /// that taper).
    pub profile: Vec<(f64, f64)>,
}

/// The code a hollow steel shaft without a given capacity is checked by
/// (`code` in a pole file or `[pole]`; owner 2026-11-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoleCode {
    /// ASCE/SEI 48-19 (`"asce48-19"`, the default; [`crate::asce48`]).
    #[default]
    Asce48,
    /// SIA 263:2013 (`"sia263-2013"`; [`crate::sia263::shaft_usage`]).
    Sia263,
}

impl PoleCode {
    /// The code of a name (none: the default).
    pub fn parse(name: Option<&str>) -> Result<Self, String> {
        match name.map(str::trim) {
            None | Some("asce48-19") => Ok(Self::Asce48),
            Some(crate::sia263::NAME) => Ok(Self::Sia263),
            Some(other) => Err(format!(
                "code \"{other}\": asce48-19 (default) or {}",
                crate::sia263::NAME
            )),
        }
    }

    /// As results name it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Asce48 => "ASCE/SEI 48-19",
            Self::Sia263 => crate::sia263::TITLE,
        }
    }
}

impl Pole {
    /// Parses and checks a pole file's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let f: PoleFile = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            f.format.as_deref(),
            f.version,
            POLE_FILE_FORMAT,
            POLE_FILE_VERSION,
            "pole",
        )?;
        let pi = std::f64::consts::PI;
        let diameter = |d: Option<Length>, c: Option<Length>, what: &str| match (d, c) {
            (Some(d), None) => Ok(d.si()),
            (None, Some(c)) => Ok(c.si() / pi),
            _ => Err(format!(
                "pole {}: give {what}_diameter or {what}_circumference",
                f.id
            )),
        };
        let pole = Pole {
            top_diameter: diameter(f.top_diameter, f.top_circumference, "top")?,
            ground_diameter: diameter(f.ground_diameter, f.ground_circumference, "ground")?,
            id: f.id,
            name: f.name,
            material: f.material,
            source: f.source,
            length: f.length.si(),
            embedment: f.embedment.si(),
            wall: f.wall.and_then(PoleWall::thickness).map(|w| w.si()),
            fibre_stress: f.fibre_stress.map(|s| s.si()),
            yield_strength: f.yield_strength.map(|s| s.si()),
            bend_radius: f.bend_radius.map(|l| l.si()),
            code: PoleCode::parse(f.code.as_deref()).map_err(|e| format!("pole: {e}"))?,
            strength_class: f.strength_class.clone(),
            moisture_class: f.moisture_class,
            moment_capacity: f.moment_capacity.map(|m| m.0),
            capacity: f
                .capacity
                .iter()
                .map(|c| (c.height.si(), c.moment.0))
                .collect(),
            elasticity: f.elasticity.map(|e| e.si()),
            density: f.density.si(),
            drag: f.drag.unwrap_or(1.0),
            sides: None,
            profile: Vec::new(),
        };
        pole.check()?;
        Ok(pole)
    }

    /// The pole a structure file describes (`[pole]`), its id `id`: the
    /// outline's heights above the feet (`foot`, local z) give the
    /// diameters by height above the ground line; the level 3 fields read
    /// as in a `.ohlp` file. A `section` of `library` (a polygonal or
    /// circular hollow section) gives the wall (unless `wall` is typed: a
    /// thickness, or solid), and its material the elasticity and density
    /// not given. Checked as a `.ohlp` pole.
    pub fn from_inline(
        id: &str,
        name: &str,
        p: &InlinePole,
        foot: f64,
        library: &SectionLibrary,
    ) -> Result<Self, String> {
        let bad = |m: String| format!("pole of {id}: {m}");
        let missing = p.missing_for_design();
        if !missing.is_empty() {
            return Err(bad(format!("not complete: {}", missing.join("; "))));
        }
        let rings: Vec<(f64, f64)> = p.rings().iter().map(|(z, d)| (z - foot, *d)).collect();
        if rings[0].0 < -1e-3 {
            return Err(bad("the outline starts below the feet".into()));
        }
        let (first, last) = (rings[0], rings[rings.len() - 1]);
        let height = last.0;
        let ground = p.diameter_at(foot).expect("an outline");
        // The plain taper from the ground line: no profile (the .ohlp
        // formula); anything else by height.
        let profile = if rings.len() == 2 && first.0.abs() < 1e-9 {
            Vec::new()
        } else {
            rings.clone()
        };
        let material: PoleMaterial = toml::Value::String(p.material.clone().unwrap_or_default())
            .try_into()
            .map_err(|e: toml::de::Error| bad(e.message().to_owned()))?;
        let (mut wall, mut elasticity, mut density) = (
            p.wall.and_then(PoleWall::thickness).map(|w| w.si()),
            p.elasticity.map(|e| e.si()),
            p.density.map(|d| d.si()),
        );
        let mut yield_strength = None;
        if let Some(sid) = &p.section {
            let s = library.section(sid).ok_or_else(|| {
                bad(format!(
                    "cross-section {sid} is not in the cross-section library"
                ))
            })?;
            let t = match (&s.shape, p.polygon_sides()) {
                (ohl_sections::Shape::PolygonalHollow { sides, t, .. }, Some(n)) if *sides == n => {
                    *t
                }
                (ohl_sections::Shape::CircularHollow { t, .. }, None) => *t,
                _ => {
                    return Err(bad(format!(
                        "section {sid} is not a hollow section of the pole's shape"
                    )));
                }
            };
            // A typed wall (a thickness or solid) wins over the section.
            if p.wall.is_none() {
                wall = Some(t);
            }
            if let Some(m) = library.material_of(s) {
                elasticity = elasticity.or(Some(m.elasticity));
                density = density.or(Some(m.density));
                yield_strength = m.yield_strength;
            }
        }
        let moment_capacity = match &p.moment_capacity {
            Some(v) => Some(
                v.clone()
                    .try_into::<Moment>()
                    .map_err(|e| bad(format!("moment_capacity: {}", e.message())))?
                    .0,
            ),
            None => None,
        };
        let capacity = p
            .capacity
            .iter()
            .map(|t| {
                toml::Value::Table(t.clone())
                    .try_into::<CapacityFile>()
                    .map(|c| (c.height.si(), c.moment.0))
                    .map_err(|e| bad(format!("capacity: {}", e.message())))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let embedment = p.embedment.map_or(0.0, |e| e.si());
        let pole = Pole {
            id: id.to_owned(),
            name: name.to_owned(),
            material,
            source: None,
            length: height + embedment,
            embedment,
            top_diameter: last.1,
            ground_diameter: ground,
            wall,
            fibre_stress: p.fibre_stress.map(|s| s.si()),
            yield_strength: p.yield_strength.map(|s| s.si()).or(yield_strength),
            bend_radius: p.bend_radius.map(|l| l.si()),
            code: PoleCode::parse(p.code.as_deref()).map_err(bad)?,
            strength_class: p.strength_class.clone(),
            moisture_class: p.moisture_class,
            moment_capacity,
            capacity,
            elasticity,
            density: density.unwrap_or(0.0),
            drag: p.drag.unwrap_or(1.0),
            sides: p.polygon_sides(),
            profile,
        };
        pole.check()?;
        Ok(pole)
    }

    /// Reads a pole file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Plausibility checks.
    pub fn check(&self) -> Result<(), String> {
        let id = &self.id;
        if !(self.length > 0.0 && self.embedment >= 0.0 && self.embedment < self.length) {
            return Err(format!(
                "pole {id}: length above 0, embedment from 0 to below the length"
            ));
        }
        if !(self.top_diameter > 0.0 && self.ground_diameter > 0.0) {
            return Err(format!("pole {id}: diameters must be above 0"));
        }
        if self
            .wall
            .is_some_and(|w| !(w > 0.0 && 2.0 * w < self.top_diameter.min(self.ground_diameter)))
        {
            return Err(format!(
                "pole {id}: wall above 0 and below half the diameter"
            ));
        }
        let positive = [self.fibre_stress, self.moment_capacity, self.elasticity]
            .into_iter()
            .flatten()
            .chain(self.capacity.iter().map(|c| c.1))
            .all(|x| x > 0.0);
        if !positive
            || self.drag.is_nan()
            || self.drag < 0.0
            || self.density.is_nan()
            || self.density < 0.0
        {
            return Err(format!(
                "pole {id}: stresses, capacities above 0; drag and density 0 or more"
            ));
        }
        if self
            .capacity
            .windows(2)
            .any(|w| w[1].0.is_nan() || w[1].0 <= w[0].0)
        {
            return Err(format!("pole {id}: capacity heights must rise"));
        }
        if let Some(c) = &self.strength_class
            && crate::sia265::class(c).is_none()
        {
            return Err(format!(
                "pole {id}: strength_class \"{c}\": one of {}",
                crate::sia265::class_names().join(", ")
            ));
        }
        if let Some(m) = self.moisture_class
            && crate::sia265::eta_w(m).is_none()
        {
            return Err(format!("pole {id}: moisture_class {m}: 1, 2 or 3"));
        }
        if self.fibre_stress.is_none()
            && self.moment_capacity.is_none()
            && self.capacity.is_empty()
            && !self.by_code()
            && !self.by_timber_code()
        {
            return Err(format!(
                "pole {id}: give fibre_stress, moment_capacity or [[capacity]], for a hollow \
                 steel shaft its yield, or for a wood pole its strength_class"
            ));
        }
        Ok(())
    }

    /// Whether the shaft is checked by ASCE/SEI 48-19 ([`crate::asce48`]):
    /// steel, hollow, its yield stress given, no capacity given (a given
    /// capacity — a table, a ground-line moment, a fibre stress — is used
    /// as before).
    pub fn by_code(&self) -> bool {
        self.material == PoleMaterial::Steel
            && self.yield_strength.is_some()
            && self.wall.is_some()
            && self.capacity.is_empty()
            && self.moment_capacity.is_none()
            && self.fibre_stress.is_none()
    }

    /// Whether the pole is checked by SIA 265 ([`crate::sia265`]): wood,
    /// solid, its strength class given, no capacity given.
    pub fn by_timber_code(&self) -> bool {
        self.material == PoleMaterial::Wood
            && self.wall.is_none()
            && self.strength_class.is_some()
            && self.capacity.is_empty()
            && self.moment_capacity.is_none()
            && self.fibre_stress.is_none()
    }

    /// The shaft's cross-section at height `h` (ASCE/SEI 48-19), none
    /// when it is solid.
    pub fn shaft_at(&self, h: f64) -> Option<crate::asce48::Shaft> {
        Some(crate::asce48::Shaft {
            sides: self.sides,
            d: self.diameter_at(h),
            t: self.wall?,
            bend_radius: self.bend_radius,
        })
    }

    /// Height above ground, m.
    pub fn height(&self) -> f64 {
        self.length - self.embedment
    }

    /// The shaft's perimeter at height `h` above ground, m: π D round; a
    /// regular polygon of `sides`, its diameter across flats, n · D ·
    /// tan(π / n) (larger than π D by its corners).
    pub fn perimeter_at(&self, h: f64) -> f64 {
        let d = self.diameter_at(h);
        match self.sides.filter(|n| *n >= 3) {
            Some(n) => {
                let n = f64::from(n);
                n * d * (std::f64::consts::PI / n).tan()
            }
            None => std::f64::consts::PI * d,
        }
    }

    /// Diameter at height `h` above ground, m (linear taper, or the
    /// profile: linear between its heights, held beyond them).
    pub fn diameter_at(&self, h: f64) -> f64 {
        if let (Some(first), Some(last)) = (self.profile.first(), self.profile.last()) {
            if h <= first.0 {
                return first.1;
            }
            if h >= last.0 {
                return last.1;
            }
            for w in self.profile.windows(2) {
                if h <= w[1].0 {
                    let t = (h - w[0].0) / (w[1].0 - w[0].0);
                    return w[0].1 + (w[1].1 - w[0].1) * t;
                }
            }
        }
        let t = (h / self.height()).clamp(0.0, 1.0);
        self.ground_diameter + (self.top_diameter - self.ground_diameter) * t
    }

    /// Moment capacity at height `h`, N·m: the table (linear, held beyond
    /// its ends), else the fibre stress × πD³/32, else the ground-line
    /// capacity.
    pub fn capacity_at(&self, h: f64) -> f64 {
        if let (Some(first), Some(last)) = (self.capacity.first(), self.capacity.last()) {
            if h <= first.0 {
                return first.1;
            }
            if h >= last.0 {
                return last.1;
            }
            for w in self.capacity.windows(2) {
                if h <= w[1].0 {
                    let t = (h - w[0].0) / (w[1].0 - w[0].0);
                    return w[0].1 + (w[1].1 - w[0].1) * t;
                }
            }
        }
        if let Some(f) = self.fibre_stress {
            let d = self.diameter_at(h);
            return f * self.modulus(d);
        }
        self.moment_capacity.unwrap_or(f64::INFINITY)
    }

    /// Elastic section modulus at diameter `d`, m³: a polygon's farthest
    /// fibre is a corner (the smaller of its moduli).
    fn modulus(&self, d: f64) -> f64 {
        let i = self.second_moment(d);
        match self.sides {
            Some(n) => i / (d / 2.0 / (std::f64::consts::PI / f64::from(n)).cos()),
            None => i / (d / 2.0),
        }
    }

    /// Second moment of area at diameter `d`, m⁴ (a regular polygon: the
    /// same about every axis through its centre, A·(12 r² + a²)/48 for
    /// inradius r and side a).
    pub(crate) fn second_moment(&self, d: f64) -> f64 {
        match self.wall {
            Some(t) => self.solid_second_moment(d) - self.solid_second_moment(d - 2.0 * t),
            None => self.solid_second_moment(d),
        }
    }

    fn solid_second_moment(&self, d: f64) -> f64 {
        match self.sides {
            Some(n) => {
                let (r, tan) = (d / 2.0, (std::f64::consts::PI / f64::from(n)).tan());
                let a = f64::from(n) * r * r * tan;
                a * r * r * (3.0 + tan * tan) / 12.0
            }
            None => std::f64::consts::PI / 64.0 * d.powi(4),
        }
    }

    /// Area at diameter `d`, m².
    pub(crate) fn area(&self, d: f64) -> f64 {
        match self.wall {
            Some(t) => self.solid_area(d) - self.solid_area(d - 2.0 * t),
            None => self.solid_area(d),
        }
    }

    fn solid_area(&self, d: f64) -> f64 {
        match self.sides {
            Some(n) => {
                let r = d / 2.0;
                f64::from(n) * r * r * (std::f64::consts::PI / f64::from(n)).tan()
            }
            None => std::f64::consts::PI / 4.0 * d * d,
        }
    }
}

/// The pole of structure `t` for the pole check (none: it has none): its
/// `[pole]` table ([`Pole::from_inline`]), or the `.ohlp` file it names —
/// by file name when the name ends in `.ohlp`, else by id among the
/// `.ohlp` files of `dir` (the structure file's folder).
pub fn structure_pole(
    t: &StructureType,
    dir: &Path,
    library: &SectionLibrary,
) -> Option<Result<Pole, String>> {
    Some(match t.pole.as_ref()? {
        PoleRef::Inline(p) => Pole::from_inline(&t.id, &t.name, p, t.foot_level(), library),
        PoleRef::File(name)
            if ohl_core::files::has_extension(Path::new(name), POLE_FILE_EXTENSION) =>
        {
            Pole::load(&dir.join(name))
        }
        PoleRef::File(id) => load_poles(dir).and_then(|list| {
            list.into_iter().find(|p| &p.id == id).ok_or_else(|| {
                format!("pole {id}: no .ohlp file with this id in {}", dir.display())
            })
        }),
    })
}

/// Every pole (`*.ohlp`) of a directory, sorted by file name.
pub fn load_poles(dir: &Path) -> Result<Vec<Pole>, String> {
    let mut out: Vec<Pole> = Vec::new();
    for path in ohl_core::files::files_with_extension(dir, POLE_FILE_EXTENSION) {
        let p = Pole::load(&path)?;
        if out.iter().any(|q| q.id == p.id) {
            return Err(format!("{}: pole id {} given twice", path.display(), p.id));
        }
        out.push(p);
    }
    Ok(out)
}

/// A load on the pole: where (structure axes, z above ground) and the
/// force (x right, y ahead, z up), N, factored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoleLoad {
    /// Point of application, m.
    pub point: [f64; 3],
    /// Force, N.
    pub force: [f64; 3],
}

/// Result of the pole check.
#[derive(Debug, Clone, PartialEq)]
pub struct PoleCheck {
    /// Share of the capacity used (1 = 100 %; infinite: the pole buckles).
    pub usage: f64,
    /// Height of the governing section above ground, m.
    pub height: f64,
    /// Bending moment there (amplified), N·m.
    pub moment: f64,
    /// Capacity there (before the strength factor), N·m.
    pub capacity: f64,
    /// Ground-line moment, first order, N·m.
    pub ground_moment: f64,
    /// Of which the wind on the pole, N·m.
    pub wind_moment: f64,
    /// P-Delta amplification of the ground-line moment (1: not computed).
    pub amplification: f64,
    /// What governs, in words.
    pub governs: String,
}

/// Sections checked between the ground line and the top.
const HEIGHTS: usize = 40;

/// Bending moment (x, y components) at height `h` from the loads above
/// and the wind, N·m.
fn moment_at(pole: &Pole, loads: &[PoleLoad], wind: [f64; 2], h: f64) -> ([f64; 2], [f64; 2]) {
    let mut m = [0.0; 2];
    for l in loads.iter().filter(|l| l.point[2] > h) {
        let r = [l.point[0], l.point[1], l.point[2] - h];
        let f = l.force;
        // r × F, the horizontal components.
        m[0] += r[1] * f[2] - r[2] * f[1];
        m[1] += r[2] * f[0] - r[0] * f[2];
    }
    // Wind: q(z) = p·drag·D(z) along the wind, moment ∫ q (z − h) dz.
    let top = pole.height();
    let mut mw = [0.0; 2];
    if (wind[0] != 0.0 || wind[1] != 0.0) && top > h {
        let n = 64;
        let dz = (top - h) / n as f64;
        let mut s = 0.0;
        for k in 0..n {
            let z = h + (k as f64 + 0.5) * dz;
            s += pole.drag * pole.diameter_at(z) * (z - h) * dz;
        }
        // Force along +x gives a moment about −y... r × F with r = z ẑ.
        mw = [-s * wind[1], s * wind[0]];
    }
    ([m[0] + mw[0], m[1] + mw[1]], mw)
}

/// The axial force (compression positive), the horizontal shear and the
/// torsion at height `h`, N and N·m: the loads above (their vertical and
/// horizontal parts, the torsion of their horizontal parts about the
/// axis), the wind on the shaft above, the shaft's own weight above times
/// `self_weight` (a hollow shaft: its wall's area at each height).
fn forces_at(
    pole: &Pole,
    loads: &[PoleLoad],
    wind: [f64; 2],
    h: f64,
    self_weight: f64,
) -> (f64, f64, f64) {
    let (mut p, mut v, mut t) = (0.0, [0.0; 2], 0.0);
    for l in loads.iter().filter(|l| l.point[2] > h) {
        p -= l.force[2];
        v[0] += l.force[0];
        v[1] += l.force[1];
        t += l.point[0] * l.force[1] - l.point[1] * l.force[0];
    }
    let top = pole.height();
    if top > h {
        let n = 64;
        let dz = (top - h) / n as f64;
        let (mut q, mut w) = (0.0, 0.0);
        for k in 0..n {
            let z = h + (k as f64 + 0.5) * dz;
            q += pole.drag * pole.diameter_at(z) * dz;
            if let Some(s) = pole.shaft_at(z) {
                w += s.area_inertia().0 * dz;
            }
        }
        v[0] += q * wind[0];
        v[1] += q * wind[1];
        p += w * pole.density * crate::model::G * self_weight;
    }
    (p, v[0].hypot(v[1]), t)
}

/// Checks a pole under `loads`, `wind` the factored wind pressure on it as
/// a vector (x, y), Pa, `strength` the load case's strength factor. With
/// `p_delta` and the pole's elasticity, the moments are amplified by the
/// P-Delta ratio of a frame model of the pole (12 tapered elements, fixed
/// at the ground line); its iterations use `budget`. The pole's own
/// weight in that model is unfactored: [`check_pole_factored`] takes a
/// load case's vertical factor on it. Err: a load, the wind or the
/// strength factor not a finite number (named), or the P-Delta solve
/// failing.
pub fn check_pole(
    pole: &Pole,
    loads: &[PoleLoad],
    wind: [f64; 2],
    strength: f64,
    p_delta: bool,
    budget: &Budget,
) -> Result<PoleCheck, String> {
    check_pole_factored(pole, loads, wind, strength, 1.0, p_delta, budget)
}

/// [`check_pole`], the pole's own weight in the P-Delta model times
/// `self_weight` (the load case's vertical factor, as the members' own
/// weight in the finite-element check; physics review 2026-10-26: with
/// the attachment loads factored and the pole's weight not, the P-Delta
/// ratio came out a little low).
///
/// Without P-Delta — the pole has no elasticity, or `p_delta` is off —
/// the moments are first order: unsafe by the amplification, said in what
/// governs (`[not on the safe side: …]`, 2026-11-08).
pub fn check_pole_factored(
    pole: &Pole,
    loads: &[PoleLoad],
    wind: [f64; 2],
    strength: f64,
    self_weight: f64,
    p_delta: bool,
    budget: &Budget,
) -> Result<PoleCheck, String> {
    let mut c = check_pole_first(pole, loads, wind, strength, self_weight, p_delta, budget)?;
    let why = match (p_delta, pole.elasticity) {
        (true, Some(_)) => None,
        (true, None) => Some("no elasticity given for the pole or its material"),
        (false, _) => Some("P-Delta turned off"),
    };
    if let Some(why) = why {
        c.governs.push_str(&format!(
            " {} P-Delta not computed ({why}): the moments are first order, low by the \
             amplification]",
            crate::check::NOT_SAFE
        ));
    }
    Ok(c)
}

fn check_pole_first(
    pole: &Pole,
    loads: &[PoleLoad],
    wind: [f64; 2],
    strength: f64,
    self_weight: f64,
    p_delta: bool,
    budget: &Budget,
) -> Result<PoleCheck, String> {
    // A load that is not a finite number would place no node in the
    // P-Delta model and give no usage: refused, named (code review
    // 2026-10-26).
    for (k, l) in loads.iter().enumerate() {
        if !l.point.iter().chain(&l.force).all(|v| v.is_finite()) {
            return Err(format!(
                "pole {}: load {} is not a finite number (at {:?} m, force {:?} N)",
                pole.id,
                k + 1,
                l.point,
                l.force
            ));
        }
    }
    if !wind.iter().all(|v| v.is_finite()) {
        return Err(format!(
            "pole {}: the wind pressure {wind:?} Pa is not a finite number",
            pole.id
        ));
    }
    if !(strength.is_finite() && strength > 0.0) || !self_weight.is_finite() {
        return Err(format!(
            "pole {}: strength factor {strength} or self-weight factor {self_weight} not a \
             finite positive number",
            pole.id
        ));
    }
    let (ground, wind_ground) = moment_at(pole, loads, wind, 0.0);
    let ground_moment = ground[0].hypot(ground[1]);
    let amplification = match (p_delta, pole.elasticity) {
        (true, Some(e)) => match amplification(pole, e, loads, wind, self_weight, budget) {
            Ok(a) => a,
            Err(FeError::Unstable { .. }) => {
                return Ok(PoleCheck {
                    usage: f64::INFINITY,
                    height: 0.0,
                    moment: f64::INFINITY,
                    capacity: pole.capacity_at(0.0),
                    ground_moment,
                    wind_moment: wind_ground[0].hypot(wind_ground[1]),
                    amplification: f64::INFINITY,
                    governs: "the pole buckles under the vertical loads (P-Delta)".into(),
                });
            }
            Err(e) => return Err(format!("pole {}: P-Delta: {e}", pole.id)),
        },
        _ => 1.0,
    };
    // SIA 265: the class, η_w and the pole's k_c — a cantilever, L_K = 2H
    // (fixed at the ground line, free at the top), its radius of gyration
    // at mid-height (approximate for a tapered pole).
    let timber = if pole.by_timber_code() {
        let c = pole
            .strength_class
            .as_deref()
            .and_then(crate::sia265::class)
            .ok_or_else(|| format!("pole {}: strength class unknown", pole.id))?;
        let eta = crate::sia265::eta_w(pole.moisture_class.unwrap_or(3)).unwrap_or(0.6);
        let i = pole.diameter_at(pole.height() / 2.0) / 4.0;
        let lambda = 2.0 * pole.height() / i;
        Some((
            c,
            eta,
            crate::sia265::kc(crate::sia265::lambda_rel(lambda, &c)),
        ))
    } else {
        None
    };
    let mut worst: Option<(f64, f64, f64, f64, String)> = None;
    for k in 0..=HEIGHTS {
        let h = pole.height() * k as f64 / HEIGHTS as f64;
        let (m, _) = moment_at(pole, loads, wind, h);
        let moment = m[0].hypot(m[1]) * amplification;
        let (usage, cap, words) = match (pole.by_code(), pole.shaft_at(h), pole.yield_strength) {
            (true, Some(shaft), Some(fy)) => {
                // ASCE/SEI 48-19: the axial force, shear and torsion of
                // the loads above, with the moment.
                let (p, v, t) = forces_at(pole, loads, wind, h, self_weight);
                let (u, w) = match pole.code {
                    PoleCode::Asce48 => crate::asce48::usage(&shaft, p, moment, v, t, fy, strength),
                    PoleCode::Sia263 => {
                        crate::sia263::shaft_usage(&shaft, p, moment, v, t, fy, strength)
                    }
                };
                let (_, i) = shaft.area_inertia();
                (u, fy * i / shaft.reach(), w)
            }
            _ if timber.is_some() => {
                let (c, eta, kc) = timber.expect("checked");
                let (p, v, t) = forces_at(pole, loads, wind, h, self_weight);
                let d = pole.diameter_at(h);
                let (u, w) = crate::sia265::round_usage(d, p, moment, v, t, kc, &c, eta, strength);
                let cap = c.fm_d * 1e6 * eta * std::f64::consts::PI * d.powi(3) / 32.0;
                (u, cap, w)
            }
            _ => {
                let cap = pole.capacity_at(h);
                (moment / (strength * cap), cap, String::new())
            }
        };
        // A usage that is not a number (a capacity of 0) is the worst.
        if worst
            .as_ref()
            .is_none_or(|w| crate::check::usage_order(usage, w.0).is_gt())
        {
            worst = Some((usage, h, moment, cap, words));
        }
    }
    let (usage, height, moment, capacity, code) = worst.expect("heights checked");
    if let Some((c, eta, _)) = &timber {
        return Ok(PoleCheck {
            usage,
            height,
            moment,
            capacity,
            ground_moment,
            wind_moment: wind_ground[0].hypot(wind_ground[1]),
            amplification,
            governs: format!(
                "{} at {height:.1} m, {} η_w {eta}{}: {code} × {strength}; moment {:.1} kN m{}",
                crate::sia265::TITLE,
                c.name,
                if pole.moisture_class.is_none() {
                    " (moisture class 3 assumed)"
                } else {
                    ""
                },
                moment / 1000.0,
                if amplification != 1.0 {
                    format!(" (P-Delta × {amplification:.3})")
                } else {
                    String::new()
                }
            ),
        });
    }
    if pole.by_code() {
        return Ok(PoleCheck {
            usage,
            height,
            moment,
            capacity,
            ground_moment,
            wind_moment: wind_ground[0].hypot(wind_ground[1]),
            amplification,
            governs: format!(
                "{} at {height:.1} m: {code} × {strength}; moment {:.1} kN m{}",
                pole.code.title(),
                moment / 1000.0,
                if amplification != 1.0 {
                    format!(" (P-Delta × {amplification:.3})")
                } else {
                    String::new()
                }
            ),
        });
    }
    Ok(PoleCheck {
        usage,
        height,
        moment,
        capacity,
        ground_moment,
        wind_moment: wind_ground[0].hypot(wind_ground[1]),
        amplification,
        governs: format!(
            "moment {:.1} kN m of {:.1} kN m × {strength} at {:.1} m{}",
            moment / 1000.0,
            capacity / 1000.0,
            height,
            if amplification != 1.0 {
                format!(" (P-Delta × {amplification:.3})")
            } else {
                String::new()
            }
        ),
    })
}

/// Elements of the pole's frame model.
const ELEMENTS: usize = 12;

/// The pole as a frame model above the ground line (fixed there), with
/// the loads on its axis (offsets as moments) and the wind as nodal
/// forces: the ratio of the second- to the first-order ground moment.
fn amplification(
    pole: &Pole,
    e: f64,
    loads: &[PoleLoad],
    wind: [f64; 2],
    self_weight: f64,
    budget: &Budget,
) -> Result<f64, FeError> {
    let top = pole.height();
    // Node heights: even steps plus every load height.
    let mut z: Vec<f64> = (0..=ELEMENTS)
        .map(|k| top * k as f64 / ELEMENTS as f64)
        .collect();
    for l in loads {
        z.push(l.point[2].clamp(0.0, top));
    }
    z.sort_by(f64::total_cmp);
    z.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut model = Model {
        name: pole.id.clone(),
        analysis: Analysis {
            nonlinear: true,
            self_weight: true,
            // A pole's own weight is its shaft's; what it carries besides
            // is in the pole check's notes.
            dead_load_given: true,
            ..Analysis::default()
        },
        ..Model::default()
    };
    for (k, h) in z.iter().enumerate() {
        let id = format!("n{k}");
        model.nodes.push(if k == 0 {
            Node::fixed(&id, [0.0, 0.0, *h])
        } else {
            Node::free(&id, [0.0, 0.0, *h])
        });
    }
    for k in 0..z.len() - 1 {
        let d = pole.diameter_at(0.5 * (z[k] + z[k + 1]));
        let i = pole.second_moment(d);
        let mut s = Section::elastic(&format!("s{k}"), e, pole.area(d), i);
        s.density = pole.density;
        model.sections.push(s);
        model.members.push(Member::new(
            &format!("m{k}"),
            MemberKind::Frame,
            k,
            k + 1,
            k,
        ));
    }
    // Every load height is a node (the loads are finite, checked by the
    // caller); the nearest node all the same, never a panic.
    let node_at = |h: f64| {
        let h = h.clamp(0.0, top);
        z.iter()
            .position(|x| (x - h).abs() < 1e-6)
            .unwrap_or_else(|| {
                z.iter()
                    .enumerate()
                    .min_by(|a, b| (a.1 - h).abs().total_cmp(&(b.1 - h).abs()))
                    .map_or(0, |(k, _)| k)
            })
    };
    let mut nodal = Vec::new();
    for l in loads {
        let r = [l.point[0], l.point[1]];
        let f = l.force;
        nodal.push(NodalLoad {
            node: node_at(l.point[2]),
            force: f,
            // The offset from the axis: (r, 0) × F.
            moment: [r[1] * f[2], -r[0] * f[2], r[0] * f[1] - r[1] * f[0]],
        });
    }
    if wind != [0.0, 0.0] {
        for k in 0..z.len() - 1 {
            let (a, b) = (z[k], z[k + 1]);
            let w = pole.drag * pole.diameter_at(0.5 * (a + b)) * (b - a) / 2.0;
            for n in [k, k + 1] {
                nodal.push(NodalLoad {
                    node: n,
                    force: [wind[0] * w, wind[1] * w, 0.0],
                    moment: [0.0; 3],
                });
            }
        }
    }
    let loads = Loads {
        nodal,
        self_weight,
        ..Default::default()
    };
    let ground = |s: &crate::Solution| {
        let r = s
            .reactions
            .iter()
            .find(|r| r.node == 0)
            .expect("fixed base");
        r.moment[0].hypot(r.moment[1])
    };
    let linear = solve(
        &model,
        &loads,
        &Options {
            nonlinear: false,
            ..Options::default()
        },
        budget,
    )?;
    let second = solve(&model, &loads, &Options::default(), budget)?;
    let m1 = ground(&linear);
    Ok(if m1 > 0.0 { ground(&second) / m1 } else { 1.0 })
}
