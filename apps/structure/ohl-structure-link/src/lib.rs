// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where the structure application meets the core: the one crate that
//! depends on both. The application's crates (`ohl-structure-loads`, …)
//! take and give plain data; this crate turns the core's results into
//! that data and back.
//!
//! - [`line_values`]: the loads the line puts on a placed structure
//!   (`ohl_loads::structure_loads`) as [`LineValues`].
//! - [`fe_loads`]: one case of a [`LoadTable`] as the loads of the
//!   structure's finite-element model (`ohl_structures_fe`).
//! - [`structure_cases`]: the cases the line sends to the placed
//!   structures (its `structure_loads` setting): the load cases chosen,
//!   then the weather cases chosen on their own
//!   (`ohl_loads::LoadCase::from_weather`, all factors 1);
//!   [`structure_case_loads`] computes their loads.
//! - [`placed_values`]: the same with the placed structure's line, its
//!   type and the other places of the type, as the line-values file the
//!   line application hands to the structure application, each case
//!   marked with its source ([`CaseSource`]) and, per case, the wires at
//!   the structure that did not solve ([`missing_wires`]).
//! - [`analyse_cases`]: cases of a [`LoadTable`] solved on the
//!   structure's model, the results as plain data ([`CaseResult`]);
//!   [`analyse_structure`]: the same on an extension, with the pole
//!   check ([`StructureRun`]: what the results panel and the structure
//!   report show).
//! - [`SectionAreas`]: the members' areas from the section library, for
//!   the surface loads of the recipes
//!   ([`ohl_structure_loads::evaluate_with_areas`]); [`section_library`]
//!   reads it.
//! - [`export_structure`]: a level 3 structure with its section library
//!   and load table as plain data for the exports
//!   (`ohl-structure-export`: IFC, SAF, CSV).

mod export;
mod run;

pub use export::{
    ExportKind, ExportLoad, ExportLoadCase, ExportMaterial, ExportMember, ExportNode,
    ExportSection, ExportStructure, LoadPlace, export_structure, member_axes,
};
pub use run::{
    CheckSummary, PoleInfo, PoleResult, RunInput, SectionUse, StructureCase, StructureRun,
    analyse_structure, cases_to_run, has_model,
};

use std::path::Path;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::{Line, StructureLoads};
use ohl_loads::{CaseLoads, LoadCase};
pub use ohl_sections::SectionLibrary;
use ohl_structure_loads::{
    At, CaseSource, CaseWind, Direction, LinePoint, LineQuantity, LineValue, LineValues, LoadTable,
    MemberAreas, MissingWire, OtherPlace, PlacedValues,
};
use ohl_structures::StructureType;
use ohl_structures_fe::{Loads, Model, NodalLoad};

/// The values the line gives at placed structure `structure`: per point,
/// its sets and phases (the phase labels of the sections hanging there)
/// and, per load case, the wires' force unfactored and factored
/// (transverse, longitudinal, vertical down), and the wind on the
/// structure's own body (`ohl_loads::CaseLoads::structure_wind`: pressure,
/// factor, side — the case's `[case.structure]` values or its weather
/// case's pressure and transverse factor). The wind bands' loads (no
/// point) are left out: the structure application puts the body wind on
/// the members; tensions per span, wind on the wires and ice on their own
/// are not given by the line's structure loads yet (the recipes using
/// them get "no line value").
pub fn line_values(loads: &[CaseLoads], line: &Line, structure: &str) -> LineValues {
    let mut out = LineValues::default();
    for case in loads {
        let w = case.structure_wind;
        out.structure_wind.push(CaseWind {
            case: case.case.name.clone(),
            pressure: w.pressure,
            factor: w.factor,
            towards: if w.towards < 0.0 {
                Direction::Left
            } else {
                Direction::Right
            },
            origin: w.origin().to_owned(),
        });
        for l in case
            .loads
            .iter()
            .filter(|l| l.structure == structure && !l.point.is_empty())
        {
            if !out.points.iter().any(|p| p.label == l.point) {
                let mut phases: Vec<String> = Vec::new();
                for s in &l.sections {
                    if let Some(sec) = line.sections.iter().find(|x| &x.id == s)
                        && !sec.phase_label.is_empty()
                        && !phases.contains(&sec.phase_label)
                    {
                        phases.push(sec.phase_label.clone());
                    }
                }
                out.points.push(LinePoint {
                    label: l.point.clone(),
                    sets: l.sets.clone(),
                    phases,
                });
            }
            let quantities = [
                (LineQuantity::Transverse, l.local[0]),
                (LineQuantity::Longitudinal, l.local[1]),
                (LineQuantity::Vertical, l.local[2]),
                (LineQuantity::TransverseFactored, l.factored[0]),
                (LineQuantity::LongitudinalFactored, l.factored[1]),
                (LineQuantity::VerticalFactored, l.factored[2]),
            ];
            for (quantity, value) in quantities {
                out.values.push(LineValue {
                    case: case.case.name.clone(),
                    point: Some(l.point.clone()),
                    set: None,
                    quantity,
                    value,
                });
            }
        }
    }
    out
}

/// The cases the line sends to the placed structures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureCases {
    /// The load cases chosen, in the file's order.
    pub load: Vec<LoadCase>,
    /// The weather cases chosen, each on its own (all factors 1), in the
    /// order of the line's weather cases.
    pub weather: Vec<LoadCase>,
    /// What was chosen and is not there (a load case or weather case
    /// since removed), in words.
    pub missing: Vec<String>,
}

impl StructureCases {
    /// No case to send.
    pub fn is_empty(&self) -> bool {
        self.load.is_empty() && self.weather.is_empty()
    }
}

/// The cases the line sends to the placed structures as `choice` says
/// (the line's `structure_loads`): of `load_cases` (the line's `.ohllc`)
/// those chosen (all by default), then each weather case chosen on its own
/// ([`LoadCase::from_weather`]: `weather: <name>`, all factors 1; named
/// `weather case: <name>` if a load case has that name), in the order of
/// `weather` (the names of the line's weather cases). Chosen names not
/// there are listed in [`StructureCases::missing`].
pub fn structure_cases(
    load_cases: &[LoadCase],
    choice: &StructureLoads,
    weather: &[String],
) -> StructureCases {
    let load: Vec<LoadCase> = load_cases
        .iter()
        .filter(|c| choice.sends_load_case(&c.name))
        .cloned()
        .collect();
    let mut missing: Vec<String> = choice
        .load_cases
        .iter()
        .flatten()
        .filter(|n| !load_cases.iter().any(|c| &c.name == *n))
        .map(|n| format!("load case \"{n}\""))
        .collect();
    missing.extend(
        choice
            .weather_cases
            .iter()
            .filter(|n| !weather.contains(n))
            .map(|n| format!("weather case \"{n}\"")),
    );
    let weather = weather
        .iter()
        .filter(|w| choice.weather_cases.contains(w))
        .map(|w| {
            let mut c = LoadCase::from_weather(w);
            if load.iter().any(|l| l.name == c.name) {
                c.name = format!("weather case: {w}");
            }
            c
        })
        .collect();
    StructureCases {
        load,
        weather,
        missing,
    }
}

/// The loads of the cases sent to the placed structures: of the load
/// cases, then of the weather cases on their own.
#[derive(Debug, Clone, Default)]
pub struct StructureCaseLoads {
    /// Per load case sent.
    pub load: Vec<CaseLoads>,
    /// Per weather case sent on its own.
    pub weather: Vec<CaseLoads>,
    /// What was chosen and is not there ([`StructureCases::missing`]).
    pub missing: Vec<String>,
}

/// The structure loads (`ohl_loads::structure_loads`, every case solved
/// in one go) of `cases` on `line` with its cables `library` and weather
/// cases `weather`. Expected cost: that of the structure loads, about one
/// solve of every section per case (seconds for a long line), bounded by
/// `budget` per section.
pub fn structure_case_loads(
    line: &Line,
    library: &ohl_cables::Library,
    weather: &[ohl_weather::WeatherCase],
    cases: StructureCases,
    budget: &Budget,
) -> Result<StructureCaseLoads, String> {
    let n = cases.load.len();
    let all: Vec<LoadCase> = cases.load.into_iter().chain(cases.weather).collect();
    let mut load = ohl_loads::structure_loads(line, library, weather, &all, budget)?;
    let weather = load.split_off(n);
    Ok(StructureCaseLoads {
        load,
        weather,
        missing: cases.missing,
    })
}

/// The line-values file of placed structure `structure` in line
/// `line_name`: its values ([`line_values`]) in the load cases `loads`,
/// then in the weather cases sent on their own `weather` (sources marked),
/// its structure type and the other placed structures of that type with
/// their values; the note names the sections a case did not solve and
/// what was chosen and is not there (`missing`), and per case the
/// sections and jumpers at the structure that did not solve are listed
/// ([`PlacedValues::missing`], [`missing_wires`]): those cases' values
/// are incomplete.
pub fn placed_values(
    loads: &[CaseLoads],
    weather: &[CaseLoads],
    missing: &[String],
    line: &Line,
    structure: &str,
    line_name: &str,
) -> PlacedValues {
    let sources: Vec<CaseSource> = std::iter::repeat_n(CaseSource::LoadCase, loads.len())
        .chain(std::iter::repeat_n(CaseSource::WeatherCase, weather.len()))
        .collect();
    let all: Vec<CaseLoads>;
    let loads = if weather.is_empty() {
        loads
    } else {
        all = loads.iter().chain(weather).cloned().collect();
        &all
    };
    let structure_type = line
        .structures
        .iter()
        .find(|s| s.id == structure)
        .and_then(|s| s.structure_type.clone())
        .unwrap_or_default();
    let others = line
        .structures
        .iter()
        .filter(|s| {
            s.id != structure
                && !structure_type.is_empty()
                && s.structure_type.as_deref() == Some(structure_type.as_str())
        })
        .map(|s| OtherPlace {
            structure: s.id.clone(),
            values: line_values(loads, line, &s.id),
            missing: missing_wires(loads, &s.id),
        })
        .collect();
    // Sections a case did not solve: their loads are missing.
    let mut failed: Vec<String> = loads
        .iter()
        .filter(|c| !c.failed.is_empty())
        .map(|c| {
            format!(
                "{}: {} section(s) not solved ({})",
                c.case.name,
                c.failed.len(),
                c.failed.join("; ")
            )
        })
        .collect();
    if !missing.is_empty() {
        failed.push(format!(
            "chosen for the structures and not in the line: {}",
            missing.join(", ")
        ));
    }
    PlacedValues {
        cases: loads.iter().map(|c| c.case.name.clone()).collect(),
        sources,
        values: line_values(loads, line, structure),
        others,
        note: (!failed.is_empty()).then(|| format!("values missing — {}", failed.join(" · "))),
        missing: missing_wires(loads, structure),
        ..PlacedValues::pending(structure, line_name, &structure_type, "")
    }
}

/// Per case of `loads`, the sections and jumpers at placed structure
/// `structure` that did not solve (`ohl_loads::CaseLoads::missing`): that
/// case's values at it lack their forces.
pub fn missing_wires(loads: &[CaseLoads], structure: &str) -> Vec<MissingWire> {
    loads
        .iter()
        .flat_map(|c| {
            c.missing_at(structure).map(|m| MissingWire {
                case: c.case.name.clone(),
                kind: m.kind.as_str().to_owned(),
                id: m.id.clone(),
                reason: m.reason.clone(),
            })
        })
        .collect()
}

/// The section library at `path` (a `.ohlsec` file, or every one of a
/// folder); a folder that is not there is an empty library.
pub fn section_library(path: &Path) -> Result<SectionLibrary, String> {
    if !path.exists() {
        return Ok(SectionLibrary::default());
    }
    SectionLibrary::load(path)
}

/// A pole file's (`.ohlp`) id, read and checked as the pole check reads
/// it; or why it does not read (the application's Libraries panel).
pub fn pole_file_id(text: &str) -> Result<String, String> {
    ohl_structures_fe::Pole::parse(text).map(|p| p.id)
}

/// A structure model file's (`.ohlfe`) name, read with the section library
/// `sections` as the finite-element check reads it; or why it does not
/// read (the application's Libraries panel).
pub fn model_file_name(text: &str, sections: &SectionLibrary) -> Result<String, String> {
    Model::parse_with(text, sections).map(|m| m.name)
}

/// The members' areas from the section library: the structure's
/// finite-element model built with it, a member's area for a pressure in
/// a direction its silhouette seen along it
/// ([`Model::projected_area`]: the profile's width for that direction ×
/// the member's length × sin θ). A member whose section is not in the
/// library, or a structure whose model does not build, has no area (the
/// row says why).
pub struct SectionAreas {
    model: Result<Model, String>,
}

impl SectionAreas {
    /// The areas of `structure` (`text`: its file's text, `dir`: its
    /// folder, for a `model = "….ohlfe"` it names) with `sections`.
    pub fn new(
        structure: &StructureType,
        text: &str,
        dir: Option<&Path>,
        sections: &SectionLibrary,
    ) -> Self {
        Self {
            model: model_of(structure, text, dir, sections),
        }
    }
}

impl MemberAreas for SectionAreas {
    fn area(&self, id: &str) -> Option<f64> {
        self.area_along(id, Direction::Right).ok()
    }

    fn area_along(&self, id: &str, direction: Direction) -> Result<f64, String> {
        let model = self
            .model
            .as_ref()
            .map_err(|e| format!("no area: the model does not build ({e})"))?;
        let k = model
            .members
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| format!("no area: no member \"{id}\" in the model"))?;
        // Table axes (vertical down) → structure axes (z up).
        let d = direction.table_vector();
        model.projected_area(k, [d[0], d[1], -d[2]]).ok_or_else(|| {
            let s = &model.sections[model.members[k].section].id;
            format!("no area: section \"{s}\" is not in the section library")
        })
    }
}

/// Case `case` of `table` as the loads of `model` (structure axes, z up):
/// a point's load at the joint carrying it (or shared to the ends of the
/// member it sits on), a joint's at that joint, a
/// member's shared half to each of its joints; rows without a value left
/// out. `self_weight`: the case's factor on the members' weight.
pub fn fe_loads(
    model: &Model,
    table: &LoadTable,
    case: &str,
    self_weight: f64,
) -> Result<Loads, String> {
    let mut nodal = Vec::new();
    let at = |node: usize, f: [f64; 3], share: f64| NodalLoad {
        node,
        force: [f[0] * share, f[1] * share, -f[2] * share],
        moment: [0.0; 3],
    };
    for r in table.case(case).filter(|r| r.missing.is_none()) {
        match r.at {
            At::Point => {
                // At its joint, or shared to the ends of its member.
                let loads = model
                    .point_loads(&r.label, [r.force[0], r.force[1], -r.force[2]])
                    .ok_or_else(|| {
                        format!("point {}: no joint of the model carries it", r.label)
                    })?;
                nodal.extend(loads);
            }
            At::Joint => {
                let n = model
                    .nodes
                    .iter()
                    .position(|n| n.id == r.label)
                    .ok_or_else(|| format!("no joint \"{}\" in the model", r.label))?;
                nodal.push(at(n, r.force, 1.0));
            }
            At::Member => {
                let m = model
                    .members
                    .iter()
                    .find(|m| m.id == r.label)
                    .ok_or_else(|| format!("no member \"{}\" in the model", r.label))?;
                nodal.push(at(m.i, r.force, 0.5));
                nodal.push(at(m.j, r.force, 0.5));
            }
        }
    }
    Ok(Loads { nodal, self_weight })
}

/// A case of the load table to solve: its name, the strength factor of
/// the checks and the factor on the structure's own weight.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseToRun {
    /// The case's name in the table.
    pub name: String,
    /// Strength factor (1: none).
    pub strength: f64,
    /// Factor on the members' weight (0: none).
    pub self_weight: f64,
}

/// A joint of the solved model.
#[derive(Debug, Clone, PartialEq)]
pub struct JointResult {
    /// Id.
    pub id: String,
    /// Where it is, structure axes (z up), m.
    pub point: [f64; 3],
    /// How far it moved, m.
    pub displacement: [f64; 3],
    /// It has a support or a spring.
    pub support: bool,
}

/// A member of the solved model.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberResult {
    /// Id.
    pub id: String,
    /// Its joints (places in [`CaseResult::joints`]).
    pub ends: [usize; 2],
    /// Axial force, N (tension positive).
    pub axial: f64,
    /// Share of its capacity used (1 = 100 %); none: no capacity known.
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
    /// A cable or a tension-only member gone slack.
    pub slack: bool,
    /// A tension-only member bowed: shortened beyond its own buckling
    /// load, it carries that load (`axial`) and no more.
    pub bowed: bool,
}

/// A support's reaction: the force and moment on the structure.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionResult {
    /// The joint.
    pub joint: String,
    /// Force, N, structure axes (z up).
    pub force: [f64; 3],
    /// Moment, N·m.
    pub moment: [f64; 3],
}

/// One case solved: plain data for the application.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseResult {
    /// The case.
    pub case: String,
    /// The joints, with their displacements.
    pub joints: Vec<JointResult>,
    /// The members, with their forces and usage.
    pub members: Vec<MemberResult>,
    /// The reactions.
    pub reactions: Vec<ReactionResult>,
    /// The sum of the loads (the table's and the self-weight), N,
    /// structure axes (z up).
    pub load_sum: [f64; 3],
    /// The sum of the reactions, N (in equilibrium: minus the loads).
    pub reaction_sum: [f64; 3],
    /// Iterations of the solver.
    pub iterations: u64,
    /// Out-of-balance forces at the end, share of the loads.
    pub residual: f64,
    /// P-Delta included.
    pub nonlinear: bool,
    /// How long it took.
    pub took: Duration,
    /// What was left out (rows without a value, moments).
    pub notes: Vec<String>,
}

impl CaseResult {
    /// The joint that moved most, and how far, m.
    pub fn largest_displacement(&self) -> Option<(&str, f64)> {
        self.joints
            .iter()
            .map(|j| {
                let d = j.displacement;
                (
                    j.id.as_str(),
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(),
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The member using most of its capacity.
    pub fn governing(&self) -> Option<&MemberResult> {
        self.members
            .iter()
            .filter(|m| m.usage.is_some())
            .max_by(|a, b| a.usage.unwrap_or(0.0).total_cmp(&b.usage.unwrap_or(0.0)))
    }

    /// The largest out-of-balance of Σ loads + Σ reactions over the three
    /// axes, N.
    pub fn imbalance(&self) -> f64 {
        (0..3)
            .map(|k| (self.load_sum[k] + self.reaction_sum[k]).abs())
            .fold(0.0, f64::max)
    }
}

/// A case's name and its result, or why it did not solve.
pub type CaseRun = (String, Result<CaseResult, String>);

/// The finite-element model of `structure`: its own members, or the
/// `model = "….ohlfe"` it names (beside the file, in `dir`); `text` is the
/// structure file's text. A member's section is looked up in `sections`
/// first.
fn model_of(
    structure: &StructureType,
    text: &str,
    dir: Option<&Path>,
    sections: &SectionLibrary,
) -> Result<Model, String> {
    match &structure.model {
        Some(file) => Model::load_with(&dir.unwrap_or(Path::new(".")).join(file), sections),
        None => Model::from_structure_type_with(text, sections),
    }
}

/// Solves `cases` of `table` on the finite-element model of `structure`
/// (`text`: its file's text, `dir`: its folder; members' sections looked
/// up in `sections` first): per case its result, or why it did not solve.
/// Err: the model does not read.
///
/// Expected cost: a few milliseconds per case for a lattice tower of a
/// few hundred members (one or two iterations first order, a few tens
/// with P-Delta). `budget` bounds each case (iterations, time, cancel).
pub fn analyse_cases(
    structure: &StructureType,
    text: &str,
    dir: Option<&Path>,
    sections: &SectionLibrary,
    table: &LoadTable,
    cases: &[CaseToRun],
    budget: &Budget,
) -> Result<Vec<CaseRun>, String> {
    let model = model_of(structure, text, dir, sections)?;
    Ok(cases
        .iter()
        .map(|c| (c.name.clone(), analyse_one(&model, table, c, &[], budget)))
        .collect())
}

/// One case solved; `extra`: loads added to the table's (the surface
/// loads on an extension's piece).
fn analyse_one(
    model: &Model,
    table: &LoadTable,
    case: &CaseToRun,
    extra: &[NodalLoad],
    budget: &Budget,
) -> Result<CaseResult, String> {
    let self_weight = if model.analysis.self_weight {
        case.self_weight
    } else {
        0.0
    };
    let mut loads = fe_loads(model, table, &case.name, self_weight)?;
    loads.nodal.extend_from_slice(extra);
    let mut notes = Vec::new();
    let left_out = table
        .case(&case.name)
        .filter(|r| r.missing.is_some())
        .count();
    if left_out > 0 {
        notes.push(format!(
            "{left_out} row(s) without a value left out (no line value, no area, …)"
        ));
    }
    if table.case(&case.name).any(|r| r.note.contains("moment")) {
        notes.push("moments of [[load]] are not in the table: left out".into());
    }
    let mut load_sum = [0.0, 0.0, -model.weight() * self_weight];
    for l in &loads.nodal {
        for (s, f) in load_sum.iter_mut().zip(l.force) {
            *s += f;
        }
    }
    let started = Instant::now();
    let (solution, usage) = ohl_structures_fe::analyse(model, &loads, case.strength, budget)
        .map_err(|e| format!("did not solve: {e}"))?;
    let took = started.elapsed();
    if let Some(note) = ohl_structures_fe::stabilisation_note(model, &solution) {
        notes.push(note);
    }
    if let Some(note) = ohl_structures_fe::tension_only_note(model, &solution) {
        notes.push(note);
    }
    // Tension-only members kept active (their set did not settle): the
    // results are approximate; their members' `governs` say so too.
    if let Some(note) = ohl_structures_fe::kept_active_note(model, &solution) {
        notes.push(note);
    }
    let joints = model
        .nodes
        .iter()
        .zip(&solution.displacements)
        .map(|(n, d)| JointResult {
            id: n.id.clone(),
            point: n.point,
            displacement: [d[0], d[1], d[2]],
            support: n.is_support(),
        })
        .collect();
    let members = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let u = usage.iter().find(|u| u.member == k);
            MemberResult {
                id: m.id.clone(),
                ends: [m.i, m.j],
                axial: solution.members[k].axial,
                usage: u.and_then(|u| u.usage),
                governs: u.map(|u| u.governs.clone()).unwrap_or_default(),
                slack: solution.members[k].slack,
                bowed: solution.members[k].bowed,
            }
        })
        .collect();
    let mut reaction_sum = [0.0; 3];
    let reactions = solution
        .reactions
        .iter()
        .map(|r| {
            for (s, f) in reaction_sum.iter_mut().zip(r.force) {
                *s += f;
            }
            ReactionResult {
                joint: model.nodes[r.node].id.clone(),
                force: r.force,
                moment: r.moment,
            }
        })
        .collect();
    Ok(CaseResult {
        case: case.name.clone(),
        joints,
        members,
        reactions,
        load_sum,
        reaction_sum,
        iterations: solution.iterations,
        residual: solution.residual,
        nonlinear: solution.nonlinear,
        took,
        notes,
    })
}
