// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The recipes: `[[load_case]]` and `[[load_case.load]]`, in a structure
//! file or a structure load-case file.

use std::collections::BTreeSet;
use std::path::Path;

use ohl_core::units::{Force, Stress};
use ohl_structures::StructureType;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `format` of a structure load-case file.
pub const STRUCTURE_LOAD_CASE_FILE_FORMAT: &str = "ohl-structure-load-cases";
/// Newest format version read. Version 2 adds the line quantities
/// `structure_wind` and `structure_wind_factored` (a surface load taking
/// the line case's wind on the structure body); a file is written at 2
/// only when a load uses one ([`StructureLoadCases::new`]).
pub const STRUCTURE_LOAD_CASE_FILE_VERSION: u32 = 2;
/// File extension of structure load-case files.
pub const STRUCTURE_LOAD_CASE_FILE_EXTENSION: &str = "ohlslc";
/// The case today's `[[load]]` (nodal forces) is read as.
pub const MODEL_CASE: &str = "model";

fn one() -> f64 {
    1.0
}

fn is_one(x: &f64) -> bool {
    *x == 1.0
}

/// A value the line gives at a point of the placed structure, in one of
/// the line's load cases. Forces in the structure's axes (transverse to
/// the right, longitudinal ahead, vertical down), N — except the two
/// structure wind pressures (Pa), which act on members as a surface load
/// ([`LineQuantity::is_pressure`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineQuantity {
    /// The wires' and strings' force across the line, unfactored.
    Transverse,
    /// Along the line, unfactored.
    Longitudinal,
    /// Down, unfactored.
    Vertical,
    /// Across the line, factored by the line's load case.
    TransverseFactored,
    /// Along the line, factored.
    LongitudinalFactored,
    /// Down, factored.
    VerticalFactored,
    /// The wire tension in the back span.
    TensionBack,
    /// The wire tension in the ahead span.
    TensionAhead,
    /// The larger of the two.
    TensionMax,
    /// The wind on the wires, across the line.
    WireWind,
    /// The ice on the wires, down.
    Ice,
    /// The wind pressure on the structure's own body in the line's case,
    /// Pa, unfactored (the case's `[case.structure]` pressure, else its
    /// weather case's): a surface load on members, blowing as the line
    /// case's wind unless a direction is given.
    StructureWind,
    /// The same times the case's structure wind factor (its
    /// `[case.structure]` factor, else its transverse factor): the
    /// pressure the line's own structure checks put on the body.
    StructureWindFactored,
}

impl LineQuantity {
    /// Every quantity.
    pub const ALL: [Self; 13] = [
        Self::Transverse,
        Self::Longitudinal,
        Self::Vertical,
        Self::TransverseFactored,
        Self::LongitudinalFactored,
        Self::VerticalFactored,
        Self::TensionBack,
        Self::TensionAhead,
        Self::TensionMax,
        Self::WireWind,
        Self::Ice,
        Self::StructureWind,
        Self::StructureWindFactored,
    ];

    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transverse => "transverse",
            Self::Longitudinal => "longitudinal",
            Self::Vertical => "vertical",
            Self::TransverseFactored => "transverse_factored",
            Self::LongitudinalFactored => "longitudinal_factored",
            Self::VerticalFactored => "vertical_factored",
            Self::TensionBack => "tension_back",
            Self::TensionAhead => "tension_ahead",
            Self::TensionMax => "tension_max",
            Self::WireWind => "wire_wind",
            Self::Ice => "ice",
            Self::StructureWind => "structure_wind",
            Self::StructureWindFactored => "structure_wind_factored",
        }
    }

    /// From its name in files.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.as_str() == name)
    }

    /// A pressure on the structure body (Pa, a surface load on members),
    /// not a force at a point.
    pub fn is_pressure(self) -> bool {
        matches!(self, Self::StructureWind | Self::StructureWindFactored)
    }

    /// A wire tension (acts along the line, towards its span).
    pub fn is_tension(self) -> bool {
        matches!(
            self,
            Self::TensionBack | Self::TensionAhead | Self::TensionMax
        )
    }

    /// Where it acts without a direction: its own axis of the table
    /// (0 transverse, 1 longitudinal, 2 vertical down), signed as given.
    pub(crate) fn axis(self) -> usize {
        match self {
            Self::Transverse
            | Self::TransverseFactored
            | Self::WireWind
            | Self::StructureWind
            | Self::StructureWindFactored => 0,
            Self::Longitudinal
            | Self::LongitudinalFactored
            | Self::TensionBack
            | Self::TensionAhead
            | Self::TensionMax => 1,
            Self::Vertical | Self::VerticalFactored | Self::Ice => 2,
        }
    }
}

/// A direction in the structure's axes (x right of the line, y ahead,
/// z up). Written `x`, `-x`, `y`, `-y`, `z`, `-z`; read also as `right`,
/// `left`, `ahead`, `back`, `up`, `down` (and `+x`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum Direction {
    /// +x, to the right of the line.
    Right,
    /// −x.
    Left,
    /// +y, ahead.
    Ahead,
    /// −y.
    Back,
    /// +z.
    Up,
    /// −z.
    Down,
}

impl Direction {
    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "x",
            Self::Left => "-x",
            Self::Ahead => "y",
            Self::Back => "-y",
            Self::Up => "z",
            Self::Down => "-z",
        }
    }

    /// The unit vector in the table's axes (transverse, longitudinal,
    /// vertical down).
    pub fn table_vector(self) -> [f64; 3] {
        match self {
            Self::Right => [1.0, 0.0, 0.0],
            Self::Left => [-1.0, 0.0, 0.0],
            Self::Ahead => [0.0, 1.0, 0.0],
            Self::Back => [0.0, -1.0, 0.0],
            Self::Up => [0.0, 0.0, -1.0],
            Self::Down => [0.0, 0.0, 1.0],
        }
    }
}

impl TryFrom<String> for Direction {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        Ok(match s.trim() {
            "x" | "+x" | "right" => Self::Right,
            "-x" | "left" => Self::Left,
            "y" | "+y" | "ahead" => Self::Ahead,
            "-y" | "back" => Self::Back,
            "z" | "+z" | "up" => Self::Up,
            "-z" | "down" => Self::Down,
            other => {
                return Err(format!(
                    "direction \"{other}\": x, -x, y, -y, z or -z (structure axes: x right, y ahead, z up)"
                ));
            }
        })
    }
}

impl From<Direction> for String {
    fn from(d: Direction) -> Self {
        d.as_str().to_owned()
    }
}

/// A side of the structure along the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Towards the ahead span (+y).
    Ahead,
    /// Towards the back span (−y).
    Back,
}

impl Side {
    /// The name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ahead => "ahead",
            Self::Back => "back",
        }
    }

    /// The direction it points to.
    pub fn direction(self) -> Direction {
        match self {
            Self::Ahead => Direction::Ahead,
            Self::Back => Direction::Back,
        }
    }
}

/// Which points or members: `"all"`, or a list of labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// Every one.
    All,
    /// These, by label (points) or id (members).
    These(Vec<String>),
}

impl<'de> Deserialize<'de> for Pick {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Given {
            Text(String),
            List(Vec<String>),
        }
        match Given::deserialize(d)? {
            Given::Text(t) if t == "all" => Ok(Self::All),
            Given::Text(t) => Err(serde::de::Error::custom(format!(
                "\"{t}\": give \"all\" or a list of labels"
            ))),
            Given::List(v) => Ok(Self::These(v)),
        }
    }
}

impl Serialize for Pick {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::All => s.serialize_str("all"),
            Self::These(v) => v.serialize(s),
        }
    }
}

/// A share: a number (0.4) or a percentage (`"40 %"`).
fn de_share<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Given {
        Number(f64),
        Text(String),
    }
    Ok(match Option::<Given>::deserialize(d)? {
        None => None,
        Some(Given::Number(x)) => Some(x),
        Some(Given::Text(t)) => Some(parse_share(&t).map_err(serde::de::Error::custom)?),
    })
}

/// `"40 %"` or `"0.4"` as 0.4.
pub(crate) fn parse_share(t: &str) -> Result<f64, String> {
    let t = t.trim();
    let bad = || format!("share \"{t}\": a number (0.4) or a percentage (\"40 %\")");
    match t.strip_suffix('%') {
        Some(p) => p
            .trim()
            .parse::<f64>()
            .map(|x| x / 100.0)
            .map_err(|_| bad()),
        None => t.parse().map_err(|_| bad()),
    }
}

fn ser_force<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => s.serialize_str(&format!("{x} N")),
        None => s.serialize_none(),
    }
}

fn ser_pressure<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => s.serialize_str(&format!("{x} Pa")),
        None => s.serialize_none(),
    }
}

/// One load of a load case: what, how, where.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoadRecipe {
    /// What: a value the line gives…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<LineQuantity>,
    /// …or a force of the case itself, N (needs a direction)…
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, Force>",
        serialize_with = "ser_force",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<f64>,
    /// …or a pressure on the members' area, Pa (a surface load; needs
    /// members and a direction). (A pressure the line gives is a `what`:
    /// `structure_wind`, `structure_wind_factored`.)
    #[serde(
        default,
        deserialize_with = "ohl_core::units::si::de_opt::<_, Stress>",
        serialize_with = "ser_pressure",
        skip_serializing_if = "Option::is_none"
    )]
    pub pressure: Option<f64>,
    /// The line's load case giving the value (default: the case's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_case: Option<String>,
    /// Factor.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub factor: f64,
    /// One-sided share (0.4 or `"40 %"`), acting on `side` only.
    #[serde(
        default,
        deserialize_with = "de_share",
        skip_serializing_if = "Option::is_none"
    )]
    pub one_sided: Option<f64>,
    /// The side it acts on: points whose strain strings point to the other
    /// side are left out; a tension acts towards it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Direction (default: the value's own axis; a tension towards its
    /// span or `side`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Points or wire points, by label, or `"all"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Pick>,
    /// Or the points carrying these sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<u32>,
    /// Or the points carrying these phases (phase labels of the line).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,
    /// Members, by id, or `"all"` (surface loads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Pick>,
    /// A note (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Default for LoadRecipe {
    fn default() -> Self {
        Self {
            what: None,
            value: None,
            pressure: None,
            line_case: None,
            factor: 1.0,
            one_sided: None,
            side: None,
            direction: None,
            points: None,
            sets: Vec::new(),
            phases: Vec::new(),
            members: None,
            note: String::new(),
        }
    }
}

impl LoadRecipe {
    /// Checks that it says one what, a where, and a direction where one
    /// is needed.
    pub fn check(&self) -> Result<(), String> {
        let whats = [
            self.what.is_some(),
            self.value.is_some(),
            self.pressure.is_some(),
        ];
        if whats.iter().filter(|x| **x).count() != 1 {
            return Err("give one of what, value or pressure".into());
        }
        let on_points = self.points.is_some() || !self.sets.is_empty() || !self.phases.is_empty();
        let line_pressure = self.what.is_some_and(LineQuantity::is_pressure);
        match (
            self.pressure.is_some() || line_pressure,
            self.members.is_some(),
        ) {
            (true, false) if line_pressure => {
                return Err(format!(
                    "{} is a pressure: it needs members (a surface load)",
                    self.what.map_or("", LineQuantity::as_str)
                ));
            }
            (true, false) => return Err("a pressure needs members".into()),
            (false, true) => {
                return Err("members take a pressure or structure_wind (a surface load)".into());
            }
            (true, true) if on_points || self.side.is_some() => {
                return Err(
                    "a surface load is on members, not points, sets, phases or a side".into(),
                );
            }
            (false, false) if !on_points => {
                return Err("say where: points (\"all\" or labels), sets or phases".into());
            }
            _ => {}
        }
        if (self.value.is_some() || self.pressure.is_some()) && self.direction.is_none() {
            return Err("a value or pressure needs a direction".into());
        }
        if self.what == Some(LineQuantity::TensionMax)
            && self.direction.is_none()
            && self.side.is_none()
        {
            return Err("tension_max needs a direction or a side".into());
        }
        if !self.factor.is_finite() {
            return Err(format!("factor {}", self.factor));
        }
        if let Some(s) = self.one_sided {
            if !(s > 0.0 && s <= 1.0) {
                return Err(format!("one_sided {s}: above 0, at most 1 (100 %)"));
            }
            if self.side.is_none() {
                return Err("one_sided needs a side".into());
            }
        }
        for (what, v) in [("value", self.value), ("pressure", self.pressure)] {
            if v.is_some_and(|v| !v.is_finite()) {
                return Err(format!("{what} not a number"));
            }
        }
        Ok(())
    }

    /// Factor × one-sided share.
    pub fn scale(&self) -> f64 {
        self.factor * self.one_sided.unwrap_or(1.0)
    }
}

/// A load case of a structure: named loads, its strength factor and the
/// factor on the structure's self-weight.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoadCaseRecipe {
    /// Unique name.
    pub name: String,
    /// What it stands for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The line's load case giving the line values (a load may name
    /// another).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_case: Option<String>,
    /// Strength (resistance) factor of the checks.
    #[serde(default = "one")]
    pub strength: f64,
    /// Factor on the structure's self-weight (0: none).
    #[serde(default = "one")]
    pub self_weight: f64,
    /// The loads.
    #[serde(rename = "load", default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<LoadRecipe>,
}

impl LoadCaseRecipe {
    /// A case without loads.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            description: String::new(),
            line_case: None,
            strength: 1.0,
            self_weight: 1.0,
            loads: Vec::new(),
        }
    }

    /// Checks the factors and every load.
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a load case without a name".into());
        }
        if !(self.strength.is_finite() && self.strength > 0.0) {
            return Err(format!(
                "load case {}: strength {} (above 0)",
                self.name, self.strength
            ));
        }
        if !(self.self_weight.is_finite() && self.self_weight >= 0.0) {
            return Err(format!(
                "load case {}: self_weight {} (0 or more)",
                self.name, self.self_weight
            ));
        }
        for (k, l) in self.loads.iter().enumerate() {
            l.check()
                .map_err(|e| format!("load case {}, load {}: {e}", self.name, k + 1))?;
        }
        Ok(())
    }
}

/// The version a structure load-case file holding `cases` is written at:
/// 2 when a load takes a structure wind of the line, else 1.
pub fn written_version(cases: &[LoadCaseRecipe]) -> u32 {
    if cases
        .iter()
        .flat_map(|c| &c.loads)
        .any(|l| l.what.is_some_and(LineQuantity::is_pressure))
    {
        2
    } else {
        1
    }
}

/// Checks a list of cases: each, and their names unique.
pub(crate) fn check_cases(cases: &[LoadCaseRecipe]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for c in cases {
        c.check()?;
        if !names.insert(c.name.as_str()) {
            return Err(format!("load case \"{}\" is defined twice", c.name));
        }
    }
    Ok(())
}

/// A structure load-case file (`.ohlslc`): the recipes on their own, for
/// export and import.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StructureLoadCases {
    /// `"ohl-structure-load-cases"` (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Name of the set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The cases.
    #[serde(rename = "load_case", default)]
    pub cases: Vec<LoadCaseRecipe>,
}

impl StructureLoadCases {
    /// The cases, with this format's header: version 2 when a load takes
    /// a structure wind of the line ([`LineQuantity::is_pressure`]), else
    /// 1 (an older program still reads it).
    pub fn new(name: &str, cases: Vec<LoadCaseRecipe>) -> Self {
        Self {
            format: Some(STRUCTURE_LOAD_CASE_FILE_FORMAT.into()),
            version: Some(written_version(&cases)),
            name: name.to_owned(),
            cases,
        }
    }

    /// Parses and checks a file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            set.format.as_deref(),
            set.version,
            STRUCTURE_LOAD_CASE_FILE_FORMAT,
            STRUCTURE_LOAD_CASE_FILE_VERSION,
            "structure load case",
        )?;
        check_cases(&set.cases)?;
        Ok(set)
    }

    /// Reads a file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The file's text (values with their units).
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("recipes are plain tables")
    }

    /// The `[[load_case]]` tables of a structure file, read and checked
    /// (a case named `model` is refused when the file also has `[[load]]`,
    /// which is read as that case).
    pub fn from_structure(structure: &StructureType) -> Result<Vec<LoadCaseRecipe>, String> {
        let cases = structure
            .load_cases
            .iter()
            .enumerate()
            .map(|(k, t)| {
                toml::Value::Table(t.clone())
                    .try_into::<LoadCaseRecipe>()
                    .map_err(|e| format!("load_case {}: {}", k + 1, e.to_string().trim()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        check_cases(&cases)?;
        if !structure.loads.is_empty() && cases.iter().any(|c| c.name == MODEL_CASE) {
            return Err(format!(
                "load case \"{MODEL_CASE}\": the name of the [[load]] tables' case"
            ));
        }
        Ok(cases)
    }

    /// The cases as `[[load_case]]` tables, to keep in a structure file.
    pub fn to_tables(cases: &[LoadCaseRecipe]) -> Vec<toml::Table> {
        cases
            .iter()
            .map(|c| toml::Table::try_from(c).expect("recipes are plain tables"))
            .collect()
    }
}
