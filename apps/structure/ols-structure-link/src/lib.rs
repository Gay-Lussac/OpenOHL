// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where the structure application meets the core: the one crate that
//! depends on both. The application's crates (`ols-structure-loads`, …)
//! take and give plain data; this crate turns the core's results into
//! that data and back.
//!
//! - [`line_values`]: the loads the line puts on a placed structure
//!   (`ols_loads::structure_loads`) as [`LineValues`].
//! - [`fe_loads`]: one case of a [`LoadTable`] as the loads of the
//!   structure's finite-element model (`ols_structures_fe`).
//! - [`analyse_cases`]: cases of a [`LoadTable`] solved on the
//!   structure's model, the results as plain data ([`CaseResult`]).
//!
//! Later also the section library (member areas) and saving back into the
//! line.

use std::path::Path;
use std::time::{Duration, Instant};

use ols_core::budget::Budget;
use ols_line::Line;
use ols_loads::CaseLoads;
use ols_structure_loads::{At, LinePoint, LineQuantity, LineValue, LineValues, LoadTable};
use ols_structures::StructureType;
use ols_structures_fe::{Loads, Model, NodalLoad};

/// The values the line gives at placed structure `structure`: per point,
/// its sets and phases (the phase labels of the sections hanging there)
/// and, per load case, the wires' force unfactored and factored
/// (transverse, longitudinal, vertical down). The wind on the structure
/// body (no point) is left out; tensions per span, wind on the wires and
/// ice on their own are not given by the line's structure loads yet (the
/// recipes using them get "no line value").
pub fn line_values(loads: &[CaseLoads], line: &Line, structure: &str) -> LineValues {
    let mut out = LineValues::default();
    for case in loads {
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
    /// A cable gone slack.
    pub slack: bool,
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
/// `model = "….olsfe"` it names (beside the file, in `dir`); `text` is the
/// structure file's text.
fn model_of(structure: &StructureType, text: &str, dir: Option<&Path>) -> Result<Model, String> {
    match &structure.model {
        Some(file) => Model::load(&dir.unwrap_or(Path::new(".")).join(file)),
        None => Model::from_structure_type(text),
    }
}

/// Solves `cases` of `table` on the finite-element model of `structure`
/// (`text`: its file's text, `dir`: its folder): per case its result, or
/// why it did not solve. Err: the model does not read.
///
/// Expected cost: a few milliseconds per case for a lattice tower of a
/// few hundred members (one or two iterations first order, a few tens
/// with P-Delta). `budget` bounds each case (iterations, time, cancel).
pub fn analyse_cases(
    structure: &StructureType,
    text: &str,
    dir: Option<&Path>,
    table: &LoadTable,
    cases: &[CaseToRun],
    budget: &Budget,
) -> Result<Vec<CaseRun>, String> {
    let model = model_of(structure, text, dir)?;
    Ok(cases
        .iter()
        .map(|c| (c.name.clone(), analyse_one(&model, table, c, budget)))
        .collect())
}

fn analyse_one(
    model: &Model,
    table: &LoadTable,
    case: &CaseToRun,
    budget: &Budget,
) -> Result<CaseResult, String> {
    let self_weight = if model.analysis.self_weight {
        case.self_weight
    } else {
        0.0
    };
    let loads = fe_loads(model, table, &case.name, self_weight)?;
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
    let (solution, usage) = ols_structures_fe::analyse(model, &loads, case.strength, budget)
        .map_err(|e| format!("did not solve: {e}"))?;
    let took = started.elapsed();
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
