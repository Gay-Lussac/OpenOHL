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

mod analysis;
pub mod capacity;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ohl_core::budget::Budget;
use ohl_line::condition::{Calculation, Condition, Method};
use ohl_line::{Line, Point};
use ohl_weather::WeatherCase;
use serde::Deserialize;

/// `format` of a load case file.
pub const LOAD_CASE_FILE_FORMAT: &str = "ohl-load-cases";
/// Newest format version read. Version 3 adds a case's broken wire
/// (`[case.broken]`, [`BrokenWire`]); version 2 a case's
/// `[case.structure]` table ([`StructureWind`]); each written only when a
/// case has one ([`LoadCases::written_version`]); older files read as
/// before.
pub const LOAD_CASE_FILE_VERSION: u32 = 3;
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
#[serde(deny_unknown_fields)]
pub struct LoadCase {
    /// Unique name.
    pub name: String,
    /// What it stands for (e.g. a code clause).
    #[serde(default)]
    pub description: String,
    /// The weather case, by name.
    pub weather: String,
    /// The wires' condition.
    #[serde(default)]
    pub condition: WireCondition,
    /// Factor on the vertical loads.
    #[serde(default = "one")]
    pub vertical: f64,
    /// Factor on the transverse loads.
    #[serde(default = "one")]
    pub transverse: f64,
    /// Factor on the longitudinal loads.
    #[serde(default = "one")]
    pub longitudinal: f64,
    /// Strength (resistance) factor of the capacity checks.
    #[serde(default = "one")]
    pub strength: f64,
    /// Tags (code, limit state…).
    #[serde(default)]
    pub tags: Vec<String>,
    /// The wind on the structures' own bodies when it differs from the
    /// wires' (version 2; none: the weather case's pressure and the
    /// transverse factor).
    #[serde(default)]
    pub structure: Option<StructureWind>,
    /// A broken wire (version 3, roadmap O9): the section solved with the
    /// wire of one of its spans gone, or of every span one at a time
    /// ([`BrokenWire`]); none: every wire whole.
    #[serde(default)]
    pub broken: Option<BrokenWire>,
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
    /// left (−x, the wind from the right), +1 to the right.
    pub towards: f64,
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
        }
    }

    /// The wind on the structures' own bodies in this case with weather
    /// case `weather` (the one it names): the pressure of its
    /// `[case.structure]` table, else the weather case's; the factor
    /// there, else the transverse factor; the side from the weather case.
    /// One rule for the line's structure checks and the structure
    /// application.
    pub fn structure_wind(&self, weather: &WeatherCase) -> BodyWind {
        let own = self.structure.unwrap_or_default();
        BodyWind {
            pressure: own
                .wind_pressure
                .unwrap_or_else(|| weather.wind_pressure.si()),
            factor: own.wind_factor.unwrap_or(self.transverse),
            towards: -weather.wind_from.to_left(),
            own_pressure: own.wind_pressure.is_some(),
            own_factor: own.wind_factor.is_some(),
        }
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
        if let Some(b) = &self.broken
            && b.section.trim().is_empty()
        {
            return Err(format!("{}: a broken wire without a section", self.name));
        }
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

    /// Reads a file.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The version a file of these cases is written at: 3 when a case has
    /// a broken wire, else 2 when a case has a `[case.structure]` table
    /// with a value, else 1 (an older program still reads it).
    pub fn written_version(&self) -> u32 {
        written_version(&self.cases)
    }
}

/// The version a load case file holding `cases` is written at
/// ([`LoadCases::written_version`]).
pub fn written_version(cases: &[LoadCase]) -> u32 {
    if cases.iter().any(|c| c.broken.is_some()) {
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
                if let Some(w) = case.broken.as_mut() {
                    w.span = BrokenSpan::One(b.span);
                }
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
/// Expected cost: one finite-element solve per section and load case
/// (~1 ms for 10 spans); `budget` applies to each solve.
pub fn structure_loads(
    line: &Line,
    library: &ohl_cables::Library,
    weather: &[WeatherCase],
    cases: &[LoadCase],
    budget: &Budget,
) -> Result<Vec<CaseLoads>, String> {
    let mut line = line.clone();
    line.set_weather_cases(weather);
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
        let broken = match &case.broken {
            Some(b) => b
                .spans(&line)
                .map_err(|e| format!("load case {}: {e}", case.name))?,
            None => Vec::new(),
        };
        let mut each = Vec::new();
        let (loads, failed, missing) = if broken.is_empty() {
            let (at, failed, missing) = case_loads(&line, library, w, case, None, budget);
            (at.into_values().collect(), failed, missing)
        } else {
            let section = case.broken.as_ref().map_or("", |b| b.section.as_str());
            // A solve per span broken; the worst of each component kept
            // (the envelope), and each break on its own when there are
            // several.
            let mut worst: BTreeMap<(String, String), AttachmentLoad> = BTreeMap::new();
            let (mut failed, mut missing) = (Vec::new(), Vec::new());
            for &k in &broken {
                let (at, f, m) = case_loads(&line, library, w, case, Some((section, k)), budget);
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
        };
        out.push(CaseLoads {
            case: case.clone(),
            loads,
            failed,
            missing,
            structure_wind: case.structure_wind(w),
            broken_each: each,
        });
    }
    Ok(out)
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
/// case `w`, the wire of span `broken` (section, span from 0) gone when
/// given; with the sections and jumpers that did not solve.
fn case_loads(
    line: &Line,
    library: &ohl_cables::Library,
    w: &WeatherCase,
    case: &LoadCase,
    broken: Option<(&str, usize)>,
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
    let results = match broken {
        None => ohl_line::solve::solve_line_with(&solving, library, w, budget),
        Some((section, span)) => {
            match ohl_line::solve::solve_line_broken(&solving, library, w, section, span, budget) {
                Ok(r) => r,
                // Checked by the caller (the section is there).
                Err(e) => {
                    failed.push(e.to_string());
                    return (at, failed, missing);
                }
            }
        }
    };
    let span_of = broken.map(|(_, k)| [k + 1; 3]);
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
        let body = r.case.structure_wind(w);
        let q = body.pressure;
        if q <= 0.0 {
            continue;
        }
        for s in &line.structures {
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
            let (right, _) = axes(s.orientation_deg);
            for b in &bands.bands {
                let (from, to) = (b.from.si() + dz, b.to.si() + dz);
                // Wind from the right blows towards the left (−x).
                let f = -q * b.drag * b.area.si() * body.towards;
                let local = [-f, 0.0, 0.0];
                r.loads.push(AttachmentLoad {
                    structure: s.id.clone(),
                    point: String::new(),
                    label: format!("wind on the structure ({from}–{to} m)"),
                    sets: Vec::new(),
                    sections: Vec::new(),
                    at: [
                        s.position[0],
                        s.position[1],
                        s.position[2] + 0.5 * (from + to),
                    ],
                    force: [-f * right[0], -f * right[1], 0.0],
                    local,
                    factored: [local[0] * body.factor, 0.0, 0.0],
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
