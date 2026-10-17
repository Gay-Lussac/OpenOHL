// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Load cases and the loads on the structures (Phase 4).
//!
//! A **load case** ([`LoadCase`]) is a weather case, a wire condition
//! (initial, after creep, after load, max sag) and load factors on the
//! vertical, transverse and longitudinal loads (and a strength factor for
//! the capacity checks). Load cases are data (`.olslc` files): a design
//! code's combinations and partial factors are written there — by a code
//! plugin or by hand from the standard (OLS CAD ships no code tables).
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
//! format = "ols-load-cases"
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

mod analysis;
pub mod capacity;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ols_core::budget::Budget;
use ols_line::condition::{Calculation, Condition, Method};
use ols_line::{Line, Point};
use ols_weather::WeatherCase;
use serde::Deserialize;

/// `format` of a load case file.
pub const LOAD_CASE_FILE_FORMAT: &str = "ols-load-cases";
/// Newest format version read.
pub const LOAD_CASE_FILE_VERSION: u32 = 1;
/// File extension of load case files.
pub const LOAD_CASE_FILE_EXTENSION: &str = "olslc";

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
    /// [`WEATHER_CASE_TAG`]. Load cases (`.olslc`) add factors and
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
        Ok(())
    }
}

/// A load case file (`.olslc`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadCases {
    /// `"ols-load-cases"` (optional).
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
        ols_core::files::check_header(
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
}

impl CaseLoads {
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
    ols_wirefe::default_fe_budget()
}

/// The structure's axes: (right of the line, ahead) in line axes.
fn axes(orientation_deg: f64) -> ([f64; 2], [f64; 2]) {
    let a = orientation_deg.to_radians();
    ([a.cos(), a.sin()], [-a.sin(), a.cos()])
}

/// Solves every section of `line` (structure types resolved) in each load
/// case and gives the loads on the structures. `weather`: the weather
/// cases the load cases name (also those of the line's creep-stretch
/// criteria). A section that does not solve is listed in `failed`; a load
/// case naming an unknown weather case fails as a whole.
///
/// Expected cost: one finite-element solve per section and load case
/// (~1 ms for 10 spans); `budget` applies to each solve.
pub fn structure_loads(
    line: &Line,
    library: &ols_cables::Library,
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
        let results = ols_line::solve::solve_line_with(&solving, library, w, budget);
        let mut solved = Vec::with_capacity(results.len());
        for (section, result) in line.sections.iter().zip(results) {
            let shape = match result {
                Ok(shape) => shape,
                Err(e) => {
                    failed.push(format!("{}: {e}", section.id));
                    missing.extend(missing_wires(
                        &line,
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
                        &line,
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
            let shape = match ols_line::jumper::solve_one_jumper(&line, library, w, &solved, j) {
                Ok(shape) => shape,
                Err(e) => {
                    failed.push(format!("jumper {}: {e}", j.id));
                    missing.extend(missing_wires(
                        &line,
                        WireKind::Jumper,
                        &j.id,
                        &e.to_string(),
                    ));
                    continue;
                }
            };
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
                        let push =
                            0.5 * w.wind_pressure.si() * string.wind_area * w.wind_from.to_left();
                        f[0] -= a[1] * push;
                        f[1] += a[0] * push;
                    }
                    let place = Place {
                        structure: &support.structure,
                        label: &string.top,
                        at: x.top,
                    };
                    add(&mut at, &line, case, place, f, &[], &j.id);
                }
            }
        }
        out.push(CaseLoads {
            case: case.clone(),
            loads: at.into_values().collect(),
            failed,
            missing,
        });
    }
    Ok(out)
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
/// band — the load case's wind pressure × drag × area, across the line
/// (the wind side of the weather case), at the band's mid-height on the
/// structure's centre line — without a point, labelled "wind on the
/// structure". Factored by the load case's transverse factor. Height
/// extensions are not added to the bands.
pub fn add_structure_wind(
    results: &mut [CaseLoads],
    line: &Line,
    types: &ols_structures::StructureLibrary,
    weather: &[WeatherCase],
) {
    for r in results {
        let Some(w) = weather.iter().find(|w| w.name == r.case.weather) else {
            continue;
        };
        let q = w.wind_pressure.si();
        if q <= 0.0 {
            continue;
        }
        for s in &line.structures {
            let Some(bands) = s
                .structure_type
                .as_deref()
                .and_then(|t| types.get(t))
                .and_then(|t| t.wind.as_ref())
            else {
                continue;
            };
            let (right, _) = axes(s.orientation_deg);
            for b in &bands.bands {
                let (from, to) = (b.from.si(), b.to.si());
                // Wind from the right blows towards the left (−x).
                let f = q * b.drag * b.area.si() * w.wind_from.to_left();
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
                    factored: [local[0] * r.case.transverse, 0.0, 0.0],
                });
            }
        }
    }
}

/// The loads as CSV (one row per load case and point; forces in N):
/// `case,structure,point,label,sets,x,y,z,transverse,longitudinal,
/// vertical,factored_transverse,factored_longitudinal,factored_vertical`
/// (`sets` separated by spaces).
pub fn to_csv(results: &[CaseLoads]) -> String {
    let mut out = String::from(
        "case,structure,point,label,sets,x,y,z,transverse,longitudinal,vertical,\
         factored_transverse,factored_longitudinal,factored_vertical\n",
    );
    let quote = ols_core::text::csv_field;
    for r in results {
        for l in &r.loads {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
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
            );
        }
    }
    out
}

#[cfg(test)]
mod tests;
