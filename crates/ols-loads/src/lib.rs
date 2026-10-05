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
pub const LOAD_CASE_FORMAT: &str = "ols-load-cases";
/// Newest format version read.
pub const LOAD_CASE_VERSION: u32 = 1;

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

impl LoadCase {
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
            LOAD_CASE_FORMAT,
            LOAD_CASE_VERSION,
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

/// The load at one support of an attachment point in a load case.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentLoad {
    /// Structure.
    pub structure: String,
    /// Set.
    pub set: u32,
    /// Phase.
    pub phase: u32,
    /// The attachment's label.
    pub label: String,
    /// Sections hanging there.
    pub sections: Vec<String>,
    /// The support (0; 0 and 1: the legs of a V-string).
    pub support: usize,
    /// Where (insulator top, or the clamp), line coordinates.
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
    /// Loads at the attachments, by structure, set, phase, support.
    pub loads: Vec<AttachmentLoad>,
    /// Sections not solved, with why (their loads are missing).
    pub failed: Vec<String>,
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
        // (structure, set, phase, support) → load.
        let mut at: BTreeMap<(String, u32, u32, usize), AttachmentLoad> = BTreeMap::new();
        let mut failed = Vec::new();
        for section in &line.sections {
            let Some(cable) = library.get(&section.cable) else {
                failed.push(format!(
                    "{}: cable {} not in the wire library",
                    section.id, section.cable
                ));
                continue;
            };
            let mut s = section.clone();
            s.calculation = calculation;
            let shape = match ols_line::solve::solve_section(&line, &s, cable, w, budget) {
                Ok(shape) => shape,
                Err(e) => {
                    failed.push(format!("{}: {e}", section.id));
                    continue;
                }
            };
            for (point, supports) in section.points.iter().zip(&shape.attachment_loads) {
                let structure = line.structure(&point.structure);
                let (right, ahead) = axes(structure.map_or(0.0, |s| s.orientation_deg));
                let label = structure
                    .and_then(|s| {
                        s.attachments
                            .iter()
                            .find(|a| a.set == point.set && a.phase == point.phase)
                    })
                    .map_or(String::new(), |a| a.label.clone());
                for (k, (p, f)) in supports.iter().enumerate() {
                    let e = at
                        .entry((point.structure.clone(), point.set, point.phase, k))
                        .or_insert_with(|| AttachmentLoad {
                            structure: point.structure.clone(),
                            set: point.set,
                            phase: point.phase,
                            label: label.clone(),
                            sections: Vec::new(),
                            support: k,
                            at: *p,
                            force: [0.0; 3],
                            local: [0.0; 3],
                            factored: [0.0; 3],
                        });
                    if !e.sections.contains(&section.id) {
                        e.sections.push(section.id.clone());
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
            }
        }
        out.push(CaseLoads {
            case: case.clone(),
            loads: at.into_values().collect(),
            failed,
        });
    }
    Ok(out)
}

/// Adds the wind on the structures themselves to `results`: for every
/// structure whose type gives wind bands (`[[wind.band]]`), one load per
/// band — the load case's wind pressure × drag × area, across the line
/// (the wind side of the weather case), at the band's mid-height on the
/// structure's centre line — as set 0, phase 0, labelled "wind on the
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
            for (k, b) in bands.bands.iter().enumerate() {
                let (from, to) = (b.from.si(), b.to.si());
                // Wind from the right blows towards the left (−x).
                let f = q * b.drag * b.area.si() * w.wind_from.to_left();
                let local = [-f, 0.0, 0.0];
                r.loads.push(AttachmentLoad {
                    structure: s.id.clone(),
                    set: 0,
                    phase: 0,
                    label: format!("wind on the structure ({from}–{to} m)"),
                    sections: Vec::new(),
                    support: k,
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

/// The loads as CSV (one row per load case and support; forces in N):
/// `case,structure,set,phase,label,support,x,y,z,transverse,longitudinal,
/// vertical,factored_transverse,factored_longitudinal,factored_vertical`.
pub fn to_csv(results: &[CaseLoads]) -> String {
    let mut out = String::from(
        "case,structure,set,phase,label,support,x,y,z,transverse,longitudinal,vertical,\
         factored_transverse,factored_longitudinal,factored_vertical\n",
    );
    let quote = ols_core::text::csv_field;
    for r in results {
        for l in &r.loads {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
                quote(&r.case.name),
                quote(&l.structure),
                l.set,
                l.phase,
                quote(&l.label),
                l.support,
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
