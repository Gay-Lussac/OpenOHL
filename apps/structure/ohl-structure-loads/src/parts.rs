// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The two parts of a structure load case (owner, 2026-10-25; spec:
//! `docs/development/structure-load-cases-spec.md`): **line forces**
//! (`[[load_case.line]]`: a line result's forces at the attachment
//! points, a factor per axis, points turned off) and **loads on the
//! structure itself** (`[load_case.structure]`: its self weight, the wind
//! on its body, ice or another weight per metre on its members, point
//! loads). With them: the sentence saying what a case does
//! ([`describe`]), the conversion of an older case's loads where it is
//! exact ([`convert_older`]) and the mirror of a wind case
//! ([`mirror_wind`]).

use ohl_core::units::{Density, Force, ForcePerLength, Length, Stress};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::recipe::{Direction, LineQuantity, LoadCaseRecipe, LoadRecipe, Pick};

fn one() -> f64 {
    1.0
}

fn all() -> Pick {
    Pick::All
}

fn is_all(p: &Pick) -> bool {
    *p == Pick::All
}

fn ser_with<S: Serializer>(v: &Option<f64>, unit: &str, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => s.serialize_str(&format!("{x} {unit}")),
        None => s.serialize_none(),
    }
}

fn ser_pressure<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    ser_with(v, "Pa", s)
}

fn ser_length<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    ser_with(v, "m", s)
}

fn ser_density<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    ser_with(v, "kg/m³", s)
}

fn ser_per_metre<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    ser_with(v, "N/m", s)
}

fn ser_force<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{v} N"))
}

fn de_force<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    ohl_core::units::si::de::<_, Force>(d)
}

/// The rows of the load table are numbered by part (column `load`), so
/// a value typed over a row keeps matching when another part changes:
/// the older loads 1 to 999, then per part a block of [`PART_BLOCK`] —
/// line forces from 1001, wind on the body from 2001, ice and weight on
/// the members from 3001, point loads from 4001, snow on the members from
/// 5001 (the part's first item 1001, its second 1002, …).
pub const PART_BLOCK: usize = 1000;

/// The parts of a case, as the rows of the load table number them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// An older `[[load_case.load]]`.
    Older,
    /// `[[load_case.line]]`.
    Line,
    /// `[[load_case.structure.wind]]`.
    Wind,
    /// `[[load_case.structure.ice]]`.
    Ice,
    /// `[[load_case.structure.point]]`.
    Point,
    /// `[[load_case.structure.snow]]`.
    Snow,
}

impl Part {
    /// The first row number of the part's block (0: the older loads).
    pub fn base(self) -> usize {
        match self {
            Self::Older => 0,
            Self::Line => PART_BLOCK,
            Self::Wind => 2 * PART_BLOCK,
            Self::Ice => 3 * PART_BLOCK,
            Self::Point => 4 * PART_BLOCK,
            Self::Snow => 5 * PART_BLOCK,
        }
    }

    /// The row number of item `k` (from 0) of the part.
    pub fn number(self, k: usize) -> usize {
        self.base() + k + 1
    }

    /// The part and item (from 0) of row number `load`.
    pub fn of(load: usize) -> Option<(Self, usize)> {
        let part = match load.checked_sub(1)? / PART_BLOCK {
            0 => Self::Older,
            1 => Self::Line,
            2 => Self::Wind,
            3 => Self::Ice,
            4 => Self::Point,
            5 => Self::Snow,
            _ => return None,
        };
        Some((part, load - part.base() - 1))
    }

    /// The name of row number `load` in words: "load 2", "line 1",
    /// "wind 1", "ice 1", "point 3".
    pub fn row_name(load: usize) -> String {
        match Self::of(load) {
            Some((Self::Older, k)) => format!("load {}", k + 1),
            Some((Self::Line, k)) => format!("line {}", k + 1),
            Some((Self::Wind, k)) => format!("wind {}", k + 1),
            Some((Self::Ice, k)) => format!("ice {}", k + 1),
            Some((Self::Point, k)) => format!("point {}", k + 1),
            Some((Self::Snow, k)) => format!("snow {}", k + 1),
            None => format!("load {load}"),
        }
    }
}

/// What every line forces table taking a share of the spans' pull
/// (`back`, `ahead`) says of itself, after "NOT ON THE SAFE SIDE: " — in
/// its rows' notes, the case's sentence, the runs and the reports.
pub const SPAN_SHARE_ASSUMPTION: &str = "the back/ahead spans' horizontal pull taken as a share \
     of the solved pull is a load ASSUMPTION, not an equilibrium state — the real one-sided \
     pull, with the strings swinging and the wires' tensions changing, can be smaller or larger \
     (the exact treatment: solve the line with the one-sided condition)";

/// Line forces (part 1): a line result's forces at the attachment points
/// (unfactored, as the line sends them), a factor on each axis — and,
/// since version 4 (owner, 2026-11-10), the share of the back and of the
/// ahead spans' horizontal pull taken (`back`, `ahead`, percent). Per
/// point, before the factors:
///
/// force(T, L) = sent(T, L) − (1 − back/100)·pull_back(T, L)
///             − (1 − ahead/100)·pull_ahead(T, L); vertical as sent.
///
/// The pulls are the line's ([`LineQuantity::PULL_BACK`],
/// [`LineQuantity::PULL_AHEAD`]: the solved horizontal pull of each side's
/// spans along the span); the wind across the spans, the vertical load and
/// the jumpers stay. A load assumption, not an equilibrium state
/// ([`SPAN_SHARE_ASSUMPTION`]); it costs no solve.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LineForces {
    /// The line result (`storm — wind from the left`), as the line sent it.
    pub result: String,
    /// Factor on the vertical forces (down).
    #[serde(default = "one")]
    pub vertical: f64,
    /// Factor on the transverse forces (right of the line).
    #[serde(default = "one")]
    pub transverse: f64,
    /// Factor on the longitudinal forces (ahead).
    #[serde(default = "one")]
    pub longitudinal: f64,
    /// Only the points carrying these sets (empty: every point).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<u32>,
    /// Points (labels) whose line forces are left out (owner: works on
    /// the line with a conductor removed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off: Vec<String>,
    /// The share of the back spans' solved horizontal pull taken,
    /// percent (0 or more; none: 100, as solved; 0: none of it; above 100
    /// allowed, noted). Version 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<f64>,
    /// The same of the ahead spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ahead: Option<f64>,
}

impl LineForces {
    /// A line result's forces at every point, every factor 1.
    pub fn new(result: &str) -> Self {
        Self {
            result: result.to_owned(),
            vertical: 1.0,
            transverse: 1.0,
            longitudinal: 1.0,
            sets: Vec::new(),
            off: Vec::new(),
            back: None,
            ahead: None,
        }
    }

    /// Whether `back` or `ahead` is given, whatever the value (the file
    /// is then of version 4).
    pub fn uses_span_shares(&self) -> bool {
        self.back.is_some() || self.ahead.is_some()
    }

    /// Whether it changes the spans' pull (a share given that is not
    /// 100 %): a load assumption, said so wherever it shows
    /// ([`SPAN_SHARE_ASSUMPTION`]). 100 % is the force as solved.
    pub fn scales_span_pull(&self) -> bool {
        self.span_reductions() != [0.0; 2]
    }

    /// The part of the back and of the ahead spans' pull taken **off**
    /// the force sent: 1 − percent/100 each (none given: 0, nothing off).
    pub fn span_reductions(&self) -> [f64; 2] {
        [self.back, self.ahead].map(|p| p.map_or(0.0, |p| 1.0 - p / 100.0))
    }

    /// The factors in the table's order: transverse, longitudinal,
    /// vertical.
    pub fn factors(&self) -> [f64; 3] {
        [self.transverse, self.longitudinal, self.vertical]
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.result.trim().is_empty() {
            return Err("line forces without a result (the line result's name)".into());
        }
        for (what, f) in [
            ("vertical", self.vertical),
            ("transverse", self.transverse),
            ("longitudinal", self.longitudinal),
        ] {
            if !f.is_finite() {
                return Err(format!("{what} factor {f}"));
            }
        }
        for (what, p) in [("back", self.back), ("ahead", self.ahead)] {
            if let Some(p) = p
                && !(p.is_finite() && p >= 0.0)
            {
                return Err(format!(
                    "{what} = {p}: the share of the {what} spans' solved pull in percent, 0 or \
                     more (100: as solved, 0: none)"
                ));
            }
        }
        for (k, p) in self.off.iter().enumerate() {
            if p.trim().is_empty() {
                return Err("off: an empty point label".into());
            }
            if self.off[..k].contains(p) {
                return Err(format!("off: point {p} given twice"));
            }
        }
        Ok(())
    }
}

/// Where the wind on the body comes from (looking ahead along the line):
/// the four main directions and the four between them, at 45°, or any
/// angle (the line's load cases, version 5 of the line-values file; owner
/// 2026-11-03). Written as a name (`"left"`, `"ahead_right"`…) or the
/// angle in degrees the wind comes from: 0 from the right, 90 from ahead,
/// 180 from the left, 270 from behind (`from = 30.0`); older files, names
/// only, read as before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindFrom {
    /// From the left: blowing to the right (+x).
    Left,
    /// From the right: blowing to the left (−x).
    Right,
    /// From ahead: blowing back (−y).
    Ahead,
    /// From behind: blowing ahead (+y).
    Back,
    /// From ahead and the left, 45° between them.
    AheadLeft,
    /// From ahead and the right.
    AheadRight,
    /// From behind and the left.
    BackLeft,
    /// From behind and the right.
    BackRight,
    /// From this angle, degrees: 0 from the right, 90 from ahead, 180
    /// from the left, 270 from behind.
    Angle(f64),
}

impl Serialize for WindFrom {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Angle(a) => s.serialize_f64(*a),
            named => s.serialize_str(named.as_str()),
        }
    }
}

impl<'de> Deserialize<'de> for WindFrom {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(f64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Number(a) if a.is_finite() => Ok(Self::from_degrees(a)),
            Raw::Number(a) => Err(serde::de::Error::custom(format!(
                "from {a}: a direction's name or an angle in degrees"
            ))),
            Raw::Text(t) => Self::parse(&t).ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "from \"{t}\": left, right, ahead, back, ahead_left, ahead_right, back_left, \
                     back_right or an angle in degrees (0 from the right, 90 from ahead)"
                ))
            }),
        }
    }
}

impl WindFrom {
    /// Every direction, in the order offered.
    pub const ALL: [Self; 8] = [
        Self::Left,
        Self::Right,
        Self::Ahead,
        Self::Back,
        Self::AheadLeft,
        Self::AheadRight,
        Self::BackLeft,
        Self::BackRight,
    ];

    /// The direction of the angle `degrees` the wind comes from (0 from
    /// the right, 90 from ahead): one of the eight names when it is one
    /// of theirs (to 1e-9°), else [`WindFrom::Angle`] (in 0 to 360).
    pub fn from_degrees(degrees: f64) -> Self {
        let a = degrees.rem_euclid(360.0);
        let named = [
            (0.0, Self::Right),
            (45.0, Self::AheadRight),
            (90.0, Self::Ahead),
            (135.0, Self::AheadLeft),
            (180.0, Self::Left),
            (225.0, Self::BackLeft),
            (270.0, Self::Back),
            (315.0, Self::BackRight),
            (360.0, Self::Right),
        ];
        named
            .into_iter()
            .find(|(d, _)| (a - d).abs() < 1e-9)
            .map_or(Self::Angle(a), |(_, w)| w)
    }

    /// The angle it comes from, degrees (0 from the right, 90 from ahead).
    pub fn degrees(self) -> f64 {
        match self {
            Self::Right => 0.0,
            Self::AheadRight => 45.0,
            Self::Ahead => 90.0,
            Self::AheadLeft => 135.0,
            Self::Left => 180.0,
            Self::BackLeft => 225.0,
            Self::Back => 270.0,
            Self::BackRight => 315.0,
            Self::Angle(a) => a,
        }
    }

    /// The name in files (an angle: `"angle"`; it is written as its
    /// number).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Angle(_) => "angle",
            Self::Left => "left",
            Self::Right => "right",
            Self::Ahead => "ahead",
            Self::Back => "back",
            Self::AheadLeft => "ahead_left",
            Self::AheadRight => "ahead_right",
            Self::BackLeft => "back_left",
            Self::BackRight => "back_right",
        }
    }

    /// From its name in files.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.as_str() == s)
    }

    /// In words: "from the left", "from ahead left", "from 30°".
    pub fn words(self) -> String {
        match self {
            Self::Left => "from the left".into(),
            Self::Right => "from the right".into(),
            Self::Ahead => "from ahead".into(),
            Self::Back => "from behind".into(),
            Self::AheadLeft => "from ahead left".into(),
            Self::AheadRight => "from ahead right".into(),
            Self::BackLeft => "from behind left".into(),
            Self::BackRight => "from behind right".into(),
            Self::Angle(a) => format!("from {}° (0° from the right, 90° from ahead)", plain(a)),
        }
    }

    /// Where it blows to, a unit vector in the table's axes (transverse
    /// right, longitudinal ahead, vertical down: horizontal here).
    pub fn towards(self) -> [f64; 3] {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        match self {
            Self::Left => [1.0, 0.0, 0.0],
            Self::Right => [-1.0, 0.0, 0.0],
            Self::Ahead => [0.0, -1.0, 0.0],
            Self::Back => [0.0, 1.0, 0.0],
            Self::AheadLeft => [s, -s, 0.0],
            Self::AheadRight => [-s, -s, 0.0],
            Self::BackLeft => [s, s, 0.0],
            Self::BackRight => [-s, s, 0.0],
            Self::Angle(a) => {
                let r = a.to_radians();
                [-r.cos(), -r.sin(), 0.0]
            }
        }
    }

    /// Left and right swapped (ahead and back stay).
    pub fn mirrored(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::AheadLeft => Self::AheadRight,
            Self::AheadRight => Self::AheadLeft,
            Self::BackLeft => Self::BackRight,
            Self::BackRight => Self::BackLeft,
            Self::Angle(a) => Self::from_degrees(180.0 - a),
            other => other,
        }
    }
}

/// The wind on the structure's body: the member wind of the line's own
/// structure checks (faces, solidity, shielding with the section
/// library's wind factors) for one direction and pressure.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BodyWindLoad {
    /// Where it comes from.
    pub from: WindFrom,
    /// The pressure, Pa…
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, Stress>",
        serialize_with = "ser_pressure",
        skip_serializing_if = "Option::is_none"
    )]
    pub pressure: Option<f64>,
    /// …or the pressure of a line result's weather case (unfactored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_of: Option<String>,
    /// Factor.
    #[serde(default = "one")]
    pub factor: f64,
}

impl BodyWindLoad {
    fn check(&self) -> Result<(), String> {
        match (self.pressure, &self.pressure_of) {
            (Some(_), Some(_)) => return Err("give pressure or pressure_of, not both".into()),
            (None, None) => {
                return Err("give a pressure or pressure_of (a line result)".into());
            }
            (Some(p), None) if !(p.is_finite() && p >= 0.0) => {
                return Err(format!("pressure {p} Pa (0 or more)"));
            }
            (None, Some(r)) if r.trim().is_empty() => {
                return Err("pressure_of: an empty line result".into());
            }
            _ => {}
        }
        if !(self.factor.is_finite() && self.factor >= 0.0) {
            return Err(format!("factor {} (0 or more)", self.factor));
        }
        Ok(())
    }
}

/// Ice, or another weight, on members: a radial thickness of a density
/// all round each member's outline, or a load per metre; down.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IceLoad {
    /// Radial thickness, m (with a density)…
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, Length>",
        serialize_with = "ser_length",
        skip_serializing_if = "Option::is_none"
    )]
    pub thickness: Option<f64>,
    /// Its density, kg/m³.
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, Density>",
        serialize_with = "ser_density",
        skip_serializing_if = "Option::is_none"
    )]
    pub density: Option<f64>,
    /// …or a load per metre of member, N/m.
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, ForcePerLength>",
        serialize_with = "ser_per_metre",
        skip_serializing_if = "Option::is_none"
    )]
    pub load: Option<f64>,
    /// Members, by id, or `"all"`.
    #[serde(default = "all", skip_serializing_if = "is_all")]
    pub members: Pick,
    /// Factor.
    #[serde(default = "one")]
    pub factor: f64,
}

impl IceLoad {
    fn check(&self) -> Result<(), String> {
        match (self.thickness, self.density, self.load) {
            (Some(t), Some(r), None) => {
                if !(t.is_finite() && t > 0.0) {
                    return Err(format!("thickness {t} m (above 0)"));
                }
                if !(r.is_finite() && r > 0.0) {
                    return Err(format!("density {r} kg/m³ (above 0)"));
                }
            }
            (None, None, Some(w)) => {
                if !(w.is_finite() && w >= 0.0) {
                    return Err(format!("load {w} N/m (0 or more)"));
                }
            }
            (Some(_), None, None) => return Err("a thickness needs a density".into()),
            (None, Some(_), None) => return Err("a density needs a thickness".into()),
            (None, None, None) => {
                return Err("give a thickness and a density, or a load per metre".into());
            }
            _ => return Err("give a thickness and a density, or a load, not both".into()),
        }
        if matches!(&self.members, Pick::These(v) if v.is_empty()) {
            return Err("members: \"all\" or a list of member ids".into());
        }
        if !(self.factor.is_finite() && self.factor >= 0.0) {
            return Err(format!("factor {} (0 or more)", self.factor));
        }
        Ok(())
    }

    /// Whether it is on member `id`.
    pub fn on(&self, id: &str) -> bool {
        match &self.members {
            Pick::All => true,
            Pick::These(v) => v.iter().any(|m| m == id),
        }
    }

    /// Its weight per metre of a member whose outline has `perimeter`
    /// (m; needed for a thickness), N/m, before the factor: a thickness t
    /// all round the outline (its full perimeter P, an angle's inside
    /// faces too: owner, 2026-10-25, conservative) is ρ·g·(P·t + π·t²);
    /// a load per metre as given.
    pub fn weight_per_metre(&self, perimeter: Option<f64>) -> Result<f64, String> {
        match (self.thickness, self.density, self.load) {
            (_, _, Some(w)) => Ok(w),
            (Some(t), Some(rho), None) => {
                let p = perimeter.ok_or("no outline")?;
                Ok(
                    rho * ohl_core::units::STANDARD_GRAVITY
                        * (p * t + std::f64::consts::PI * t * t),
                )
            }
            _ => Err("no thickness and density, no load".into()),
        }
    }

    /// How much wider it makes a member for the wind, m: 2·t (a load per
    /// metre: none).
    pub fn widening(&self) -> f64 {
        self.thickness.map_or(0.0, |t| 2.0 * t)
    }
}

/// A force of the case at points.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PointLoad {
    /// The force, N.
    #[serde(deserialize_with = "de_force", serialize_with = "ser_force")]
    pub value: f64,
    /// Its direction (structure axes).
    pub direction: Direction,
    /// Points or wire points, by label, or `"all"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Pick>,
    /// Or the points carrying these sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<u32>,
    /// Or the points carrying these phases (the line's phase labels).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,
    /// A note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl PointLoad {
    /// The same as an older load (a `value` of the case), to evaluate it
    /// the same way.
    pub fn as_load(&self) -> LoadRecipe {
        LoadRecipe {
            value: Some(self.value),
            direction: Some(self.direction),
            points: self.points.clone(),
            sets: self.sets.clone(),
            phases: self.phases.clone(),
            note: self.note.clone(),
            ..LoadRecipe::default()
        }
    }
}

/// Snow on members (version 5 of the line-values file, from a line load
/// case's snow on the structures; owner 2026-11-03): a load per m² on
/// each member's plan — its width seen from above times its length
/// projected on the ground (a vertical member none) — down, the rule of
/// the line's own structure checks (`ohl_loads`, theory: Ice and snow on
/// the structure).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnowLoad {
    /// The snow load q, Pa (N/m² of plan), unfactored.
    #[serde(deserialize_with = "de_pressure", serialize_with = "ser_pa")]
    pub load: f64,
    /// Members, by id, or `"all"`.
    #[serde(default = "all", skip_serializing_if = "is_all")]
    pub members: Pick,
    /// Factor.
    #[serde(default = "one")]
    pub factor: f64,
}

impl SnowLoad {
    fn check(&self) -> Result<(), String> {
        if !(self.load.is_finite() && self.load >= 0.0) {
            return Err(format!("load {} Pa (0 or more)", self.load));
        }
        if matches!(&self.members, Pick::These(v) if v.is_empty()) {
            return Err("members: \"all\" or a list of member ids".into());
        }
        if !(self.factor.is_finite() && self.factor >= 0.0) {
            return Err(format!("factor {} (0 or more)", self.factor));
        }
        Ok(())
    }

    /// Whether it is on member `id`.
    pub fn on(&self, id: &str) -> bool {
        match &self.members {
            Pick::All => true,
            Pick::These(v) => v.iter().any(|m| m == id),
        }
    }
}

fn ser_pa<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{v} Pa"))
}

fn de_pressure<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    ohl_core::units::si::de::<_, Stress>(d)
}

/// Loads on the structure itself (part 2).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnLoads {
    /// Factor on the structure's self weight; given, it is the case's
    /// (the case's older `self_weight` must then be left at 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_weight: Option<f64>,
    /// The wind on the body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind: Vec<BodyWindLoad>,
    /// Ice or another weight on members.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ice: Vec<IceLoad>,
    /// Point loads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point: Vec<PointLoad>,
    /// Snow on members (version 5 of the line-values file: a line load
    /// case's snow on the structures).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snow: Vec<SnowLoad>,
}

impl OwnLoads {
    /// Nothing given.
    pub fn is_empty(&self) -> bool {
        self.self_weight.is_none()
            && self.wind.is_empty()
            && self.ice.is_empty()
            && self.point.is_empty()
            && self.snow.is_empty()
    }

    /// How much wider member `id` is for the wind, m: 2·t of each ice
    /// given as a thickness on it (owner, 2026-10-25: conservative).
    pub fn widening(&self, id: &str) -> f64 {
        self.ice
            .iter()
            .filter(|i| i.on(id))
            .map(IceLoad::widening)
            .sum()
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if let Some(s) = self.self_weight
            && !(s.is_finite() && s >= 0.0)
        {
            return Err(format!("structure self_weight {s} (0 or more)"));
        }
        for (k, w) in self.wind.iter().enumerate() {
            w.check()
                .map_err(|e| format!("structure wind {}: {e}", k + 1))?;
        }
        for (k, i) in self.ice.iter().enumerate() {
            i.check()
                .map_err(|e| format!("structure ice {}: {e}", k + 1))?;
        }
        for (k, s) in self.snow.iter().enumerate() {
            s.check()
                .map_err(|e| format!("structure snow {}: {e}", k + 1))?;
        }
        for (k, p) in self.point.iter().enumerate() {
            let at = |e: String| format!("structure point {}: {e}", k + 1);
            if !p.value.is_finite() {
                return Err(at("value not a number".into()));
            }
            if p.points.is_none() && p.sets.is_empty() && p.phases.is_empty() {
                return Err(at(
                    "say where: points (\"all\" or labels), sets or phases".into()
                ));
            }
        }
        for (part, n) in [
            ("wind", self.wind.len()),
            ("ice", self.ice.len()),
            ("point", self.point.len()),
            ("snow", self.snow.len()),
        ] {
            if n >= PART_BLOCK {
                return Err(format!(
                    "structure {part}: at most {} of them",
                    PART_BLOCK - 1
                ));
            }
        }
        Ok(())
    }
}

/// A number as the sentence writes it: at least one decimal (1.0, 1.35).
fn factor_text(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

/// A number without trailing zeros (650, 0.6, 2.5).
fn plain(x: f64) -> String {
    let t = format!("{:.3}", x);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.into() }
}

fn sets_text(sets: &[u32]) -> String {
    sets.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn force_text(n: f64) -> String {
    if n.abs() >= 1000.0 {
        format!("{} kN", plain(n / 1000.0))
    } else {
        format!("{} N", plain(n))
    }
}

/// Where a load at points acts, in words.
fn where_text(points: &Option<Pick>, sets: &[u32], phases: &[String]) -> String {
    let mut out = Vec::new();
    match points {
        Some(Pick::These(v)) => out.push(format!("at {}", v.join(", "))),
        Some(Pick::All) | None if sets.is_empty() && phases.is_empty() => {
            out.push("at every point".to_owned());
        }
        _ => {}
    }
    if !sets.is_empty() {
        out.push(format!(
            "at the points of set{} {}",
            if sets.len() > 1 { "s" } else { "" },
            sets_text(sets)
        ));
    }
    if !phases.is_empty() {
        out.push(format!("of phase {}", phases.join(", ")));
    }
    out.join(" ")
}

/// One line forces row in words: "storm — wind from the left: vertical ×
/// 1.35, transverse × 1.5, longitudinal × 1.0 at every point (without
/// A3)".
pub fn line_forces_text(l: &LineForces) -> String {
    let mut s = format!(
        "{}: vertical × {}, transverse × {}, longitudinal × {} {}",
        l.result,
        factor_text(l.vertical),
        factor_text(l.transverse),
        factor_text(l.longitudinal),
        if l.sets.is_empty() {
            "at every point".to_owned()
        } else {
            format!(
                "at the points of set{} {}",
                if l.sets.len() > 1 { "s" } else { "" },
                sets_text(&l.sets)
            )
        }
    );
    if !l.off.is_empty() {
        s.push_str(&format!(" (without {})", l.off.join(", ")));
    }
    if l.uses_span_shares() {
        s.push_str(&format!("; {}", span_share_text(l)));
    }
    s
}

/// The shares of the spans' pull a line forces table takes, in words:
/// "back spans' pull at 5 %, ahead spans' pull at 0 % of the solved pull"
/// (a share above 100 % said).
pub fn span_share_text(l: &LineForces) -> String {
    let side = |what: &str, p: Option<f64>| {
        let p = p.unwrap_or(100.0);
        format!(
            "{what} spans' pull at {} %{}",
            plain(p),
            if p > 100.0 { " (above 100 %)" } else { "" }
        )
    };
    format!(
        "{}, {} of the solved pull",
        side("back", l.back),
        side("ahead", l.ahead)
    )
}

/// A body wind in words: "wind on the body from the left, 650 Pa × 1.5".
pub fn wind_text(w: &BodyWindLoad) -> String {
    let pressure = match (&w.pressure, &w.pressure_of) {
        (Some(p), _) => format!("{} Pa", plain(*p)),
        (None, Some(r)) => format!("the pressure of {r}"),
        (None, None) => "no pressure".to_owned(),
    };
    format!(
        "wind on the body {}, {pressure} × {}",
        w.from.words(),
        factor_text(w.factor)
    )
}

/// An ice load in words: "ice 20 mm (900 kg/m³) on every member × 1.0".
pub fn ice_text(i: &IceLoad) -> String {
    let what = match (i.thickness, i.density, i.load) {
        (_, _, Some(w)) => format!("weight {} N/m", plain(w)),
        (Some(t), Some(r), None) => format!("ice {} mm ({} kg/m³)", plain(t * 1000.0), plain(r)),
        _ => "ice".to_owned(),
    };
    let on = match &i.members {
        Pick::All => "every member".to_owned(),
        Pick::These(v) if v.len() == 1 => format!("member {}", v[0]),
        Pick::These(v) => format!("members {}", v.join(", ")),
    };
    format!("{what} on {on} × {}", factor_text(i.factor))
}

/// A snow load in words: "snow 2.93 kPa on every member's plan × 1.35".
pub fn snow_text(s: &SnowLoad) -> String {
    let on = match &s.members {
        Pick::All => "every member's plan".to_owned(),
        Pick::These(v) if v.len() == 1 => format!("member {}'s plan", v[0]),
        Pick::These(v) => format!("the plan of members {}", v.join(", ")),
    };
    format!(
        "snow {} kPa on {on} × {}",
        plain(s.load / 1000.0),
        factor_text(s.factor)
    )
}

/// A point load in words: "2 kN down at A1".
pub fn point_text(p: &PointLoad) -> String {
    let d = match p.direction {
        Direction::Right => "to the right",
        Direction::Left => "to the left",
        Direction::Ahead => "ahead",
        Direction::Back => "back",
        Direction::Up => "up",
        Direction::Down => "down",
    };
    format!(
        "{} {d} {}",
        force_text(p.value),
        where_text(&p.points, &p.sets, &p.phases)
    )
}

/// What case `case` does, in words, its parts separated by " · ": the
/// line forces, the self weight, the wind on the body, the ice, the
/// point loads, and how many loads it has in the older form.
pub fn describe(case: &LoadCaseRecipe) -> String {
    let mut parts: Vec<String> = case.line.iter().map(line_forces_text).collect();
    parts.push(format!(
        "self weight × {}",
        factor_text(case.self_weight_factor())
    ));
    if let Some(s) = &case.structure {
        parts.extend(s.wind.iter().map(wind_text));
        parts.extend(s.ice.iter().map(ice_text));
        parts.extend(s.point.iter().map(point_text));
        parts.extend(s.snow.iter().map(snow_text));
    }
    if !case.loads.is_empty() {
        parts.push(format!(
            "{} load{} in the older form",
            case.loads.len(),
            if case.loads.len() > 1 { "s" } else { "" }
        ));
    }
    parts.join(" · ")
}

/// What [`convert_older`] made of a case.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// The case with what was exact in the new parts.
    pub case: LoadCaseRecipe,
    /// The older loads kept as they are (their number in the case before,
    /// from 1, and why), in words.
    pub kept: Vec<String>,
    /// What was converted, in words.
    pub done: Vec<String>,
}

/// The line result an older load's line case `name` is now (`weather:
/// storm` → `storm`); `results`: the results the line sends (empty: not
/// known, the name taken as it is). Err: the line sends that weather case
/// as two results (the wind from the left and from the right).
fn result_of(name: &str, results: &[String]) -> Result<String, String> {
    let plain = name.strip_prefix("weather: ").unwrap_or(name).to_owned();
    if results.is_empty() || results.contains(&plain) {
        return Ok(plain);
    }
    let sides = [
        format!("{plain} — wind from the left"),
        format!("{plain} — wind from the right"),
    ];
    if sides.iter().all(|s| results.contains(s)) {
        return Err(format!(
            "the line sends \"{plain}\" twice now ({} and {}): choose one",
            sides[0], sides[1]
        ));
    }
    Ok(plain)
}

/// An older case's loads in the new parts where that is exact; the rest
/// stays as it is (said). Exact: the line's unfactored transverse,
/// longitudinal and vertical forces of one line case at the same points
/// (every point, or the same sets), each axis with its factor, no share,
/// side, direction or phase — one line forces row per line case and
/// points (an axis given twice: its factors added; an axis not given:
/// factor 0); a force of the case at points (`value`) — a point load; the
/// case's `self_weight` — the structure's self weight. `results`: the
/// line results the line sends (`weather: X` becomes `X`; empty: the
/// names as they are). Pressures and the line's other values stay.
pub fn convert_older(case: &LoadCaseRecipe, results: &[String]) -> Converted {
    let mut out = case.clone();
    out.loads.clear();
    let mut kept = Vec::new();
    let mut done = Vec::new();
    let mut own = out.structure.clone().unwrap_or_default();
    for (k, l) in case.loads.iter().enumerate() {
        let number = k + 1;
        let plain_where = l.one_sided.is_none()
            && l.side.is_none()
            && l.direction.is_none()
            && l.phases.is_empty()
            && l.members.is_none()
            && l.pressure.is_none();
        let axis = match l.what {
            Some(LineQuantity::Transverse) => Some(0),
            Some(LineQuantity::Longitudinal) => Some(1),
            Some(LineQuantity::Vertical) => Some(2),
            _ => None,
        };
        let every = matches!(l.points, Some(Pick::All) | None);
        if let (Some(axis), true, true) = (axis, plain_where, every) {
            let Some(lc) = l.line_case.as_ref().or(case.line_case.as_ref()) else {
                kept.push(format!("load {number}: no line case named"));
                out.loads.push(l.clone());
                continue;
            };
            match result_of(lc, results) {
                Ok(result) => {
                    let row = match out
                        .line
                        .iter_mut()
                        .position(|r| r.result == result && r.sets == l.sets)
                    {
                        Some(i) => &mut out.line[i],
                        None => {
                            let mut r = LineForces::new(&result);
                            r.vertical = 0.0;
                            r.transverse = 0.0;
                            r.longitudinal = 0.0;
                            r.sets = l.sets.clone();
                            out.line.push(r);
                            out.line.last_mut().expect("pushed")
                        }
                    };
                    match axis {
                        0 => row.transverse += l.factor,
                        1 => row.longitudinal += l.factor,
                        _ => row.vertical += l.factor,
                    }
                    done.push(format!("load {number}: line forces of {result}"));
                }
                Err(why) => {
                    kept.push(format!("load {number}: {why}"));
                    out.loads.push(l.clone());
                }
            }
            continue;
        }
        if let (None, Some(v), Some(d), None, None) =
            (l.what, l.value, l.direction, l.one_sided, l.side)
            && l.members.is_none()
        {
            own.point.push(PointLoad {
                value: v * l.factor,
                direction: d,
                points: l.points.clone(),
                sets: l.sets.clone(),
                phases: l.phases.clone(),
                note: l.note.clone(),
            });
            done.push(format!("load {number}: a point load"));
            continue;
        }
        let why = match l.what {
            Some(q) if q.is_pressure() => {
                "the line's wind as a pressure: the wind on the body is the member wind now \
                 (not the same rule)"
            }
            Some(LineQuantity::TransverseFactored)
            | Some(LineQuantity::LongitudinalFactored)
            | Some(LineQuantity::VerticalFactored) => {
                "a factored line value: the line sends its forces unfactored now"
            }
            Some(_) if !every => "on chosen points (line forces take every point or sets)",
            Some(_) if !plain_where => "a share, side, direction or phase",
            Some(_) => "a line value the line forces do not take",
            None if l.pressure.is_some() => {
                "a pressure on the members' area: the wind on the body is the member wind now \
                 (not the same rule)"
            }
            None => "a share or side",
        };
        kept.push(format!("load {number}: {why}"));
        out.loads.push(l.clone());
    }
    if case.self_weight != 1.0 && own.self_weight.is_none() {
        own.self_weight = Some(case.self_weight);
        out.self_weight = 1.0;
        done.push("self weight".to_owned());
    }
    out.structure = (!own.is_empty()).then_some(own);
    // The case's line case: kept only while an older load takes it.
    if !out
        .loads
        .iter()
        .any(|l| l.what.is_some() && l.line_case.is_none())
    {
        out.line_case = None;
    }
    Converted {
        case: out,
        kept,
        done,
    }
}

/// `text` with the words "left" and "right" swapped.
fn swap_sides(text: &str) -> String {
    const MARK: &str = "\u{0}";
    text.replace("left", MARK)
        .replace("right", "left")
        .replace(MARK, "right")
}

/// A copy of case `case` with the wind from the other side: every
/// "left" turned "right" and back in its name, its line results and the
/// line results its body wind takes the pressure of, and its body wind
/// directions mirrored (ahead and back stay). A name that does not change
/// gets " (mirrored)". The older loads and the points turned off stay as
/// they are.
pub fn mirror_wind(case: &LoadCaseRecipe) -> LoadCaseRecipe {
    let mut out = case.clone();
    out.name = swap_sides(&case.name);
    if out.name == case.name {
        out.name = format!("{} (mirrored)", case.name);
    }
    out.description = swap_sides(&case.description);
    for l in &mut out.line {
        l.result = swap_sides(&l.result);
    }
    if let Some(s) = &mut out.structure {
        for w in &mut s.wind {
            w.from = w.from.mirrored();
            if let Some(r) = &mut w.pressure_of {
                *r = swap_sides(r);
            }
        }
    }
    out
}
