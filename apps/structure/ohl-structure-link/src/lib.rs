// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
//! - [`string_volumes`]: the volumes on a structure's strings (their
//!   insulator assemblies' shapes) placed on its strings in still air, as
//!   triangles for OHL-Structure's view (the line's placement,
//!   `ohl_clearance::volumes`).
//! - [`export_structure`]: a level 3 structure with its section library
//!   and load table as plain data for the exports
//!   (`ohl-structure-export`: IFC, SAF, CSV).

mod export;
mod run;
mod volumes;

pub use export::{
    ExportKind, ExportLoad, ExportLoadCase, ExportMaterial, ExportMember, ExportNode,
    ExportSection, ExportStructure, LoadPlace, export_structure, member_axes,
};
pub use run::{
    CheckSummary, PoleInfo, PoleResult, RunInput, SectionUse, StructureCase, StructureRun,
    analyse_structure, cases_to_run, has_model,
};
pub use volumes::{VolumeMesh, string_volumes};

use std::collections::HashMap;
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
/// The one usage classification (over / near / pass, a usage that is not
/// a number over) of the line's checks, for the structure application's
/// colours and reports.
pub use ohl_structures_fe::check::{NEAR_FULL, UsageClass, classify_usage, usage_order};
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

/// The cases the line sends to the placed structures: its **line
/// results** (owner, 2026-10-25): each weather case chosen, unfactored,
/// the wires in the chosen condition; a weather case with wind twice,
/// the wind from the left and from the right ([`line_result_name`]). The
/// factors are the structure load cases' (one place for them); the line's
/// `.ohllc` load cases are not sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureCases {
    /// The line results, as load cases of factor 1 (in the order of the
    /// line's weather cases, the left before the right).
    pub weather: Vec<LoadCase>,
    /// Their weather cases, each named as its result (the wind on its
    /// side).
    pub weather_cases: Vec<ohl_weather::WeatherCase>,
    /// What was chosen and is not there (a weather case since removed),
    /// in words.
    pub missing: Vec<String>,
}

impl StructureCases {
    /// No case to send.
    pub fn is_empty(&self) -> bool {
        self.weather.is_empty()
    }
}

/// The name of a line result: weather case `weather` with the wind from
/// `side` (none: a weather case without wind, its own name).
pub fn line_result_name(weather: &str, side: Option<ohl_weather::WindSide>) -> String {
    match side {
        None => weather.to_owned(),
        Some(ohl_weather::WindSide::Left) => format!("{weather} — wind from the left"),
        Some(ohl_weather::WindSide::Right) => format!("{weather} — wind from the right"),
    }
}

/// The name of the line result of a broken wire: weather case `weather`
/// with `broken` (`high wind — S3 span 2 broken`), the wind from `side`
/// ([`line_result_name`]).
pub fn broken_result_name(
    weather: &str,
    broken: &ohl_loads::BrokenWire,
    side: Option<ohl_weather::WindSide>,
) -> String {
    line_result_name(&format!("{weather} — {}", broken.describe()), side)
}

/// [`structure_cases`] with the line's load cases `load_cases`: then also
/// the broken-wire cases `choice` sends ([`StructureLoads::broken_cases`],
/// roadmap O9), after the weather cases — each a line result of its
/// weather case with its wire broken ([`broken_result_name`]), unfactored
/// (every factor 1), the wires in `choice`'s condition, a wind case twice
/// (the wind from the left and from the right). A name chosen that is not
/// a load case with a broken wire, or whose weather case is not there, is
/// listed in [`StructureCases::missing`].
pub fn structure_cases_with(
    choice: &StructureLoads,
    weather: &[ohl_weather::WeatherCase],
    load_cases: &[LoadCase],
) -> StructureCases {
    let mut out = structure_cases(choice, weather);
    let condition = choice
        .condition
        .as_deref()
        .and_then(ohl_loads::WireCondition::parse)
        .unwrap_or_default();
    for name in &choice.broken_cases {
        let Some((lc, broken)) = load_cases
            .iter()
            .find(|c| &c.name == name)
            .and_then(|c| Some((c, c.broken.as_ref()?)))
        else {
            out.missing
                .push(format!("load case \"{name}\" with a broken wire"));
            continue;
        };
        let Some(w) = weather.iter().find(|w| w.name == lc.weather) else {
            out.missing.push(format!(
                "weather case \"{}\" of load case \"{name}\"",
                lc.weather
            ));
            continue;
        };
        let sides: Vec<Option<ohl_weather::WindSide>> = if w.wind_pressure.si() > 0.0 {
            vec![
                Some(ohl_weather::WindSide::Left),
                Some(ohl_weather::WindSide::Right),
            ]
        } else {
            vec![None]
        };
        for side in sides {
            let result = broken_result_name(&w.name, broken, side);
            let mut case = w.clone();
            case.name = result.clone();
            if let Some(side) = side {
                case.wind_from = side;
            }
            let mut c = LoadCase::from_weather(&result);
            c.name = result.clone();
            c.description = format!(
                "load case \"{name}\": weather case \"{}\"{}, {}, unfactored",
                w.name,
                side.map_or(String::new(), |s| format!(
                    ", the wind from the {}",
                    s.as_str()
                )),
                broken.describe()
            );
            c.condition = condition;
            c.broken = Some(broken.clone());
            c.tags.push(BROKEN_WIRE_TAG.to_owned());
            out.weather.push(c);
            out.weather_cases.push(case);
        }
    }
    out
}

/// The tag of a line result with a broken wire.
pub const BROKEN_WIRE_TAG: &str = "broken wire";

/// The line results ([`StructureCases`]) of the weather cases `choice`
/// sends, of `weather` (the line's weather cases, in their order), the
/// wires in `choice`'s condition (as strung by default). A chosen name
/// not there is listed in [`StructureCases::missing`]. The broken-wire
/// cases need the line's load cases: [`structure_cases_with`].
pub fn structure_cases(
    choice: &StructureLoads,
    weather: &[ohl_weather::WeatherCase],
) -> StructureCases {
    let condition = choice
        .condition
        .as_deref()
        .and_then(ohl_loads::WireCondition::parse)
        .unwrap_or_default();
    let missing = choice
        .weather_cases
        .iter()
        .filter(|n| !weather.iter().any(|w| &w.name == *n))
        .map(|n| format!("weather case \"{n}\""))
        .collect();
    let mut out = StructureCases {
        missing,
        ..Default::default()
    };
    for w in weather
        .iter()
        .filter(|w| choice.weather_cases.contains(&w.name))
    {
        let sides: Vec<Option<ohl_weather::WindSide>> = if w.wind_pressure.si() > 0.0 {
            vec![
                Some(ohl_weather::WindSide::Left),
                Some(ohl_weather::WindSide::Right),
            ]
        } else {
            vec![None]
        };
        for side in sides {
            let name = line_result_name(&w.name, side);
            let mut case = w.clone();
            case.name = name.clone();
            if let Some(side) = side {
                case.wind_from = side;
            }
            let mut c = LoadCase::from_weather(&name);
            c.name = name.clone();
            c.description = match side {
                Some(_) => format!(
                    "weather case \"{}\", the wind from the {}, unfactored",
                    w.name,
                    side.map_or("", |s| s.as_str())
                ),
                None => format!("weather case \"{}\", unfactored", w.name),
            };
            c.condition = condition;
            out.weather.push(c);
            out.weather_cases.push(case);
        }
    }
    out
}

/// The loads of the line results sent to the placed structures.
#[derive(Debug, Clone, Default)]
pub struct StructureCaseLoads {
    /// Per line result.
    pub weather: Vec<CaseLoads>,
    /// What was chosen and is not there ([`StructureCases::missing`]).
    pub missing: Vec<String>,
}

/// The structure loads (`ohl_loads::structure_loads`, every case solved
/// in one go) of the line results `cases` on `line` with its cables
/// `library`. Expected cost: that of the structure loads, about one solve
/// of every section per line result (a wind case counts twice; seconds
/// for a long line), bounded by `budget` per section.
pub fn structure_case_loads(
    line: &Line,
    library: &ohl_cables::Library,
    cases: StructureCases,
    budget: &Budget,
) -> Result<StructureCaseLoads, String> {
    let weather =
        ohl_loads::structure_loads(line, library, &cases.weather_cases, &cases.weather, budget)?;
    Ok(StructureCaseLoads {
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
    // A broken wire in every span: a case per break, as the structure's
    // own checks combine the components (the line's envelope is for its
    // allowable-loads check only; physics review 2026-10-26).
    let split =
        |c: &[CaseLoads]| -> Vec<CaseLoads> { c.iter().flat_map(CaseLoads::each_break).collect() };
    let (loads, weather) = (split(loads), split(weather));
    let sources: Vec<CaseSource> = std::iter::repeat_n(CaseSource::LoadCase, loads.len())
        .chain(std::iter::repeat_n(CaseSource::WeatherCase, weather.len()))
        .collect();
    let loads: Vec<CaseLoads> = loads.into_iter().chain(weather).collect();
    let loads = loads.as_slice();
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
///
/// The wind on the body of a case's loads on the structure itself is the
/// member wind of the line's own structure checks
/// ([`ohl_structures_fe::wind::MemberWindSetup`] with the library's
/// `[wind]` factors: faces, solidity, shielding; the faces found once);
/// a member's ice takes its profile's outline
/// ([`ohl_structures_fe::drawn::outline`]: its full perimeter) and its
/// length.
pub struct SectionAreas {
    model: Result<Model, String>,
    wind: Option<ohl_sections::StructureWind>,
    faces: std::cell::OnceCell<Vec<ohl_structures_fe::wind::Face>>,
}

impl SectionAreas {
    /// The areas of `structure` (`text`: its file's text, `dir`: its
    /// folder, for a `model = "….ohlfe"` it names) with `sections` (and
    /// its wind factors).
    pub fn new(
        structure: &StructureType,
        text: &str,
        dir: Option<&Path>,
        sections: &SectionLibrary,
    ) -> Self {
        Self {
            model: model_of(structure, text, dir, sections),
            wind: sections.wind.clone(),
            faces: std::cell::OnceCell::new(),
        }
    }

    fn model(&self, what: &str) -> Result<&Model, String> {
        self.model
            .as_ref()
            .map_err(|e| format!("{what}: the model does not build ({e})"))
    }

    fn member(&self, id: &str, what: &str) -> Result<(&Model, usize), String> {
        let model = self.model(what)?;
        let k = model
            .members
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| format!("{what}: no member \"{id}\" in the model"))?;
        Ok((model, k))
    }
}

/// The perimeter of a profile's outline (its outer edge, an angle's
/// inside faces too: [`ohl_structures_fe::drawn::outline`]), m.
pub fn outline_perimeter(profile: &ohl_sections::Section) -> f64 {
    let o = ohl_structures_fe::drawn::outline(&profile.shape);
    (0..o.len())
        .map(|k| {
            let (a, b) = (o[k], o[(k + 1) % o.len()]);
            (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .sum()
}

impl MemberAreas for SectionAreas {
    fn area(&self, id: &str) -> Option<f64> {
        self.area_along(id, Direction::Right).ok()
    }

    fn member_ids(&self) -> Option<Vec<String>> {
        let model = self.model.as_ref().ok()?;
        Some(model.members.iter().map(|m| m.id.clone()).collect())
    }

    fn member_wind(
        &self,
        pressure: f64,
        towards: [f64; 3],
        widened: &dyn Fn(&str) -> f64,
    ) -> Result<ohl_structure_loads::BodyWind, String> {
        let model = self.model("no member wind")?;
        let faces = self.faces.get_or_init(|| {
            if ohl_structures_fe::wind::needs_faces(self.wind.as_ref()) {
                ohl_structures_fe::wind::faces(model)
            } else {
                Vec::new()
            }
        });
        let setup = ohl_structures_fe::wind::MemberWindSetup::with_faces(
            model,
            self.wind.as_ref(),
            faces.clone(),
        );
        let extra: Vec<f64> = model.members.iter().map(|m| widened(&m.id)).collect();
        // Table axes (vertical down) → structure axes (z up), and back.
        let w = setup.wind_iced(pressure, [towards[0], towards[1], -towards[2]], &extra);
        let mut row_of = vec![None; model.members.len()];
        for (row, r) in w.members.iter().enumerate() {
            row_of[r.member] = Some(row);
        }
        let members = model
            .members
            .iter()
            .enumerate()
            .map(|(k, m)| {
                let section = &model.sections[m.section];
                let force = if section.profile.is_none() {
                    Err(format!(
                        "no area: cross-section \"{}\" is not in the cross-section library",
                        section.id
                    ))
                } else {
                    // A member along the wind has no row there: no force.
                    Ok(row_of[k].map_or([0.0; 3], |row| {
                        let f = w.member_force(row);
                        [f[0], f[1], -f[2]]
                    }))
                };
                (m.id.clone(), force)
            })
            .collect();
        Ok(ohl_structure_loads::BodyWind {
            members,
            notes: w.notes,
        })
    }

    fn perimeter(&self, id: &str) -> Result<f64, String> {
        let (model, k) = self.member(id, "no outline")?;
        let section = &model.sections[model.members[k].section];
        section
            .profile
            .as_ref()
            .map(outline_perimeter)
            .ok_or_else(|| {
                format!(
                    "no outline: cross-section \"{}\" is not in the cross-section library",
                    section.id
                )
            })
    }

    fn length(&self, id: &str) -> Result<f64, String> {
        let (model, k) = self.member(id, "no length")?;
        Ok(model.length(&model.members[k]))
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
            format!("no area: cross-section \"{s}\" is not in the cross-section library")
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
    // Joints and members by id (the first of an id, as a search finds
    // it): a search per row was slow on a large lattice (2026-10-26).
    let mut node_of: HashMap<&str, usize> = HashMap::with_capacity(model.nodes.len());
    for (k, n) in model.nodes.iter().enumerate() {
        node_of.entry(n.id.as_str()).or_insert(k);
    }
    let mut member_of: HashMap<&str, usize> = HashMap::with_capacity(model.members.len());
    for (k, m) in model.members.iter().enumerate() {
        member_of.entry(m.id.as_str()).or_insert(k);
    }
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
                let n = node_of
                    .get(r.label.as_str())
                    .copied()
                    .ok_or_else(|| format!("no joint \"{}\" in the model", r.label))?;
                nodal.push(at(n, r.force, 1.0));
            }
            At::Member => {
                let m = member_of
                    .get(r.label.as_str())
                    .map(|&k| &model.members[k])
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
    /// The moment of each attachment point's offset (a rigid link from its
    /// joint, 2026-10-21): the points at an offset only, in the order of
    /// the load table.
    pub points: Vec<PointMoment>,
}

/// The moment of an attachment point's offset in one case: r × F at its
/// joint ([`ohl_structures_fe::PointLoad`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PointMoment {
    /// The attachment point's label.
    pub label: String,
    /// The moment, N·m, structure axes (z up): the sum over the point's
    /// rows of the load table.
    pub moment: [f64; 3],
    /// Applied at its joint (a frame member carries it); false: left out,
    /// only trusses meet the joint ([`PointMoment::note`]).
    pub applied: bool,
    /// What was left out, in words (none: applied).
    pub note: Option<String>,
}

impl PointMoment {
    /// Its size, N·m.
    pub fn size(&self) -> f64 {
        let m = self.moment;
        (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt()
    }
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

    /// The member using most of its capacity (a usage that is not a
    /// number first, never hidden).
    pub fn governing(&self) -> Option<&MemberResult> {
        self.members
            .iter()
            .filter(|m| m.usage.is_some())
            .max_by(|a, b| {
                ohl_structures_fe::check::usage_order(
                    a.usage.unwrap_or(0.0),
                    b.usage.unwrap_or(0.0),
                )
            })
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
    Ok(in_case_order(cases.len(), threads(cases.len()), |k| {
        let c = &cases[k];
        (c.name.clone(), analyse_one(&model, table, c, &[], budget))
    }))
}

/// Threads for `n` cases: the cores, at most one per case.
pub(crate) fn threads(n: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, |c| c.get())
        .min(n)
        .max(1)
}

/// `f` of 0..n on `threads` threads (each takes the next case when done),
/// the results in case order (2026-10-26: the cases are independent, each
/// solve the same whichever thread does it).
pub(crate) fn in_case_order<T: Send>(
    n: usize,
    threads: usize,
    f: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    if threads <= 1 || n <= 1 {
        return (0..n).map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done: std::sync::Mutex<Vec<(usize, T)>> = std::sync::Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if k >= n {
                        return;
                    }
                    let r = f(k);
                    done.lock().expect("not poisoned").push((k, r));
                }
            });
        }
    });
    let mut done = done.into_inner().expect("not poisoned");
    done.sort_by_key(|(k, _)| *k);
    done.into_iter().map(|(_, r)| r).collect()
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
    // The moment of each attachment point's offset (a rigid link to its
    // joint), applied or left out (2026-10-21).
    let mut points: Vec<PointMoment> = Vec::new();
    for r in table
        .case(&case.name)
        .filter(|r| r.missing.is_none() && r.at == At::Point)
    {
        let Some(at) = model.point_load(&r.label, [r.force[0], r.force[1], -r.force[2]]) else {
            continue;
        };
        if at.moment != [0.0; 3] {
            match points.iter_mut().find(|p| p.label == r.label) {
                Some(p) => {
                    for (s, m) in p.moment.iter_mut().zip(at.moment) {
                        *s += m;
                    }
                    p.applied &= at.moment_applied;
                    if p.note.is_none() {
                        p.note.clone_from(&at.note);
                    }
                }
                None => points.push(PointMoment {
                    label: r.label.clone(),
                    moment: at.moment,
                    applied: at.moment_applied,
                    note: at.note.clone(),
                }),
            }
        }
        match at.note {
            Some(n) => notes.push(n),
            None if at.moment_applied => {
                let m = at.moment;
                notes.push(format!(
                    "attachment point {}: the moment of its offset ({:.2}, {:.2}, {:.2}) kN m \
                     applied at its joint",
                    r.label,
                    m[0] / 1000.0,
                    m[1] / 1000.0,
                    m[2] / 1000.0
                ));
            }
            None => {}
        }
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
    if let Some(note) = ohl_structures_fe::convergence_note(&solution) {
        notes.push(note);
    }
    notes.push(ohl_structures_fe::check::capacity_note(model, &usage));
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
    // Each member's usage (the first given for it), by member: a search
    // per member was O(members²) (2026-10-26).
    let mut usage_of: Vec<Option<&_>> = vec![None; model.members.len()];
    for u in &usage {
        if let Some(slot) = usage_of.get_mut(u.member)
            && slot.is_none()
        {
            *slot = Some(u);
        }
    }
    let members = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let u = usage_of[k];
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
        points,
    })
}

#[cfg(test)]
mod order_tests {
    /// The cases solved side by side come back in their order, each the
    /// result of its own index, whatever the threads.
    #[test]
    fn cases_come_back_in_their_order() {
        let f = |k: usize| (k, (k as f64).sqrt().to_bits());
        let one: Vec<_> = super::in_case_order(97, 1, f);
        for threads in [2, 4, 16] {
            assert_eq!(super::in_case_order(97, threads, f), one);
        }
        assert!(super::in_case_order(0, 4, f).is_empty());
    }
}
