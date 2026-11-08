// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load cases and the loads on the structures (Phase 4).
//!
//! A **load case** ([`LoadCase`]) is a weather case, a wire condition
//! (initial, after creep, after load, max sag) and load factors on the
//! vertical, transverse and longitudinal loads (and a strength factor for
//! the capacity checks). Load cases are data (`.ohllc` files): a design
//! code's combinations and partial factors are written there — by a code
//! plugin or by hand from the standard (OpenOHL ships no code tables).
//!
//! [`structure_loads`] solves every section in each load case (finite
//! elements: strings swing, each span its own tension) and gives, at every
//! attachment point, the force the wires and strings put on the structure:
//! in the line's axes, and in the structure's (transverse to the right of
//! the line, longitudinal ahead, vertical down), unfactored and factored.
//! A section or jumper that does not solve in a case is listed per
//! structure it ends at, hangs on or is held by ([`CaseLoads::missing`]):
//! that structure's loads in that case are **incomplete** (they lack the
//! wire's forces) and its usage checks say so
//! ([`capacity::StructureUsage::status`]).
//!
//! ```toml
//! format = "ohl-load-cases"
//! version = 1
//!
//! [[case]]
//! name = "wind 90°"
//! weather = "strong wind"
//! condition = "initial"
//! vertical = 1.35
//! transverse = 1.5
//! longitudinal = 1.0
//! strength = 1.0
//! ```
//!
//! **The structures' own wind** may differ from the wires' (design codes
//! treat them apart): a case's optional `[case.structure]` table gives
//! the wind pressure on the structure bodies (`wind_pressure`, instead of
//! the weather case's) and the factor on it (`wind_factor`, instead of
//! `transverse`), each on its own (version 2; [`StructureWind`],
//! [`LoadCase::structure_wind`]). Absent, the structures take the weather
//! case's pressure and the transverse factor, as before. The wires and
//! strings always take the weather case's pressure and `transverse`.
//!
//! ```toml
//! [[case]]
//! name = "wind on the towers"
//! weather = "high wind"
//! transverse = 1.4
//!
//! [case.structure]
//! wind_pressure = "900 Pa"   # on the structure bodies (the wires: the weather case's)
//! wind_factor = 1.2          # on that pressure (the wires: transverse, 1.4)
//! ```
//!
//! **Version 4** (owner 2026-11-03): removed conductors
//! (`[[case.removed]]`, [`RemovedWire`]), several broken wires broken
//! together (`[[case.broken]]`), the wires' wind side (`wind_from`), the
//! structures' local conditions (`use_local`), and in `[case.structure]`
//! the wind's direction ([`WindDirection`]), ice and snow on the
//! structures ([`StructureOptions`]); each structure takes only the cases
//! of its set (`ohl_line::Line::load_cases_for`). [`LoadCases::to_toml`]
//! writes a file at the lowest version its cases need.

mod analysis;
mod body;
pub mod capacity;
mod options;
mod write;

pub use body::add_structure_loads;
pub use options::{
    DEFAULT_SNOW_EXPOSURE, DEFAULT_SNOW_SHAPE, DEFAULT_SNOW_THERMAL, IceOn, RemovedWire, SnowOn,
    StructureIce, StructureOptions, WindDirection, snow_load,
};

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ohl_core::budget::Budget;
use ohl_line::condition::{Calculation, Condition, Method};
use ohl_line::{Line, Point};
use ohl_weather::WeatherCase;
use serde::Deserialize;

/// `format` of a load case file.
pub const LOAD_CASE_FILE_FORMAT: &str = "ohl-load-cases";
/// Newest format version read. Version 4 (2026-11-03) adds several
/// broken wires per case (`[[case.broken]]`), removed conductors
/// (`[[case.removed]]`, [`RemovedWire`]), the wires' wind side
/// (`wind_from`), `use_local`, and in `[case.structure]` the wind's
/// direction, ice and snow on the structures ([`StructureOptions`]);
/// version 3 a case's broken wire (`[case.broken]`, [`BrokenWire`]);
/// version 2 a case's `[case.structure]` table ([`StructureWind`]); each
/// written only when a case has one ([`LoadCases::written_version`]);
/// older files read as before.
pub const LOAD_CASE_FILE_VERSION: u32 = 4;
/// File extension of load case files.
pub const LOAD_CASE_FILE_EXTENSION: &str = "ohllc";

fn one() -> f64 {
    1.0
}

/// A wire condition, as written in files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireCondition {
    /// As strung.
    #[default]
    Initial,
    /// After creep.
    Creep,
    /// After the heaviest load.
    Load,
    /// After creep or load, whichever sags more.
    MaxSag,
}

impl WireCondition {
    /// The sections' condition.
    pub fn condition(self) -> Condition {
        match self {
            Self::Initial => Condition::Initial,
            Self::Creep => Condition::Creep,
            Self::Load => Condition::Load,
            Self::MaxSag => Condition::MaxSag,
        }
    }

    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Creep => "creep",
            Self::Load => "load",
            Self::MaxSag => "max_sag",
        }
    }

    /// From its name in files.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == name)
    }

    /// Every condition, in words' order.
    pub const ALL: [Self; 4] = [Self::Initial, Self::Creep, Self::Load, Self::MaxSag];

    /// In words.
    pub fn words(self) -> &'static str {
        match self {
            Self::Initial => "as strung",
            Self::Creep => "after creep",
            Self::Load => "after the heaviest load",
            Self::MaxSag => "after creep or load, whichever sags more",
        }
    }
}

/// A load case: a weather case, a wire condition, load factors.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "LoadCaseFile")]
pub struct LoadCase {
    /// Unique name.
    pub name: String,
    /// What it stands for (e.g. a code clause).
    pub description: String,
    /// The weather case, by name.
    pub weather: String,
    /// The wires' condition.
    pub condition: WireCondition,
    /// Factor on the vertical loads.
    pub vertical: f64,
    /// Factor on the transverse loads.
    pub transverse: f64,
    /// Factor on the longitudinal loads.
    pub longitudinal: f64,
    /// Strength (resistance) factor of the capacity checks.
    pub strength: f64,
    /// Tags (code, limit state…).
    pub tags: Vec<String>,
    /// The wind on the structures' own bodies when it differs from the
    /// wires' (version 2; none: the weather case's pressure and the
    /// transverse factor).
    pub structure: Option<StructureWind>,
    /// A broken wire (version 3, roadmap O9): the section solved with the
    /// wire of one of its spans gone, or of every span one at a time
    /// ([`BrokenWire`]); none: every wire whole. With several broken
    /// wires (version 4, `[[case.broken]]`) the first of them, the others
    /// in [`LoadCase::broken_also`] ([`LoadCase::broken_wires`] gives
    /// them all).
    pub broken: Option<BrokenWire>,
    /// The broken wires after the first (version 4; owner 2026-11-03):
    /// all broken together with [`LoadCase::broken`] in one solve per
    /// section.
    pub broken_also: Vec<BrokenWire>,
    /// Removed conductors (`[[case.removed]]`, version 4; owner
    /// 2026-11-03): sections whose wire is not there in this case
    /// ([`RemovedWire`]).
    pub removed: Vec<RemovedWire>,
    /// The side the wind on the wires comes from in this case (version 4;
    /// none: the weather case's). Only meaningful when the weather case
    /// has wind (a note says so when it has none).
    pub wind_from: Option<ohl_weather::WindSide>,
    /// Take the structures' local conditions (version 4): the wires
    /// solved with the spans' local wind and ice for strength
    /// (`ohl_line::local::localized`, `Purpose::Strength`), the structure
    /// bodies' wind times each structure's `wind_static`.
    pub use_local: bool,
    /// The version-4 values for the structure bodies: the wind's
    /// direction, ice and snow on them ([`StructureOptions`]).
    pub on_structure: StructureOptions,
}

/// A load case as files give it (versions 1 to 4).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadCaseFile {
    name: String,
    #[serde(default)]
    description: String,
    weather: String,
    #[serde(default)]
    condition: WireCondition,
    #[serde(default = "one")]
    vertical: f64,
    #[serde(default = "one")]
    transverse: f64,
    #[serde(default = "one")]
    longitudinal: f64,
    #[serde(default = "one")]
    strength: f64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    structure: Option<StructureFile>,
    #[serde(default)]
    broken: Option<OneOrMany<BrokenWire>>,
    #[serde(default)]
    removed: Vec<RemovedWire>,
    #[serde(default)]
    wind_from: Option<ohl_weather::WindSide>,
    #[serde(default)]
    use_local: bool,
}

/// The `[case.structure]` table as files give it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructureFile {
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, ohl_core::units::Stress>"
    )]
    wind_pressure: Option<f64>,
    #[serde(default)]
    wind_factor: Option<f64>,
    #[serde(default)]
    direction: Option<WindDirection>,
    #[serde(default)]
    ice: Option<StructureIce>,
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, ohl_core::units::Density>"
    )]
    ice_density: Option<f64>,
    #[serde(default)]
    snow: bool,
    #[serde(default)]
    snow_shape: Option<f64>,
    #[serde(default)]
    snow_exposure: Option<f64>,
    #[serde(default)]
    snow_thermal: Option<f64>,
}

/// One table (`[case.broken]`, version 3) or several (`[[case.broken]]`,
/// version 4).
struct OneOrMany<T>(Vec<T>);

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = toml::Value::deserialize(d)?;
        match v {
            toml::Value::Array(items) => items
                .into_iter()
                .map(|i| i.try_into::<T>().map_err(serde::de::Error::custom))
                .collect::<Result<Vec<T>, D::Error>>()
                .map(OneOrMany),
            one => one
                .try_into::<T>()
                .map(|t| OneOrMany(vec![t]))
                .map_err(serde::de::Error::custom),
        }
    }
}

impl From<LoadCaseFile> for LoadCase {
    fn from(f: LoadCaseFile) -> Self {
        let mut broken = f.broken.map_or_else(Vec::new, |b| b.0).into_iter();
        let (structure, on_structure) = match f.structure {
            None => (None, StructureOptions::default()),
            Some(s) => (
                Some(StructureWind {
                    wind_pressure: s.wind_pressure,
                    wind_factor: s.wind_factor,
                }),
                StructureOptions {
                    direction: s.direction,
                    ice: s.ice,
                    ice_density: s.ice_density,
                    snow: s.snow,
                    snow_shape: s.snow_shape,
                    snow_exposure: s.snow_exposure,
                    snow_thermal: s.snow_thermal,
                },
            ),
        };
        Self {
            name: f.name,
            description: f.description,
            weather: f.weather,
            condition: f.condition,
            vertical: f.vertical,
            transverse: f.transverse,
            longitudinal: f.longitudinal,
            strength: f.strength,
            tags: f.tags,
            structure,
            broken: broken.next(),
            broken_also: broken.collect(),
            removed: f.removed,
            wind_from: f.wind_from,
            use_local: f.use_local,
            on_structure,
        }
    }
}

/// A load case's broken wire (`[case.broken]`, version 3; roadmap O9,
/// owner 2026-10-19 and 2026-10-25): in that case the section `section`
/// is solved with the wire of span `span` gone
/// (`ohl_line::solve::solve_section_broken`), the other wires as they
/// are. The wire is named by its section and span; `set`, when given, is
/// the set of the section's points at the span's ends (the wire of that
/// set in that span, as the structures number their wire points): a
/// check that the span is the wire meant, refused when neither end has
/// it. A bundle's broken wire is its whole phase (every subconductor in
/// that span). `span = "every"`: a case per span, one at a time, and for
/// each structure point the largest of each force component (by size,
/// its sign kept) with the span giving it — an envelope, conservative for
/// the allowable-loads check only (the components may come from different
/// spans); the finite-element and pole checks take each break on its own
/// ([`CaseLoads::broken_each`]).
///
/// ```toml
/// [case.broken]
/// section = "S3"
/// span = 2        # 1 = the section's first span; or "every"
/// set = 1         # optional
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokenWire {
    /// The section, by id.
    pub section: String,
    /// The span, or every span one at a time.
    pub span: BrokenSpan,
    /// The set of the wire at the span's ends (a check; none: not
    /// checked).
    #[serde(default)]
    pub set: Option<u32>,
}

/// Which span of a [`BrokenWire`] is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenSpan {
    /// Span `n`, counted from 1 (the section's first span is 1).
    One(usize),
    /// Every span of the section, one at a time (the worst kept).
    Every,
}

impl<'de> Deserialize<'de> for BrokenSpan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(i64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Number(n) if n >= 1 => Ok(Self::One(n as usize)),
            Raw::Number(n) => Err(serde::de::Error::custom(format!(
                "span {n}: spans are counted from 1"
            ))),
            Raw::Text(t) if t == "every" => Ok(Self::Every),
            Raw::Text(t) => Err(serde::de::Error::custom(format!(
                "span \"{t}\": a span number (from 1) or \"every\""
            ))),
        }
    }
}

impl BrokenWire {
    /// In words for names and notes: `S3 span 2 set 1 broken`,
    /// `S3 every span broken, one at a time`.
    pub fn describe(&self) -> String {
        let set = self.set.map_or(String::new(), |s| format!(" set {s}"));
        match self.span {
            BrokenSpan::One(n) => format!("{} span {n}{set} broken", self.section),
            BrokenSpan::Every => {
                format!("{}{set} every span broken, one at a time", self.section)
            }
        }
    }

    /// The spans broken in `line` (0 = first): the one, or every span of
    /// the section. Err: no such section or span, or the set given at
    /// neither end of the span.
    pub fn spans(&self, line: &Line) -> Result<Vec<usize>, String> {
        let s = line
            .sections
            .iter()
            .find(|s| s.id == self.section)
            .ok_or_else(|| format!("broken wire: no section {}", self.section))?;
        let n = s.points.len().saturating_sub(1);
        let spans: Vec<usize> = match self.span {
            BrokenSpan::One(k) if (1..=n).contains(&k) => vec![k - 1],
            BrokenSpan::One(k) => {
                return Err(format!(
                    "broken wire: section {} has no span {k} (it has {n})",
                    self.section
                ));
            }
            BrokenSpan::Every => (0..n).collect(),
        };
        if let Some(set) = self.set {
            for &k in &spans {
                if s.points[k].set != set && s.points[k + 1].set != set {
                    return Err(format!(
                        "broken wire: span {} of section {} is not of set {set} (its ends: sets \
                         {} and {})",
                        k + 1,
                        self.section,
                        s.points[k].set,
                        s.points[k + 1].set
                    ));
                }
            }
        }
        Ok(spans)
    }
}

/// A load case's own values for the wind on the structure bodies
/// (`[case.structure]`, version 2): each one given replaces the line's
/// value for the structures only — the wires and strings keep the weather
/// case's pressure and the transverse factor. A design code that treats
/// the wind on pylons apart from the wind on the wires writes them (a
/// design-code plugin in the `.ohllc` text it generates). A gust or
/// structure factor of a code goes into the pressure or the factor;
/// drag (force) coefficients stay with the structure (its wind bands,
/// its members' sections, its pole).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureWind {
    /// The wind pressure on the structure bodies, Pa (none: the weather
    /// case's `wind_pressure`).
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, ohl_core::units::Stress>"
    )]
    pub wind_pressure: Option<f64>,
    /// The factor on that pressure (none: the case's `transverse`
    /// factor).
    #[serde(default)]
    pub wind_factor: Option<f64>,
}

impl StructureWind {
    /// Neither value given.
    pub fn is_empty(&self) -> bool {
        self.wind_pressure.is_none() && self.wind_factor.is_none()
    }
}

/// The wind on a structure's own body in a load case, as every structure
/// check takes it ([`LoadCase::structure_wind`]): the wind bands of the
/// usage, the member wind of the finite-element check, the wind on a
/// pole, and the line cases in the structure application.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyWind {
    /// Pressure, Pa (unfactored).
    pub pressure: f64,
    /// Factor on it.
    pub factor: f64,
    /// Where it blows, across the line in the structure's axes: −1 to the
    /// left (−x, the wind from the right), +1 to the right. With a
    /// direction along or at an angle to the line ([`BodyWind::direction`]):
    /// the transverse part of where it blows ([`BodyWind::towards_xy`]
    /// gives both parts).
    pub towards: f64,
    /// Where the wind comes from (version 4, `[case.structure]
    /// direction`; [`WindDirection`]: 0° from the right, 90° from
    /// ahead); by default the side of the wind on the wires.
    pub direction: WindDirection,
    /// The pressure is the case's own (`[case.structure]`), not the
    /// weather case's.
    pub own_pressure: bool,
    /// The factor is the case's own, not its transverse factor.
    pub own_factor: bool,
}

impl Default for BodyWind {
    /// No wind (pressure 0, factor 1, to the left).
    fn default() -> Self {
        Self {
            pressure: 0.0,
            factor: 1.0,
            towards: -1.0,
            direction: WindDirection::Right,
            own_pressure: false,
            own_factor: false,
        }
    }
}

impl BodyWind {
    /// Pressure × factor, Pa: the design pressure on the body.
    pub fn factored(&self) -> f64 {
        self.pressure * self.factor
    }

    /// Where it blows, a unit vector in the structure's axes (x right of
    /// the line, y ahead).
    pub fn towards_xy(&self) -> [f64; 2] {
        self.direction.towards()
    }

    /// It blows along the line, or at an angle to it (not straight
    /// across).
    pub fn has_longitudinal(&self) -> bool {
        self.towards_xy()[1].abs() > 1e-12
    }

    /// The case gives a value of its own for the structures.
    pub fn is_own(&self) -> bool {
        self.own_pressure || self.own_factor
    }

    /// Where the values come from, for notes: empty when both are the
    /// line's (the weather case's pressure, the transverse factor).
    pub fn origin(&self) -> &'static str {
        match (self.own_pressure, self.own_factor) {
            (true, true) => "the load case's own pressure and factor for the structures",
            (true, false) => "the load case's own pressure for the structures",
            (false, true) => "the load case's own factor for the structures",
            (false, false) => "",
        }
    }
}

/// The name of the load case of weather case `weather` on its own
/// ([`LoadCase::from_weather`]).
pub const WEATHER_CASE_PREFIX: &str = "weather: ";

/// The tag of a load case made of a weather case on its own.
pub const WEATHER_CASE_TAG: &str = "weather case";

impl LoadCase {
    /// Weather case `weather` on its own as a load case (the line's
    /// weather cases sent to the structures, Criteria › Weather cases for
    /// structures): named `weather: <name>`, the wires as strung
    /// (initial), every factor 1 (strength too), tagged
    /// [`WEATHER_CASE_TAG`]. Load cases (`.ohllc`) add factors and
    /// combinations on top.
    pub fn from_weather(weather: &str) -> Self {
        Self {
            name: format!("{WEATHER_CASE_PREFIX}{weather}"),
            description: format!("weather case \"{weather}\", factors 1"),
            weather: weather.to_owned(),
            condition: WireCondition::Initial,
            vertical: 1.0,
            transverse: 1.0,
            longitudinal: 1.0,
            strength: 1.0,
            tags: vec![WEATHER_CASE_TAG.to_owned()],
            structure: None,
            broken: None,
            broken_also: Vec::new(),
            removed: Vec::new(),
            wind_from: None,
            use_local: false,
            on_structure: StructureOptions::default(),
        }
    }

    /// Every broken wire of the case: [`LoadCase::broken`] and
    /// [`LoadCase::broken_also`], in order.
    pub fn broken_wires(&self) -> impl Iterator<Item = &BrokenWire> {
        self.broken.iter().chain(&self.broken_also)
    }

    /// Whether a wire of the case is broken.
    pub fn has_broken(&self) -> bool {
        self.broken.is_some() || !self.broken_also.is_empty()
    }

    /// Sets the broken wires (the first in [`LoadCase::broken`]).
    pub fn set_broken(&mut self, wires: Vec<BrokenWire>) {
        let mut it = wires.into_iter();
        self.broken = it.next();
        self.broken_also = it.collect();
    }

    /// The weather case `weather` as this case takes it for the wires:
    /// its wind from [`LoadCase::wind_from`] when given.
    pub fn wire_weather(&self, weather: &WeatherCase) -> WeatherCase {
        match self.wind_from {
            Some(side) if side != weather.wind_from => WeatherCase {
                wind_from: side,
                ..weather.clone()
            },
            _ => weather.clone(),
        }
    }

    /// Whether the case uses anything of file version 4.
    pub fn uses_version_4(&self) -> bool {
        !self.broken_also.is_empty()
            || !self.removed.is_empty()
            || self.wind_from.is_some()
            || self.use_local
            || !self.on_structure.is_empty()
    }

    /// The wind on the structures' own bodies in this case with weather
    /// case `weather` (the one it names): the pressure of its
    /// `[case.structure]` table, else the weather case's; the factor
    /// there, else the transverse factor; the side from the weather case.
    /// One rule for the line's structure checks and the structure
    /// application.
    ///
    /// Version 4: the side is the case's `wind_from` when given, and the
    /// direction `[case.structure] direction` when given
    /// ([`BodyWind::direction`]).
    pub fn structure_wind(&self, weather: &WeatherCase) -> BodyWind {
        let own = self.structure.unwrap_or_default();
        let side = self.wind_from.unwrap_or(weather.wind_from);
        let direction = self
            .on_structure
            .direction
            .unwrap_or(WindDirection::of_side(side));
        BodyWind {
            pressure: own
                .wind_pressure
                .unwrap_or_else(|| weather.wind_pressure.si()),
            factor: own.wind_factor.unwrap_or(self.transverse),
            towards: direction.towards()[0],
            direction,
            own_pressure: own.wind_pressure.is_some(),
            own_factor: own.wind_factor.is_some(),
        }
    }

    /// [`LoadCase::structure_wind`] at structure `s`: with
    /// [`LoadCase::use_local`], the pressure times its local
    /// `wind_static` (none given: as the case).
    pub fn structure_wind_at(
        &self,
        weather: &WeatherCase,
        s: Option<&ohl_line::Structure>,
    ) -> BodyWind {
        let mut body = self.structure_wind(weather);
        if self.use_local
            && let Some(f) = s.and_then(|s| s.local.wind_static)
        {
            body.pressure *= f;
        }
        body
    }

    /// The ice on structure `s` in this case with weather case `weather`
    /// ([`StructureOptions::ice`]): a thickness given, or with `ice =
    /// true` the structure's local `structure_ice`; with `ice = false`
    /// none. Not given (owner 2026-11-04: "yes apply local ice too"): a
    /// case taking the local conditions ([`LoadCase::use_local`], or a
    /// check's switch, [`with_local`]) whose weather case has ice
    /// ([`WeatherCase::has_ice`]) puts the structure's local
    /// `structure_ice` on it ([`LoadCase::takes_local_ice`]); else none.
    /// None too where the structure gives no local ice, or a thickness 0.
    /// Err: ice to put and no density — neither the case's `ice_density`
    /// nor the weather case's (none is made up).
    pub fn structure_ice_at(
        &self,
        weather: &WeatherCase,
        s: Option<&ohl_line::Structure>,
    ) -> Result<Option<IceOn>, String> {
        let local = || s.and_then(|s| s.local.structure_ice);
        let thickness = match self.on_structure.ice {
            Some(StructureIce::NoIce) => return Ok(None),
            Some(StructureIce::Thickness(t)) => t,
            Some(StructureIce::Local) => match local() {
                Some(t) => t,
                None => return Ok(None),
            },
            None if self.takes_local_ice(weather) => match local() {
                Some(t) => t,
                None => return Ok(None),
            },
            None => return Ok(None),
        };
        if thickness <= 0.0 {
            return Ok(None);
        }
        let density = self.ice_density(weather)?;
        Ok(Some(IceOn { thickness, density }))
    }

    /// Whether this case puts the structures' local ice on them without
    /// saying so: it gives no `[case.structure] ice`, takes the local
    /// conditions and its weather case has ice (owner 2026-11-04).
    pub fn takes_local_ice(&self, weather: &WeatherCase) -> bool {
        self.on_structure.ice.is_none() && self.use_local && weather.has_ice()
    }

    /// Whether this case may put ice or snow on the structures: ice or
    /// snow given, or the local ice taken ([`LoadCase::takes_local_ice`]).
    pub fn ices_or_snows(&self, weather: &WeatherCase) -> bool {
        matches!(
            self.on_structure.ice,
            Some(StructureIce::Local | StructureIce::Thickness(_))
        ) || self.on_structure.snow
            || self.takes_local_ice(weather)
    }

    /// The density of the ice on the structures: the case's
    /// `ice_density`, else the weather case's. Err: neither gives one
    /// (none is made up).
    pub fn ice_density(&self, weather: &WeatherCase) -> Result<f64, String> {
        self.on_structure
            .ice_density
            .or(weather.ice_density.map(|d| d.si()))
            .ok_or_else(|| {
                if self.takes_local_ice(weather) {
                    format!(
                        "load case {}: the structures' local ice (the case takes the local \
                         conditions and weather case \"{}\" has ice) needs a density: give \
                         [case.structure] ice_density, the weather case's ice density, or \
                         ice = false for no ice on the structures (none is made up)",
                        self.name, weather.name
                    )
                } else {
                    format!(
                        "load case {}: ice on the structures needs a density: give \
                         [case.structure] ice_density (weather case \"{}\" gives none)",
                        self.name, weather.name
                    )
                }
            })
    }

    /// The snow on structure `s` in this case ([`StructureOptions::snow`]):
    /// its snow on the ground ([`ohl_line::local::LocalConditions::snow`])
    /// and μ C_e C_T; none without snow or without the snow on the ground
    /// there.
    pub fn structure_snow_at(&self, s: Option<&ohl_line::Structure>) -> Option<SnowOn> {
        if !self.on_structure.snow {
            return None;
        }
        let ground = s?.local.snow()?;
        Some(SnowOn {
            ground,
            factor: self.on_structure.snow_factor(),
        })
    }

    /// Checks the factors.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a load case without a name".into());
        }
        for (what, f) in [
            ("vertical", self.vertical),
            ("transverse", self.transverse),
            ("longitudinal", self.longitudinal),
            ("strength", self.strength),
        ] {
            if !(f.is_finite() && (0.0..=100.0).contains(&f)) {
                return Err(format!("{}: {what} factor {f} (0 to 100)", self.name));
            }
        }
        if self.strength == 0.0 {
            return Err(format!("{}: strength factor 0", self.name));
        }
        let mut every = None;
        let mut seen: Vec<(&str, BrokenSpan)> = Vec::new();
        for b in self.broken_wires() {
            if b.section.trim().is_empty() {
                return Err(format!("{}: a broken wire without a section", self.name));
            }
            if seen.contains(&(b.section.as_str(), b.span)) {
                return Err(format!("{}: {} given twice", self.name, b.describe()));
            }
            seen.push((b.section.as_str(), b.span));
            if b.span == BrokenSpan::Every {
                if every.is_some() {
                    return Err(format!(
                        "{}: span = \"every\" in one broken wire only per case (each span of \
                         that section one at a time)",
                        self.name
                    ));
                }
                every = Some(b.section.as_str());
            }
        }
        if let Some(sec) = every
            && self.broken_wires().filter(|b| b.section == sec).count() > 1
        {
            return Err(format!(
                "{}: section {sec} has span = \"every\" (each span broken one at a time) and \
                 another broken span: give them in separate cases",
                self.name
            ));
        }
        let mut removed = std::collections::BTreeSet::new();
        for r in &self.removed {
            if r.section.trim().is_empty() {
                return Err(format!("{}: a removed wire without a section", self.name));
            }
            if !removed.insert(r.section.as_str()) {
                return Err(format!(
                    "{}: section {} removed twice",
                    self.name, r.section
                ));
            }
            if self.broken_wires().any(|b| b.section == r.section) {
                return Err(format!(
                    "{}: section {} is both removed and broken (a removed wire is not there to \
                     break)",
                    self.name, r.section
                ));
            }
        }
        self.on_structure
            .check()
            .map_err(|e| format!("{}: {e}", self.name))?;
        if let Some(s) = &self.structure {
            if let Some(p) = s.wind_pressure
                && !(p.is_finite() && p >= 0.0)
            {
                return Err(format!(
                    "{}: structure wind_pressure {p} Pa (0 or more)",
                    self.name
                ));
            }
            if let Some(f) = s.wind_factor
                && !(f.is_finite() && (0.0..=100.0).contains(&f))
            {
                return Err(format!(
                    "{}: structure wind_factor {f} (0 to 100)",
                    self.name
                ));
            }
        }
        Ok(())
    }
}

/// A load case file (`.ohllc`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadCases {
    /// `"ohl-load-cases"` (optional).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Name of the set (e.g. the code).
    #[serde(default)]
    pub name: String,
    /// The cases.
    #[serde(rename = "case", default)]
    pub cases: Vec<LoadCase>,
}

impl LoadCases {
    /// Parses and checks a file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            set.format.as_deref(),
            set.version,
            LOAD_CASE_FILE_FORMAT,
            LOAD_CASE_FILE_VERSION,
            "load case",
        )?;
        let mut names = std::collections::BTreeSet::new();
        for c in &set.cases {
            c.validate()?;
            if !names.insert(c.name.as_str()) {
                return Err(format!("load case \"{}\" is defined twice", c.name));
            }
        }
        Ok(set)
    }

    /// Reads a file's cases without checking them (the load case form
    /// edits a file whose cases may not check yet while it is being
    /// filled in); the TOML and its header must read. [`LoadCases::parse`]
    /// checks.
    pub fn read_unchecked(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            set.format.as_deref(),
            set.version,
            LOAD_CASE_FILE_FORMAT,
            LOAD_CASE_FILE_VERSION,
            "load case",
        )?;
        Ok(set)
    }

    /// Reads a file.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The version a file of these cases is written at: 4 when a case
    /// uses a version-4 value ([`LoadCase::uses_version_4`]), else 3 when
    /// a case has a broken wire, else 2 when a case has a
    /// `[case.structure]` table with a value, else 1 (an older program
    /// still reads it).
    pub fn written_version(&self) -> u32 {
        written_version(&self.cases)
    }
}

/// The version a load case file holding `cases` is written at
/// ([`LoadCases::written_version`]).
pub fn written_version(cases: &[LoadCase]) -> u32 {
    if cases.iter().any(LoadCase::uses_version_4) {
        4
    } else if cases.iter().any(|c| c.broken.is_some()) {
        3
    } else if cases
        .iter()
        .any(|c| c.structure.is_some_and(|s| !s.is_empty()))
    {
        2
    } else {
        1
    }
}

/// The load at one point of a structure in a load case: a point strings
/// hang on (or a wire point a wire is fixed to), or the wind on the
/// structure itself.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentLoad {
    /// Structure.
    pub structure: String,
    /// The point's label (empty: the wind on the structure itself).
    pub point: String,
    /// What it is, for reports (the point's label, or the wind band).
    pub label: String,
    /// The sets of the wires hanging there.
    pub sets: Vec<u32>,
    /// Sections hanging there (and jumpers, by their support clamps).
    pub sections: Vec<String>,
    /// Where, line coordinates.
    pub at: Point,
    /// The force on the structure, line axes (x east, y north, z up), N,
    /// unfactored.
    pub force: Point,
    /// In the structure's axes: transverse (to the right of the line),
    /// longitudinal (ahead), vertical (down), N, unfactored.
    pub local: [f64; 3],
    /// The same, factored by the load case.
    pub factored: [f64; 3],
    /// A broken-wire case ([`LoadCase::broken`]): the broken span
    /// (1 = the section's first) giving each component (transverse,
    /// longitudinal, vertical) — with `"every"`, the span whose break
    /// gives the largest; none: no broken wire.
    pub broken_span: Option<[usize; 3]>,
}

/// The loads of a load case.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseLoads {
    /// The load case.
    pub case: LoadCase,
    /// Loads at the structures' points, by structure and point.
    pub loads: Vec<AttachmentLoad>,
    /// Sections not solved, with why (their loads are missing).
    pub failed: Vec<String>,
    /// Per structure, the sections ending at or hanging on it and the
    /// jumpers held by its strings that did not solve in this case: their
    /// forces are missing from its loads, which are therefore incomplete
    /// (the loads given are those computed, nothing is made up for the
    /// missing wires).
    pub missing: Vec<MissingWire>,
    /// The wind on the structures' own bodies in this case
    /// ([`LoadCase::structure_wind`]).
    pub structure_wind: BodyWind,
    /// A broken wire in every span (`span = "every"`, more than one span):
    /// the loads of each break on its own, in the order of the spans
    /// (empty otherwise). [`CaseLoads::loads`] is then the envelope (per
    /// component the largest by size, the components possibly from
    /// different breaks): conservative for the allowable-loads check
    /// (Method 2), which takes each component on its own in both
    /// directions, but not for a check combining the components (a member
    /// force is `a·T + b·L + c·V`), so the finite-element and pole checks
    /// take every break as its own load and keep the worst (physics
    /// review 2026-10-26).
    pub broken_each: Vec<BrokenLoads>,
    /// What the case assumed, in words (version 4: the wires' wind side,
    /// the local conditions, removed and broken wires, the per-pylon
    /// sets, ice and snow on the structures; empty for a plain case).
    pub notes: Vec<String>,
}

/// The loads of one break of a broken wire in every span
/// ([`CaseLoads::broken_each`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BrokenLoads {
    /// The span broken (1 = the section's first).
    pub span: usize,
    /// The loads at the structures' points with that span broken, by
    /// structure and point (factored as the case says).
    pub loads: Vec<AttachmentLoad>,
}

impl CaseLoads {
    /// The wind on the body of structure `s` in this case: the case's
    /// ([`CaseLoads::structure_wind`]), with [`LoadCase::use_local`] the
    /// pressure times the structure's local `wind_static` — the one rule
    /// of [`LoadCase::structure_wind_at`], applied once (owner
    /// 2026-11-03: a check's "Use local conditions" switch sets
    /// `use_local` on its cases, nothing else scales the wind).
    pub fn structure_wind_at(&self, s: Option<&ohl_line::Structure>) -> BodyWind {
        let mut body = self.structure_wind;
        if self.case.use_local
            && let Some(f) = s.and_then(|s| s.local.wind_static)
        {
            body.pressure *= f;
        }
        body
    }

    /// This case as the structures' finite-element and pole checks take
    /// it: itself, or with a broken wire in every span one case per break
    /// ([`CaseLoads::broken_each`]), named `<case> (span <k> broken)`,
    /// its broken wire that one span, its loads those of that break (the
    /// structure wind's loads, at no point, kept in each).
    pub fn each_break(&self) -> Vec<CaseLoads> {
        if self.broken_each.is_empty() {
            return vec![self.clone()];
        }
        self.broken_each
            .iter()
            .map(|b| {
                let mut case = self.case.clone();
                case.name = format!("{} (span {} broken)", self.case.name, b.span);
                let wires = case
                    .broken_wires()
                    .cloned()
                    .map(|mut w| {
                        if w.span == BrokenSpan::Every {
                            w.span = BrokenSpan::One(b.span);
                        }
                        w
                    })
                    .collect();
                case.set_broken(wires);
                let mut loads = b.loads.clone();
                loads.extend(self.loads.iter().filter(|l| l.point.is_empty()).cloned());
                CaseLoads {
                    case,
                    loads,
                    broken_each: Vec::new(),
                    ..self.clone()
                }
            })
            .collect()
    }

    /// The wires missing from the loads of structure `structure`.
    pub fn missing_at<'a>(&'a self, structure: &'a str) -> impl Iterator<Item = &'a MissingWire> {
        self.missing
            .iter()
            .filter(move |m| m.structure == structure)
    }

    /// Whether the loads of structure `structure` miss a wire's forces.
    pub fn is_incomplete(&self, structure: &str) -> bool {
        self.missing_at(structure).next().is_some()
    }
}

/// What kind of wire a [`MissingWire`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireKind {
    /// A section (its spans between its structures).
    Section,
    /// A jumper held by a structure's strings.
    Jumper,
}

impl WireKind {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Section => "section",
            Self::Jumper => "jumper",
        }
    }
}

/// A section or jumper that did not solve in a load case, at one
/// structure it ends at, hangs on or is held by: the structure's loads in
/// that case lack its forces.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingWire {
    /// The structure.
    pub structure: String,
    /// Section or jumper.
    pub kind: WireKind,
    /// Its id.
    pub id: String,
    /// Why it did not solve.
    pub reason: String,
}

impl MissingWire {
    /// `section S12 not solved (reason)`.
    pub fn describe(&self) -> String {
        format!(
            "{} {} not solved ({})",
            self.kind.as_str(),
            self.id,
            self.reason
        )
    }

    /// `S12 not solved` (a jumper: `jumper J1 not solved`).
    pub fn short(&self) -> String {
        match self.kind {
            WireKind::Section => format!("{} not solved", self.id),
            WireKind::Jumper => format!("jumper {} not solved", self.id),
        }
    }
}

/// The structures a section of `line` ends at or hangs on, each once, in
/// its order.
pub fn section_structures(line: &Line, section: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(s) = line.sections.iter().find(|s| s.id == section) {
        for p in &s.points {
            if !out.contains(&p.structure) {
                out.push(p.structure.clone());
            }
        }
    }
    out
}

/// The structures whose strings hold jumper `jumper` of `line` (its
/// support clamps), each once.
pub fn jumper_structures(line: &Line, jumper: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(j) = line.jumpers.iter().find(|j| j.id == jumper) {
        for c in j.supports() {
            let s = c
                .structure
                .clone()
                .or_else(|| j.from.structure().map(str::to_owned))
                .or_else(|| j.to.structure().map(str::to_owned));
            if let Some(s) = s
                && !out.contains(&s)
            {
                out.push(s);
            }
        }
    }
    out
}

/// The missing wires of a section or jumper `id` of `line` that did not
/// solve, one per structure it ends at, hangs on or is held by.
pub fn missing_wires(line: &Line, kind: WireKind, id: &str, reason: &str) -> Vec<MissingWire> {
    let structures = match kind {
        WireKind::Section => section_structures(line, id),
        WireKind::Jumper => jumper_structures(line, id),
    };
    structures
        .into_iter()
        .map(|structure| MissingWire {
            structure,
            kind,
            id: id.to_owned(),
            reason: reason.to_owned(),
        })
        .collect()
}

/// The default exit clauses of one section solve.
pub fn default_budget() -> Budget {
    ohl_wirefe::default_fe_budget()
}

// The structure's axes: (right of the line, ahead) in line axes (the
// line's one rule).
use ohl_line::links::axes;

/// Solves every section of `line` (structure types resolved) in each load
/// case and gives the loads on the structures. A case with a broken wire
/// ([`LoadCase::broken`]) solves that section with the wire of its span
/// gone (`ohl_line::solve::solve_line_broken`), with `"every"` once per
/// span, the largest of each component kept
/// ([`AttachmentLoad::broken_span`] says which span gave it) and each
/// break's loads on their own ([`CaseLoads::broken_each`]); a section
/// or span not in the line fails the case as a whole. `weather`: the weather
/// cases the load cases name (also those of the line's creep-stretch
/// criteria). A section that does not solve is listed in `failed`; a load
/// case naming an unknown weather case fails as a whole.
///
/// Version 4 (2026-11-03): several broken wires are broken together, in
/// one solve per section (`ohl_line::solve::solve_line_broken_spans`;
/// with `"every"` in one of them, once per span of that section, the
/// others broken in each); removed conductors ([`LoadCase::removed`]) are
/// left out of the solve, with the jumpers and spacers on them; the
/// wires' wind comes from [`LoadCase::wind_from`] when given; with
/// [`LoadCase::use_local`] the wires are solved with the structures'
/// local conditions for strength (`ohl_line::local::localized`). Each
/// structure takes only the cases of its set
/// ([`ohl_line::Line::load_cases_for`]): the loads of a structure not
/// taking a case are left out of that case. A case with ice on the
/// structures and no density fails as a whole. What was assumed is in
/// [`CaseLoads::notes`].
///
/// Expected cost: one finite-element solve per section and load case
/// (~1 ms for 10 spans), one more per section with a broken wire;
/// `budget` applies to each solve.
pub fn structure_loads(
    line: &Line,
    library: &ohl_cables::Library,
    weather: &[WeatherCase],
    cases: &[LoadCase],
    budget: &Budget,
) -> Result<Vec<CaseLoads>, String> {
    let mut line = line.clone();
    line.set_weather_cases(weather);
    let has_local = ohl_line::local::has_local(&line);
    let local = (has_local && cases.iter().any(|c| c.use_local))
        .then(|| ohl_line::local::localized(&line, ohl_line::local::Purpose::Strength));
    let set_notes = load_case_set_notes(&line, cases);
    let mut out = Vec::with_capacity(cases.len());
    for case in cases {
        let w = weather
            .iter()
            .find(|w| w.name == case.weather)
            .ok_or_else(|| {
                format!(
                    "load case {}: no weather case \"{}\"",
                    case.name, case.weather
                )
            })?;
        let fail = |e: String| format!("load case {}: {e}", case.name);
        let mut notes = set_notes.clone();
        if let Some(side) = case.wind_from {
            if w.wind_pressure.si() > 0.0 {
                notes.push(format!(
                    "the wind on the wires from the {} (the load case's; weather case \"{}\": \
                     from the {})",
                    side.as_str(),
                    w.name,
                    w.wind_from.as_str()
                ));
            } else {
                notes.push(format!(
                    "wind_from = \"{}\" has no effect: weather case \"{}\" has no wind",
                    side.as_str(),
                    w.name
                ));
            }
        }
        let wires_weather = case.wire_weather(w);
        let base = match (&local, case.use_local) {
            (Some(l), true) => {
                notes.push(
                    "local conditions: the wires solved with each span's local wind and ice \
                     for strength (from its structures' wind_static and wire_ice), the \
                     structure bodies' wind times each structure's wind_static"
                        .into(),
                );
                l
            }
            (None, true) => {
                notes.push(
                    "local conditions: no structure gives any, the weather case as written".into(),
                );
                &line
            }
            _ => &line,
        };
        // Removed conductors: their sections (and the jumpers and spacers
        // on them) out of the solve.
        for r in &case.removed {
            r.check(base).map_err(fail)?;
        }
        let removed_line;
        let solving = if case.removed.is_empty() {
            base
        } else {
            removed_line = without_removed(base, &case.removed, &mut notes);
            &removed_line
        };
        // Ice on the structures: refused without a density (at compute
        // time: the weather case may give it).
        let mut local_ice: Vec<String> = Vec::new();
        for s in &solving.structures {
            if line.takes_load_case(&s.id, &case.name)
                && let Some(ice) = case.structure_ice_at(w, Some(s)).map_err(fail)?
                && case.takes_local_ice(w)
            {
                local_ice.push(format!("{} {:.0} mm", s.id, ice.thickness * 1000.0));
            }
        }
        if !local_ice.is_empty() {
            notes.push(format!(
                "local ice on the structures (the case takes the local conditions, weather case \
                 \"{}\" has ice, the case gives no ice of its own): {}",
                w.name,
                local_ice.join(", ")
            ));
        }
        // The broken wires: those of one span, all together; one with
        // "every", once per span of its section.
        let mut fixed: Vec<(String, Vec<usize>)> = Vec::new();
        let mut every: Option<(String, Vec<usize>)> = None;
        for b in case.broken_wires() {
            let spans = b.spans(solving).map_err(fail)?;
            match b.span {
                BrokenSpan::Every => every = Some((b.section.clone(), spans)),
                BrokenSpan::One(_) => match fixed.iter_mut().find(|f| f.0 == b.section) {
                    Some(f) => f.1.extend(spans),
                    None => fixed.push((b.section.clone(), spans)),
                },
            }
            notes.push(format!("broken wire: {}", b.describe()));
        }
        if case.broken_wires().count() > 1 {
            notes.push(
                "the wires listed broken together (one solve per section), the static state \
                 after the breaks"
                    .into(),
            );
        }
        // The span named for each component (the first one broken, or the
        // span of "every").
        let first = fixed.first().and_then(|f| f.1.first().copied());
        let mut each = Vec::new();
        let (mut loads, failed, mut missing) = match &every {
            None => {
                let (at, failed, missing) = case_loads(
                    solving,
                    library,
                    &wires_weather,
                    case,
                    &fixed,
                    first,
                    budget,
                );
                (at.into_values().collect::<Vec<_>>(), failed, missing)
            }
            Some((section, broken)) => {
                // A solve per span broken; the worst of each component kept
                // (the envelope), and each break on its own when there are
                // several.
                let mut worst: BTreeMap<(String, String), AttachmentLoad> = BTreeMap::new();
                let (mut failed, mut missing) = (Vec::new(), Vec::new());
                for &k in broken {
                    let mut all = fixed.clone();
                    all.push((section.clone(), vec![k]));
                    let (at, f, m) = case_loads(
                        solving,
                        library,
                        &wires_weather,
                        case,
                        &all,
                        Some(k),
                        budget,
                    );
                    if broken.len() > 1 {
                        each.push(BrokenLoads {
                            span: k + 1,
                            loads: at.values().cloned().collect(),
                        });
                    }
                    for x in f {
                        if !failed.contains(&x) {
                            failed.push(x);
                        }
                    }
                    for x in m {
                        if !missing.contains(&x) {
                            missing.push(x);
                        }
                    }
                    for (key, l) in at {
                        match worst.get_mut(&key) {
                            Some(e) => envelope(e, &l, &line),
                            None => {
                                worst.insert(key, l);
                            }
                        }
                    }
                }
                (worst.into_values().collect(), failed, missing)
            }
        };
        // Per-pylon sets: a structure not taking the case has no loads in
        // it.
        if line.has_load_case_sets() {
            let takes = |s: &str| line.takes_load_case(s, &case.name);
            let mut out_of: Vec<&str> = loads
                .iter()
                .map(|l: &AttachmentLoad| l.structure.as_str())
                .chain(missing.iter().map(|m| m.structure.as_str()))
                .filter(|s| !takes(s))
                .collect();
            out_of.sort_unstable();
            out_of.dedup();
            if !out_of.is_empty() {
                notes.push(format!(
                    "not in the load case set of {} structure(s): {}",
                    out_of.len(),
                    out_of.join(", ")
                ));
            }
            loads.retain(|l| takes(&l.structure));
            missing.retain(|m| takes(&m.structure));
            for b in &mut each {
                b.loads.retain(|l| takes(&l.structure));
            }
        }
        if case.ices_or_snows(w) {
            notes.push(
                "ice and snow on the structures: on the members of the types' models (the \
                 finite-element check), the shafts of the poles (ice), and listed per structure \
                 (Compute structure loads); not in the allowable loads at the attachments"
                    .into(),
            );
        }
        let body = case.structure_wind(w);
        if case.on_structure.direction.is_some() && body.factored() > 0.0 {
            notes.push(format!(
                "the wind on the structure bodies {} (the wires' from the {})",
                body.direction.words(),
                case.wind_from.unwrap_or(w.wind_from).as_str()
            ));
        }
        out.push(CaseLoads {
            case: case.clone(),
            loads,
            failed,
            missing,
            structure_wind: body,
            broken_each: each,
            notes,
        });
    }
    Ok(out)
}

/// `cases` as a check with **Use local conditions** ticked takes them:
/// each with [`LoadCase::use_local`] (owner 2026-11-03: the switch and a
/// case's `use_local` mean the same — this case takes the structures'
/// local conditions; [`structure_loads`] then solves the wires on the
/// localized line and every body wind takes `wind_static` once, through
/// [`LoadCase::structure_wind_at`]). Unticked: the cases as written (a
/// case's own `use_local` kept).
pub fn with_local(cases: Vec<LoadCase>, ticked: bool) -> Vec<LoadCase> {
    if !ticked {
        return cases;
    }
    cases
        .into_iter()
        .map(|c| LoadCase {
            use_local: true,
            ..c
        })
        .collect()
}

/// The unknown names of the per-pylon load case sets (a structure's own
/// or a type's default naming a case not in `cases`), in words; checked
/// when the loads are computed (the library may change after the line
/// was written).
pub fn load_case_set_notes(line: &Line, cases: &[LoadCase]) -> Vec<String> {
    let known = |n: &String| cases.iter().any(|c| &c.name == n);
    let mut out = Vec::new();
    for s in &line.structures {
        let unknown: Vec<&str> = s
            .load_cases
            .iter()
            .filter(|n| !known(n))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            out.push(format!(
                "structure {}: load case(s) not in the library: {}",
                s.id,
                unknown.join(", ")
            ));
        }
    }
    for (t, names) in &line.structure_loads.type_load_cases {
        let unknown: Vec<&str> = names
            .iter()
            .filter(|n| !known(n))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            out.push(format!(
                "structure type {t}: default load case(s) not in the library: {}",
                unknown.join(", ")
            ));
        }
    }
    out
}

/// `line` without the sections of `removed` (removed conductors), the
/// jumpers with an end on a wire point only they have or on a span point
/// of theirs (and those on such jumpers), and the spacers on their span
/// points — said in `notes`.
fn without_removed(line: &Line, removed: &[RemovedWire], notes: &mut Vec<String>) -> Line {
    let mut out = line.clone();
    let gone = |id: &str| removed.iter().any(|r| r.section == id);
    out.sections.retain(|s| !gone(&s.id));
    let kept_point = |p: &ohl_line::SectionPoint| out.sections.iter().any(|s| s.points.contains(p));
    let removed_point = |p: &ohl_line::SectionPoint| {
        line.sections
            .iter()
            .any(|s| gone(&s.id) && s.points.contains(p))
    };
    let mut jumpers_gone: Vec<String> = Vec::new();
    loop {
        let before = jumpers_gone.len();
        for j in &out.jumpers {
            if jumpers_gone.contains(&j.id) {
                continue;
            }
            let off = [&j.from, &j.to].into_iter().any(|end| match end {
                ohl_line::JumperEnd::Wire(p) => removed_point(p) && !kept_point(p),
                ohl_line::JumperEnd::Tap { tap } => match tap {
                    ohl_line::Tap::Span(r) => gone(&r.section),
                    ohl_line::Tap::Jumper { jumper, .. } => jumpers_gone.contains(jumper),
                },
            });
            if off {
                jumpers_gone.push(j.id.clone());
            }
        }
        if jumpers_gone.len() == before {
            break;
        }
    }
    out.jumpers.retain(|j| !jumpers_gone.contains(&j.id));
    let spacers = out.spacers.len();
    out.spacers
        .retain(|x| !gone(&x.a.section) && !gone(&x.b.section));
    notes.push(format!(
        "removed conductor(s): {} — not in the solve, no loads from {} on any structure",
        removed
            .iter()
            .map(RemovedWire::describe)
            .collect::<Vec<_>>()
            .join(", "),
        if removed.len() == 1 { "it" } else { "them" }
    ));
    if !jumpers_gone.is_empty() {
        notes.push(format!(
            "jumper(s) on the removed wire(s) left out too: {}",
            jumpers_gone.join(", ")
        ));
    }
    if out.spacers.len() < spacers {
        notes.push(format!(
            "{} spacer(s) on the removed wire(s) left out",
            spacers - out.spacers.len()
        ));
    }
    out
}

/// Keeps in `e` the larger of each force component of `e` and `l` (by
/// size of the factored value, the sign kept; factored and unfactored
/// together) with the span giving it, the force in line axes made again
/// of the components kept (the envelope of a broken wire in every span:
/// conservative for the allowable-loads check, which takes each component
/// on its own; the components may come from different spans, so the
/// finite-element and pole checks take each break on its own instead,
/// [`CaseLoads::broken_each`]).
fn envelope(e: &mut AttachmentLoad, l: &AttachmentLoad, line: &Line) {
    let mut kept = e.broken_span.unwrap_or_default();
    let theirs = l.broken_span.unwrap_or_default();
    for i in 0..3 {
        if l.factored[i].abs() > e.factored[i].abs() {
            e.factored[i] = l.factored[i];
            e.local[i] = l.local[i];
            kept[i] = theirs[i];
        }
    }
    e.broken_span = Some(kept);
    for s in &l.sections {
        if !e.sections.contains(s) {
            e.sections.push(s.clone());
        }
    }
    let (right, ahead) = axes(
        line.structure(&e.structure)
            .map_or(0.0, |s| s.orientation_deg),
    );
    e.force = [
        e.local[0] * right[0] + e.local[1] * ahead[0],
        e.local[0] * right[1] + e.local[1] * ahead[1],
        -e.local[2],
    ];
}

/// What [`case_loads`] gives: the loads by (structure, point), the
/// sections and jumpers not solved (in words, and per structure).
type Loads = (
    BTreeMap<(String, String), AttachmentLoad>,
    Vec<String>,
    Vec<MissingWire>,
);

/// The loads at the structures' points of load case `case` in weather
/// case `w`, the wires of the spans `broken` (per section, spans from 0)
/// gone together when given; `named` the broken span the loads name
/// ([`AttachmentLoad::broken_span`], from 0); with the sections and
/// jumpers that did not solve.
fn case_loads(
    line: &Line,
    library: &ohl_cables::Library,
    w: &WeatherCase,
    case: &LoadCase,
    broken: &[(String, Vec<usize>)],
    named: Option<usize>,
    budget: &Budget,
) -> Loads {
    let calculation = Calculation::ALL
        .into_iter()
        .find(|c| c.condition() == case.condition.condition() && c.method() == Method::Fe)
        .expect("every condition has an FE calculation");
    // (structure, point) → load.
    let mut at: BTreeMap<(String, String), AttachmentLoad> = BTreeMap::new();
    let mut failed = Vec::new();
    let mut missing = Vec::new();
    // Every section in the load case's condition, solved together where
    // they share strings.
    let mut solving = line.clone();
    for s in &mut solving.sections {
        s.calculation = calculation;
    }
    let results = if broken.is_empty() {
        ohl_line::solve::solve_line_with(&solving, library, w, budget)
    } else {
        let spans: Vec<(&str, Vec<usize>)> = broken
            .iter()
            .map(|(s, k)| (s.as_str(), k.clone()))
            .collect();
        match ohl_line::solve::solve_line_broken_spans(&solving, library, w, &spans, budget) {
            Ok(r) => r,
            // Checked by the caller (the sections are there).
            Err(e) => {
                failed.push(e.to_string());
                return (at, failed, missing);
            }
        }
    };
    let span_of = if broken.is_empty() {
        None
    } else {
        named.map(|k| [k + 1; 3])
    };

    let mut solved = Vec::with_capacity(results.len());
    for (section, result) in line.sections.iter().zip(results) {
        let shape = match result {
            Ok(shape) => shape,
            Err(e) => {
                failed.push(format!("{}: {e}", section.id));
                missing.extend(missing_wires(
                    line,
                    WireKind::Section,
                    &section.id,
                    &e.to_string(),
                ));
                continue;
            }
        };
        for (point, supports) in section.points.iter().zip(&shape.attachment_loads) {
            let structure = line.structure(&point.structure);
            // The sets hanging on the same strings.
            let sets: Vec<u32> = structure
                .and_then(|s| {
                    let w = s.wire_at_set(point.set)?;
                    Some(
                        s.component(&w.label)
                            .wires
                            .iter()
                            .filter_map(|&i| s.wires[i].set)
                            .collect(),
                    )
                })
                .unwrap_or_else(|| vec![point.set]);
            for support in supports {
                let place = Place {
                    structure: &point.structure,
                    label: &support.label,
                    at: support.at,
                };
                add(
                    &mut at,
                    line,
                    case,
                    place,
                    support.force,
                    &sets,
                    &section.id,
                );
            }
        }
        solved.push(shape);
    }
    // Jumpers' support clamps: their strings pull on their tops (the
    // string's force towards the clamp, half its weight and wind).
    for j in line.jumpers.iter().filter(|j| !j.supports().is_empty()) {
        let shape = match ohl_line::jumper::solve_one_jumper(line, library, w, &solved, j) {
            Ok(shape) => shape,
            Err(e) => {
                failed.push(format!("jumper {}: {e}", j.id));
                missing.extend(missing_wires(line, WireKind::Jumper, &j.id, &e.to_string()));
                continue;
            }
        };
        // The local wind at its structures (roadmap O15).
        let local = line.jumper_wind_factor(j);
        for support in &shape.supports {
            let Some(structure) = line.structure(&support.structure) else {
                continue;
            };
            for x in &support.strings {
                let Some(string) = structure.string(&x.label) else {
                    continue;
                };
                let towards = [0, 1, 2].map(|k| support.clamp[k] - x.top[k]);
                let d = towards.iter().map(|v| v * v).sum::<f64>().sqrt();
                let mut f = towards.map(|v| if d > 0.0 { v / d * x.force } else { 0.0 });
                f[2] -= string.weight / 2.0;
                // Half the wind on the string, across the line.
                if let Some(a) = line.direction_at([x.top[0], x.top[1]]) {
                    let push = 0.5
                        * w.wind_pressure.si()
                        * local
                        * string.wind_area
                        * w.wind_from.to_left();
                    f[0] -= a[1] * push;
                    f[1] += a[0] * push;
                }
                let place = Place {
                    structure: &support.structure,
                    label: &string.top,
                    at: x.top,
                };
                add(&mut at, line, case, place, f, &[], &j.id);
            }
        }
    }
    // Risers (jumpers given by their tension, 2026-10-25): their pull on
    // the wire points they are dead-ended at where no section ends (a
    // section there carries it as a jumper's load). Through the strings to
    // their tops, shared equally, each with its whole weight; on a wire
    // point without strings, there. Simplification: the riser's ends are
    // its wire points as given (its strings are not swung by its pull).
    if line.jumpers.iter().any(|j| j.tension.is_some()) {
        let shapes = ohl_line::jumper::solve_jumpers(line, library, w, &solved);
        for (j, shape) in line.jumpers.iter().zip(shapes) {
            if j.tension.is_none() {
                continue;
            }
            let shape = match shape {
                Ok(shape) => shape,
                Err(e) => {
                    failed.push(format!("jumper {}: {e}", j.id));
                    missing.extend(missing_wires(line, WireKind::Jumper, &j.id, &e.to_string()));
                    continue;
                }
            };
            for (end, force) in [&j.from, &j.to].into_iter().zip(shape.end_forces) {
                let Some(point) = end.point() else {
                    continue;
                };
                if line.sections.iter().any(|s| s.points.contains(point)) {
                    continue;
                }
                let Some(structure) = line.structure(&point.structure) else {
                    continue;
                };
                let Some(wire) = structure.wire_at_set(point.set) else {
                    continue;
                };
                let strings: Vec<_> = structure.strings_to(&wire.label).collect();
                if strings.is_empty() {
                    let place = Place {
                        structure: &point.structure,
                        label: &wire.label,
                        at: wire.position,
                    };
                    add(&mut at, line, case, place, force, &[point.set], &j.id);
                    continue;
                }
                let share = 1.0 / strings.len() as f64;
                for string in strings {
                    let Some(top) = structure.node(&string.top) else {
                        continue;
                    };
                    let mut f = force.map(|x| x * share);
                    f[2] -= string.weight;
                    let place = Place {
                        structure: &point.structure,
                        label: &string.top,
                        at: top.position(),
                    };
                    add(&mut at, line, case, place, f, &[point.set], &j.id);
                }
            }
        }
    }
    if span_of.is_some() {
        for l in at.values_mut() {
            l.broken_span = span_of;
        }
    }
    (at, failed, missing)
}

/// A point of a structure a load acts at.
struct Place<'a> {
    structure: &'a str,
    label: &'a str,
    at: Point,
}

/// Adds force `f` (line axes, N, unfactored) at a point of a structure,
/// from `source` (a section or a jumper) and the wires of `sets`.
fn add(
    loads: &mut BTreeMap<(String, String), AttachmentLoad>,
    line: &Line,
    case: &LoadCase,
    place: Place,
    f: Point,
    sets: &[u32],
    source: &str,
) {
    let (right, ahead) = axes(
        line.structure(place.structure)
            .map_or(0.0, |s| s.orientation_deg),
    );
    let e = loads
        .entry((place.structure.to_owned(), place.label.to_owned()))
        .or_insert_with(|| AttachmentLoad {
            structure: place.structure.to_owned(),
            point: place.label.to_owned(),
            label: place.label.to_owned(),
            sets: Vec::new(),
            sections: Vec::new(),
            at: place.at,
            force: [0.0; 3],
            local: [0.0; 3],
            factored: [0.0; 3],
            broken_span: None,
        });
    if !e.sections.iter().any(|s| s == source) {
        e.sections.push(source.to_owned());
    }
    for set in sets {
        if !e.sets.contains(set) {
            e.sets.push(*set);
        }
    }
    for (sum, x) in e.force.iter_mut().zip(f) {
        *sum += x;
    }
    let local = [
        f[0] * right[0] + f[1] * right[1],
        f[0] * ahead[0] + f[1] * ahead[1],
        -f[2],
    ];
    let factors = [case.transverse, case.longitudinal, case.vertical];
    for i in 0..3 {
        e.local[i] += local[i];
        e.factored[i] += local[i] * factors[i];
    }
}

/// Adds the wind on the structures themselves to `results`: for every
/// structure whose type gives wind bands (`[[wind.band]]`), one load per
/// band — the structures' wind pressure of the load case × drag × area,
/// across the line (the wind side of the weather case), at the band's
/// mid-height on the structure's centre line — without a point, labelled
/// "wind on the structure". Factored by the structures' wind factor. Both
/// are [`LoadCase::structure_wind`]: the case's `[case.structure]` values
/// where given, else the weather case's pressure and the transverse
/// factor (as before).
///
/// A structure standing on a height extension stands its `dz` higher:
/// the bands (heights above the ground of the structure as written) rise
/// by `dz` with it — the rule of the structure application's results.
/// The force does not change (the pressure is the weather case's, the
/// same at every height); the extension's own piece below the feet has
/// no band (its members take the member wind of the `fe` check).
///
/// Version 4: the wind blows from the case's direction
/// ([`BodyWind::direction`]): along the line, or at an angle to it, the
/// band's force has a longitudinal part. A band holds one area (the area
/// facing a wind across the line); a wind from another direction takes
/// that same area, all along the direction it blows (|F| = q·c·A) —
/// an approximation, not on the safe side where the structure shows
/// more area along the line (crossarms seen in full; theory: Structure
/// wind). With [`LoadCase::use_local`] the pressure is times the
/// structure's local `wind_static`. A structure not taking the case
/// ([`ohl_line::Line::load_cases_for`]) gets none.
pub fn add_structure_wind(
    results: &mut [CaseLoads],
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    weather: &[WeatherCase],
) {
    for r in results {
        let Some(w) = weather.iter().find(|w| w.name == r.case.weather) else {
            continue;
        };
        if r.case.structure_wind(w).pressure <= 0.0 {
            continue;
        }
        for s in &line.structures {
            if !line.takes_load_case(&s.id, &r.case.name) {
                continue;
            }
            let body = r.case.structure_wind_at(w, Some(s));
            let q = body.pressure;
            let [tx, ty] = body.towards_xy();
            let Some(t) = s.structure_type.as_deref().and_then(|t| types.get(t)) else {
                continue;
            };
            let Some(bands) = t.wind.as_ref() else {
                continue;
            };
            let dz = s
                .extension
                .as_deref()
                .and_then(|e| t.extension(e))
                .map_or(0.0, |e| e.dz.si());
            let (right, ahead) = axes(s.orientation_deg);
            for b in &bands.bands {
                let (from, to) = (b.from.si() + dz, b.to.si() + dz);
                // Along where it blows (wind from the right: towards −x).
                let f = q * b.drag * b.area.si();
                let local = [f * tx, f * ty, 0.0];
                r.loads.push(AttachmentLoad {
                    structure: s.id.clone(),
                    point: String::new(),
                    // With a part along the line: the band's one area (the
                    // one across) — said not on the safe side (2026-11-08).
                    label: if body.has_longitudinal() {
                        format!(
                            "wind on the structure ({from}–{to} m; NOT ON THE SAFE SIDE: the \
                             band's area across the line taken for a wind with a part along it)"
                        )
                    } else {
                        format!("wind on the structure ({from}–{to} m)")
                    },
                    sets: Vec::new(),
                    sections: Vec::new(),
                    at: [
                        s.position[0],
                        s.position[1],
                        s.position[2] + 0.5 * (from + to),
                    ],
                    force: [
                        local[0] * right[0] + local[1] * ahead[0],
                        local[0] * right[1] + local[1] * ahead[1],
                        0.0,
                    ],
                    local,
                    factored: [local[0] * body.factor, local[1] * body.factor, 0.0],

                    broken_span: None,
                });
            }
        }
    }
}

/// The loads as CSV (one row per load case and point; forces in N):
/// `case,structure,point,label,sets,x,y,z,transverse,longitudinal,
/// vertical,factored_transverse,factored_longitudinal,factored_vertical,
/// broken_span` (`sets` separated by spaces; `broken_span`: in a
/// broken-wire case the broken span giving each component, transverse
/// longitudinal vertical, separated by spaces — the same three for one
/// span; empty otherwise).
pub fn to_csv(results: &[CaseLoads]) -> String {
    let mut out = String::from(
        "case,structure,point,label,sets,x,y,z,transverse,longitudinal,vertical,\
         factored_transverse,factored_longitudinal,factored_vertical,broken_span\n",
    );
    let quote = ohl_core::text::csv_field;
    for r in results {
        for l in &r.loads {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{}",
                quote(&r.case.name),
                quote(&l.structure),
                quote(&l.point),
                quote(&l.label),
                l.sets
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                l.at[0],
                l.at[1],
                l.at[2],
                l.local[0],
                l.local[1],
                l.local[2],
                l.factored[0],
                l.factored[1],
                l.factored[2],
                l.broken_span
                    .map_or(String::new(), |k| format!("{} {} {}", k[0], k[1], k[2])),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod broken_tests;

#[cfg(test)]
mod v4_tests;

#[cfg(test)]
mod local_tests;
